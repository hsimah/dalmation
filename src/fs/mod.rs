use std::path::Path;

use gtk::gio;
use gtk::prelude::*;

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
