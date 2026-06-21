use gtk::gio::prelude::*;
use gtk::{gio, glib, CustomSorter};
use std::cmp::Ordering;
use std::path::Path;

use gtk::Ordering as GtkOrdering;

/// gio metadata key under which we persist each directory's sort preference.
/// Stored by the GVfs metadata backend, so it survives across sessions.
const META_KEY: &str = "metadata::dalmation-sort";

/// What to order entries by.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Modified,
}

/// Ascending or descending. Note this only flips the *key* comparison —
/// directories always sort before files regardless (see `compare`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    Desc,
}

/// A complete sort specification: a key plus a direction.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub key: SortKey,
    pub order: SortOrder,
}

impl Default for Sort {
    fn default() -> Self {
        Sort {
            key: SortKey::Name,
            order: SortOrder::Asc,
        }
    }
}

impl Sort {
    /// Encode as the short `"<key>:<order>"` string we store in metadata and use
    /// as the `win.sort` action target.
    pub fn to_meta(self) -> String {
        let key = match self.key {
            SortKey::Name => "name",
            SortKey::Modified => "modified",
        };
        let order = match self.order {
            SortOrder::Asc => "asc",
            SortOrder::Desc => "desc",
        };
        format!("{key}:{order}")
    }

    /// Parse a stored `"<key>:<order>"` string. `None` on anything unexpected,
    /// so callers can fall back to the default.
    pub fn from_meta(s: &str) -> Option<Self> {
        let (key, order) = s.split_once(':')?;
        let key = match key {
            "name" => SortKey::Name,
            "modified" => SortKey::Modified,
            _ => return None,
        };
        let order = match order {
            "asc" => SortOrder::Asc,
            "desc" => SortOrder::Desc,
            _ => return None,
        };
        Some(Sort { key, order })
    }
}

/// Owns the `CustomSorter` that drives the grid's `SortListModel`. Cloning shares
/// the same underlying sorter (it's a GObject handle), so any clone can re-sort.
#[derive(Clone)]
pub struct DirectorySorter {
    sorter: CustomSorter,
}

impl DirectorySorter {
    pub fn new() -> Self {
        let sorter = CustomSorter::new(|a, b| Self::compare(Sort::default(), a, b));
        DirectorySorter { sorter }
    }

    pub fn sorter(&self) -> &CustomSorter {
        &self.sorter
    }

    /// Re-sort by `sort`. Swaps the comparison function on the `CustomSorter`;
    /// the `SortListModel` re-sorts itself in response (the sorter emits
    /// "changed").
    pub fn sort(&self, sort: Sort) {
        self.sorter
            .set_sort_func(move |a, b| Self::compare(sort, a, b));
    }

    /// The one comparator, parameterized by `sort`. Directories always come
    /// first — that's decided *before* and *outside* the asc/desc flip, so
    /// "descending" reverses files-among-files and dirs-among-dirs without ever
    /// floating a file above a folder.
    fn compare(sort: Sort, a: &glib::Object, b: &glib::Object) -> GtkOrdering {
        let a = a.downcast_ref::<gio::FileInfo>().unwrap();
        let b = b.downcast_ref::<gio::FileInfo>().unwrap();

        let a_dir = a.file_type() == gio::FileType::Directory;
        let b_dir = b.file_type() == gio::FileType::Directory;
        match (a_dir, b_dir) {
            (true, false) => return Ordering::Less.into(),
            (false, true) => return Ordering::Greater.into(),
            _ => {}
        }

        let ordering = match sort.key {
            SortKey::Name => a.display_name().cmp(&b.display_name()),
            SortKey::Modified => modified(a).cmp(&modified(b)),
        };
        match sort.order {
            SortOrder::Asc => ordering,
            SortOrder::Desc => ordering.reverse(),
        }
        .into()
    }
}

/// Last-modified time as a raw unix seconds value (0 if missing). Requires the
/// grid's `DirectoryList` to request the `time::modified` attribute.
fn modified(info: &gio::FileInfo) -> u64 {
    info.attribute_uint64("time::modified")
}

/// Read the saved sort for `dir` from its gio metadata, falling back to the
/// default if it's unset or unreadable.
pub fn read_sort(dir: &Path) -> Sort {
    gio::File::for_path(dir)
        .query_info(
            META_KEY,
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .ok()
        .and_then(|info| info.attribute_string(META_KEY))
        .and_then(|s| Sort::from_meta(&s))
        .unwrap_or_default()
}

/// Persist `sort` for `dir` as gio metadata. Best-effort: errors (e.g. a
/// filesystem without a metadata backend) are silently ignored.
pub fn write_sort(dir: &Path, sort: Sort) {
    let _ = gio::File::for_path(dir).set_attribute_string(
        META_KEY,
        &sort.to_meta(),
        gio::FileQueryInfoFlags::NONE,
        gio::Cancellable::NONE,
    );
}
