use std::path::{Path, PathBuf};

use gtk::gio;
use gtk::prelude::*;

pub mod directory_sorter;

/// File operations return a human-readable error string on failure, ready to put
/// straight into an error dialog. (Small surface, so a custom error type would
/// be overkill here.)
pub type Fallible = Result<(), String>;

/// Move a file or folder to the system trash (recoverable). Uses gio so it goes
/// to the same trash your desktop's Files app uses.
pub fn trash(path: &Path) -> Fallible {
    gio::File::for_path(path)
        .trash(gio::Cancellable::NONE)
        .map_err(|e| e.to_string())
}

/// Permanently remove a file, or recursively remove a directory. Irreversible.
pub fn delete_permanent(path: &Path) -> Fallible {
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    result.map_err(|e| e.to_string())
}

/// Rename `path` within its parent directory. `new_name` is a bare file name
/// (no slashes). Refuses to clobber an existing entry.
pub fn rename(path: &Path, new_name: &str) -> Fallible {
    let new_name = new_name.trim();
    if new_name.is_empty() || new_name.contains('/') {
        return Err("Name can't be empty or contain '/'".to_string());
    }
    let parent = path.parent().ok_or("No parent directory")?;
    let dest = parent.join(new_name);
    if dest == path {
        return Ok(()); // unchanged
    }
    if dest.exists() {
        return Err(format!("'{new_name}' already exists"));
    }
    std::fs::rename(path, &dest).map_err(|e| e.to_string())
}

/// Copy `src` into `dest_dir`, keeping its name (or a "(copy)" variant if that
/// would clobber something). Recurses into directories.
pub fn copy_into(src: &Path, dest_dir: &Path) -> Fallible {
    let name = src.file_name().ok_or("Source has no name")?;
    let dest = unique_destination(&dest_dir.join(name));
    copy_recursive(src, &dest).map_err(|e| e.to_string())
}

/// Move `src` into `dest_dir`. Tries a fast rename; if that crosses a filesystem
/// boundary (EXDEV), falls back to copy-then-delete.
pub fn move_into(src: &Path, dest_dir: &Path) -> Fallible {
    let name = src.file_name().ok_or("Source has no name")?;
    let target = dest_dir.join(name);
    if target == src {
        return Ok(()); // already here
    }
    let dest = unique_destination(&target);
    if std::fs::rename(src, &dest).is_ok() {
        return Ok(());
    }
    copy_recursive(src, &dest).map_err(|e| e.to_string())?;
    delete_permanent(src)
}

fn copy_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    if std::fs::symlink_metadata(src)?.file_type().is_dir() {
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dest).map(|_| ())
    }
}

/// `dest` if free, else `dest` with " (copy)" / " (copy N)" inserted before the
/// extension until a free name is found.
fn unique_destination(dest: &Path) -> PathBuf {
    if !dest.exists() {
        return dest.to_path_buf();
    }
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    let stem = dest
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = dest.extension().map(|s| s.to_string_lossy().into_owned());
    for n in 1u32.. {
        let suffix = if n == 1 {
            " (copy)".to_string()
        } else {
            format!(" (copy {n})")
        };
        let name = match &ext {
            Some(ext) => format!("{stem}{suffix}.{ext}"),
            None => format!("{stem}{suffix}"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}
