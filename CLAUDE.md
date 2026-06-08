# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

dalmation is a GTK4 file manager for Niri, written in Rust (edition 2024). It uses the `gtk4` crate (imported as `gtk`, feature `v4_22`) for the UI, and a small worker-thread pool for off-thread image thumbnailing.

## Commands

```sh
cargo run         # build + launch the app
cargo build       # debug build
cargo build --release
cargo check       # fast type-check without producing a binary
cargo fmt         # format (rustfmt)
cargo clippy      # lint
```

There is no test suite yet.

> The user builds and runs the app themselves — hand them the command rather than running long-lived GUI processes.

## Architecture

**Single GTK thread, one window.** `main.rs` → `app::build()` creates the `Application` (id `dev.hsimah.Dalmation`), installs CSS on `startup`, and builds a window on `activate`. `ui::window::build_window` assembles the whole UI: a `PathBar` on top, and a horizontal `Paned` splitting a `FileTree` sidebar from a `FileGrid`.

**The shared-handle pattern is everywhere — learn it first.** GTK is single-threaded, so the codebase never uses `Arc`/`Mutex`. Two idioms recur:
- Plain shared mutable state uses `Rc<RefCell<T>>` wrapped in a `#[derive(Clone)]` newtype (see `nav::Nav`, the `Clipboard` type in `commands.rs`, and `thumbnail::Inner`). Cloning the newtype shares the *same* state.
- UI components (`FileGrid`, `PathBar`, `FileTree`) are structs of GTK object handles and also `#[derive(Clone)]`. Cloning gives another handle to the *same* widgets, so they can be moved into signal closures.

`nav.rs` is the canonical commented example of `Rc<RefCell<T>>` — read it before touching shared state.

**Navigation is funneled through two closures** in `build_window`: `show` (update views, no history) and `navigate` (record history then show, dirs only). Every navigation source — path-bar Enter, grid activate, tree activate, Back — calls one of these so the views stay in sync. These closures intentionally form a reference cycle with the widgets; that's fine because the single window lives for the whole app. For *transient* widgets (dialogs, rows), break the cycle with `glib::clone!(#[weak] ...)` — see the rename/properties dialogs in `commands.rs` and `info_dialog.rs`.

**Commands, accelerators, and the context menu share one source of truth** (`ui::commands::install`). Each operation is a `gio::SimpleAction` registered on the window under the `win.` prefix; both `set_accels_for_action` and the `gio::Menu` reference those action names. To add a file operation: add the `fs` function, register an action, then add the accelerator and menu entry pointing at the same `win.<name>`. Copy/cut/paste use an internal in-process `Clipboard` (no system clipboard).

**The grid uses GTK's list-model stack** (`file_grid.rs`): a `DirectoryList` (async, monitored for live updates) → `MultiSelection` → `GridView` with a `SignalListItemFactory`. Cells are recycled (`connect_setup` builds once, `connect_bind` refills), so only on-screen cells exist. Entry paths are always resolved as `dir.child(info.name())` from the `DirectoryList`'s own directory, so the grid needs no knowledge of `Nav`. The tree (`file_tree.rs`) uses the analogous `TreeListModel` stack, wrapping each node's `PathBuf` in a `glib::BoxedAnyObject` and lazily reading subdirectories only when a row expands.

**Thumbnailing crosses the thread boundary carefully** (`thumbnail/mod.rs`). This is the only multi-threaded part. The rules it follows, which any change must preserve:
- Worker threads are **pure Rust only** — never construct a GTK/GObject value off the main thread (they aren't `Send`). Workers decode/resize with the `image` crate and read/write PNG cache files, returning plain `Vec<u8>` RGBA.
- Anything needing glib (the `file://` URI, the MD5 of it) is computed on the **main thread** in `ThumbJob::for_path` and shipped to the worker in a `Send` struct.
- The `GdkTexture` is built on the **main thread** in the result pump (`glib::spawn_future_local`, which doesn't require `Send`).
- Two `async_channel`s carry work: main→workers (drained with `recv_blocking`) and workers→main (awaited on the GLib loop).
- Recycled grid cells are tracked via `image.set_widget_name(path)` plus a `WeakRef`, so a late thumbnail result is only applied if that cell still wants that exact path.

The thumbnail cache is the **freedesktop shared cache** (`~/.cache/thumbnails/{normal,large,...}`), interoperable with other file managers: thumbnails are keyed by `md5(uri)`, validated by the `Thumb::MTime` PNG text chunk, written atomically (temp file + rename, mode 0600, dirs 0700), and failures are recorded in `fail/dalmation/`.

**Styling** is a single `src/style.css`, `include_str!`'d into the binary and loaded at `APPLICATION` priority (`theme.rs`) so it overrides the system theme but inherits its named colors. The root window and dialogs carry the `.dalmation` CSS class.

`fs/mod.rs` holds the filesystem operations (trash via gio, permanent delete, rename, recursive copy/move with `(copy N)` de-duplication). They return `Result<(), String>` where the error string is ready to drop into an `AlertDialog`.
