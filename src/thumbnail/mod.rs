use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

use gtk::gio;
use gtk::gio::prelude::*;
use gtk::glib;
use gtk::prelude::*;
use gtk::{gdk, Image};

/// Longest edge (px) of a generated thumbnail. The freedesktop "large" size is
/// 256; we match it so our thumbnails live in `thumbnails/large/` and are shared
/// with other file managers.
const THUMB_SIZE: u32 = 256;

/// Identifier for our per-application `fail/` subdirectory (spec requirement).
const APP_FAIL_DIR: &str = "dalmation";

/// freedesktop size directories. When *reading* we accept a thumbnail from any
/// of these (we only display ~64px, so even the smallest is plenty) — this is
/// what lets us reuse thumbnails another app generated at a different zoom. We
/// always *write* to `large`.
const SIZE_DIRS: [&str; 4] = ["normal", "large", "x-large", "xx-large"];
const WRITE_SIZE_DIR: &str = "large";

/// A finished thumbnail decoded to raw RGBA8 on a worker thread.
///
/// Crucially this is *plain data*: `Vec<u8>` is `Send`, so it can cross the
/// thread boundary. We deliberately do NOT create any GTK/GObject value on a
/// worker thread — those aren't `Send`. The texture is built on the main thread.
struct ThumbData {
    rgba: Vec<u8>,
    width: i32,
    height: i32,
}

/// Everything a worker needs to honour the freedesktop spec for one file. We
/// compute the glib bits (URI, MD5) on the MAIN thread and ship this plain,
/// `Send` struct to the worker — keeping the worker pure-Rust.
struct ThumbJob {
    /// Source file.
    path: PathBuf,
    /// Canonical `file://` URI, stored in the thumbnail's `Thumb::URI` chunk.
    uri: String,
    /// Source mtime (secs since epoch); the validity key (`Thumb::MTime`).
    mtime: u64,
    /// Existing thumbnails to try, across all sizes: `<size>/<md5(uri)>.png`.
    read_candidates: Vec<PathBuf>,
    /// Where we write a freshly generated thumbnail: `large/<md5(uri)>.png`.
    write_path: PathBuf,
    /// `~/.cache/thumbnails/fail/dalmation/<md5(uri)>.png`.
    fail_path: PathBuf,
}

/// Worker -> main message: which path, and the outcome (`None` = not an image or
/// failed to decode).
struct ThumbResult {
    path: PathBuf,
    data: Option<ThumbData>,
}

/// Drives off-thread thumbnail generation and feeds results back into `Image`
/// widgets. Cheap to clone (all shared state lives behind one `Rc`).
#[derive(Clone)]
pub struct Thumbnailer {
    inner: Rc<Inner>,
    /// Main -> workers request queue. `async_channel::Sender` is `Send + Clone`.
    requests: async_channel::Sender<ThumbJob>,
}

/// All the shared, single-threaded state, each field a `RefCell` because the
/// bind closure and the result pump both mutate it (at different times) on the
/// one UI thread — the same `Rc<RefCell<..>>` story as `Nav`.
struct Inner {
    /// path -> ready GPU texture (in-memory; survives navigation within a session).
    cache: RefCell<HashMap<PathBuf, gdk::Texture>>,
    /// paths currently queued/decoding, to avoid enqueuing duplicates on scroll.
    inflight: RefCell<HashSet<PathBuf>>,
    /// paths that failed/aren't images, so we don't keep retrying them this session.
    failed: RefCell<HashSet<PathBuf>>,
    /// path -> the Image cell that last asked for it. WEAK, because grid cells
    /// are recycled and destroyed; a weak ref lets them die without leaking.
    bound: RefCell<HashMap<PathBuf, glib::WeakRef<Image>>>,
}

