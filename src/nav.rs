use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Shared, mutable navigation state for a window (current directory + history).
///
/// MEMORY NOTE — this is the one pattern you'll see everywhere in GTK-Rust:
/// `Rc<RefCell<T>>`.
///   * GTK is single-threaded — all UI code runs on one thread — so we never
///     need the thread-safe `Arc<Mutex<T>>`. `Rc` is the cheaper single-thread
///     equivalent.
///   * `Rc<T>` = shared ownership. Cloning it doesn't copy the data; it bumps a
///     reference count and hands back another handle to the *same* `NavState`.
///   * `RefCell<T>` = "interior mutability": it lets us mutate through a shared
///     reference, moving Rust's borrow checking from compile time to runtime
///     (`borrow()` / `borrow_mut()`; double-mutable-borrow panics instead of
///     failing to compile).
///
/// We wrap all that in a newtype `Nav` and derive `Clone`, so callers just clone
/// a `Nav` to share it into closures — every clone points at the same state.
#[derive(Clone)]
pub struct Nav(Rc<RefCell<NavState>>);

struct NavState {
    current: PathBuf,
    back: Vec<PathBuf>,
}

impl Nav {
    pub fn new(start: PathBuf) -> Self {
        Nav(Rc::new(RefCell::new(NavState {
            current: start,
            back: Vec::new(),
        })))
    }

    /// The directory currently being shown. Returns an owned clone so callers
    /// don't hold a `RefCell` borrow open (which would risk a runtime panic).
    pub fn current(&self) -> PathBuf {
        self.0.borrow().current.clone()
    }

    /// Move to `path`, pushing the old directory onto the back-stack.
    pub fn go_to(&self, path: &Path) {
        let mut state = self.0.borrow_mut();
        let previous = std::mem::replace(&mut state.current, path.to_path_buf());
        state.back.push(previous);
    }

    /// Go to the previous directory, if any. Returns the new current dir.
    pub fn go_back(&self) -> Option<PathBuf> {
        let mut state = self.0.borrow_mut();
        let previous = state.back.pop()?;
        state.current = previous.clone();
        Some(previous)
    }
}