impl Thumbnailer {
    pub fn new() -> Self {
        let inner = Rc::new(Inner {
            cache: RefCell::new(HashMap::new()),
            inflight: RefCell::new(HashSet::new()),
            failed: RefCell::new(HashSet::new()),
            bound: RefCell::new(HashMap::new()),
        });

        let (req_tx, req_rx) = async_channel::unbounded::<ThumbJob>();
        let (res_tx, res_rx) = async_channel::unbounded::<ThumbResult>();

        // --- Worker pool: pure-Rust decode + resize + cache IO on bg threads. ---
        let worker_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .min(4);
        for _ in 0..worker_count {
            let req_rx = req_rx.clone();
            let res_tx = res_tx.clone();
            std::thread::spawn(move || {
                // `recv_blocking` parks the thread until a request arrives, and
                // returns `Err` once every Sender is dropped (app shutting down).
                while let Ok(job) = req_rx.recv_blocking() {
                    let data = generate(&job);
                    if res_tx
                        .send_blocking(ThumbResult {
                            path: job.path,
                            data,
                        })
                        .is_err()
                    {
                        break; // main side gone
                    }
                }
            });
        }

        // --- Result pump: runs on the MAIN thread via the GLib main loop. ---
        // `spawn_future_local` does NOT require the future to be `Send`, which is
        // what lets us touch `Rc`, `RefCell`, and GTK widgets in here.
        {
            let inner = inner.clone();
            glib::spawn_future_local(async move {
                while let Ok(ThumbResult { path, data }) = res_rx.recv().await {
                    inner.inflight.borrow_mut().remove(&path);

                    let Some(data) = data else {
                        inner.failed.borrow_mut().insert(path);
                        continue;
                    };

                    // Build the texture here, on the main thread.
                    let texture = texture_from_rgba(data);

                    // Update the waiting cell — but only if it still wants this
                    // exact path (cells get recycled to other files as you scroll).
                    if let Some(weak) = inner.bound.borrow_mut().remove(&path) {
                        if let Some(image) = weak.upgrade() {
                            let want = path.to_string_lossy();
                            if image.widget_name().as_str() == want.as_ref() {
                                image.set_paintable(Some(&texture));
                            }
                        }
                    }

                    inner.cache.borrow_mut().insert(path, texture);
                }
            });
        }

        Thumbnailer {
            inner,
            requests: req_tx,
        }
    }

    /// Show a thumbnail for `path` in `image`. Cached → applied immediately;
    /// otherwise the themed icon stays as a placeholder and a background request
    /// is queued (once).
    ///
    /// We stash `path` in the widget's name as a "what do I currently want?"
    /// marker, so a late-arriving result can tell whether this recycled cell has
    /// since moved on to a different file.
    pub fn request(&self, path: &Path, image: &Image) {
        let key = path.to_string_lossy();
        image.set_widget_name(&key);

        // Reading the cache: clone the texture handle (cheap refcount bump) so we
        // don't hold the RefCell borrow while mutating the widget.
        let cached = self.inner.cache.borrow().get(path).cloned();
        if let Some(texture) = cached {
            image.set_paintable(Some(&texture));
            return;
        }

        if self.inner.failed.borrow().contains(path) {
            return; // not an image / known failure: leave the themed icon
        }

        // Remember which cell to update when the result arrives.
        self.inner
            .bound
            .borrow_mut()
            .insert(path.to_path_buf(), image.downgrade());

        // Enqueue at most once per path.
        let already = self.inner.inflight.borrow().contains(path);
        if already {
            return;
        }
        self.inner.inflight.borrow_mut().insert(path.to_path_buf());

        // Build the spec job here (main thread: needs glib for the URI + MD5).
        match ThumbJob::for_path(path) {
            Some(job) => {
                let _ = self.requests.try_send(job);
            }
            None => {
                // Couldn't stat / no cache dir: treat as failed to avoid a loop.
                self.inner.inflight.borrow_mut().remove(path);
                self.inner.failed.borrow_mut().insert(path.to_path_buf());
            }
        }
    }
}

impl ThumbJob {
    /// Resolve the spec paths for a file. Runs on the MAIN thread (uses glib).
    fn for_path(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())?;

        // glib gives us the correctly percent-encoded URI; getting this exactly
        // right is what makes the cache interoperable with other apps.
        let uri = gio::File::for_path(path).uri().to_string();
        let md5 = glib::compute_checksum_for_string(glib::ChecksumType::Md5, uri.as_str())?;

        let base = thumbnails_dir()?;
        let file = format!("{md5}.png");
        Some(ThumbJob {
            path: path.to_path_buf(),
            uri,
            mtime,
            read_candidates: SIZE_DIRS.iter().map(|s| base.join(s).join(&file)).collect(),
            write_path: base.join(WRITE_SIZE_DIR).join(&file),
            fail_path: base.join("fail").join(APP_FAIL_DIR).join(&file),
        })
    }
}

/// Build a GdkTexture from raw RGBA8. MAIN THREAD ONLY (MemoryTexture::new
/// asserts it). `from_owned` moves the `Vec` into the GBytes with no copy.
fn texture_from_rgba(data: ThumbData) -> gdk::Texture {
    let stride = data.width as usize * 4;
    let bytes = glib::Bytes::from_owned(data.rgba);
    gdk::MemoryTexture::new(
        data.width,
        data.height,
        gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        stride,
    )
    .upcast()
}

// ---------------------------------------------------------------------------
// Everything below runs on WORKER THREADS: pure Rust, no GTK/GObject.
// ---------------------------------------------------------------------------

/// Produce RGBA8 for a job: reuse a valid cached thumbnail, honour a recorded
/// failure, or decode the original (and write the result + metadata to disk).
fn generate(job: &ThumbJob) -> Option<ThumbData> {
    // 1) Reuse any existing thumbnail (any size) whose Thumb::MTime matches the
    //    source — this is what shares the cache with other file managers.
    for candidate in &job.read_candidates {
        if read_thumb_mtime(candidate) == Some(job.mtime) {
            if let Some(data) = load_rgba(candidate) {
                return Some(data);
            }
        }
    }

    // 2) A recorded failure (matching mtime) means: don't bother decoding again.
    if read_thumb_mtime(&job.fail_path) == Some(job.mtime) {
        return None;
    }

    // 3) Decode + downscale the original.
    match decode_thumbnail(&job.path, THUMB_SIZE) {
        Some(thumb) => {
            let (w, h) = thumb.dimensions();
            write_png_atomic(&job.write_path, thumb.as_raw(), w, h, &job.uri, job.mtime);
            Some(ThumbData {
                rgba: thumb.into_raw(),
                width: w as i32,
                height: h as i32,
            })
        }
        None => {
            // Record a tiny fail marker so we skip this file next time too.
            write_png_atomic(&job.fail_path, &[0, 0, 0, 0], 1, 1, &job.uri, job.mtime);
            None
        }
    }
}

fn decode_thumbnail(path: &Path, size: u32) -> Option<image::RgbaImage> {
    if !has_image_extension(path) {
        return None;
    }
    let img = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    Some(img.thumbnail(size, size).to_rgba8())
}

/// Decode a cached PNG's pixels to RGBA8 (metadata validated separately).
fn load_rgba(file: &Path) -> Option<ThumbData> {
    let rgba = image::open(file).ok()?.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(ThumbData {
        rgba: rgba.into_raw(),
        width: width as i32,
        height: height as i32,
    })
}

/// Read the `Thumb::MTime` tEXt chunk from a thumbnail/fail PNG, if present.
/// `read_info` stops at the image data, so this never decodes pixels.
fn read_thumb_mtime(png_path: &Path) -> Option<u64> {
    let file = std::fs::File::open(png_path).ok()?;
    let reader = png::Decoder::new(file).read_info().ok()?;
    reader
        .info()
        .uncompressed_latin1_text
        .iter()
        .find(|chunk| chunk.keyword == "Thumb::MTime")
        .and_then(|chunk| chunk.text.trim().parse::<u64>().ok())
}

/// Write a PNG with the `Thumb::URI` / `Thumb::MTime` chunks, atomically: encode
/// into a temp sibling, fix permissions, then rename into place so other apps
/// never observe a half-written file.
fn write_png_atomic(dest: &Path, rgba: &[u8], width: u32, height: u32, uri: &str, mtime: u64) {
    let Some(parent) = dest.parent() else { return };
    ensure_dir(parent);

    let tmp = temp_sibling(dest);
    if encode_png(&tmp, rgba, width, height, uri, mtime).is_ok() {
        set_mode(&tmp, 0o600);
        if std::fs::rename(&tmp, dest).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

fn encode_png(
    path: &Path,
    rgba: &[u8],
    width: u32,
    height: u32,
    uri: &str,
    mtime: u64,
) -> Result<(), png::EncodingError> {
    let file = std::fs::File::create(path)?; // io::Error -> EncodingError
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.add_text_chunk("Thumb::URI".to_string(), uri.to_string())?;
    encoder.add_text_chunk("Thumb::MTime".to_string(), mtime.to_string())?;
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    writer.finish()
}

/// `~/.cache/thumbnails`, from the environment (no glib on workers; this is also
/// called on the main thread by `ThumbJob::for_path`).
fn thumbnails_dir() -> Option<PathBuf> {
    let mut dir = cache_root()?;
    dir.push("thumbnails");
    Some(dir)
}

fn cache_root() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg));
        }
    }
    std::env::var_os("HOME").map(|home| {
        let mut path = PathBuf::from(home);
        path.push(".cache");
        path
    })
}

/// A unique temp path next to `dest`, for atomic rename. Process-unique via pid +
/// a monotonic counter (single process, so that's sufficient).
fn temp_sibling(dest: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let stem = dest
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("thumb");
    let mut tmp = dest.to_path_buf();
    tmp.set_file_name(format!("{stem}.{}.{n}.tmp", std::process::id()));
    tmp
}

fn ensure_dir(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    set_mode(dir, 0o700); // spec: thumbnail dirs are private (0700)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) {}

fn has_image_extension(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    matches!(
        ext.as_deref(),
        Some(
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "tif" | "ico" | "tga"
                | "avif" | "qoi" | "ppm" | "pgm" | "pbm" | "ff"
        )
    )
}
