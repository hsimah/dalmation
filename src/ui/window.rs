use std::path::Path;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{glib, Application, ApplicationWindow, Box as GtkBox, Orientation, Paned, HeaderBar};

use crate::nav::Nav;
use crate::ui::commands;
use crate::ui::file_grid::FileGrid;
use crate::ui::path_bar::PathBar;

/// Build the main window: a vertical stack of [ path bar ] over a horizontal
/// [ sidebar tree | file grid ] split, all wired together for navigation.
pub fn build_window(app: &Application) -> ApplicationWindow {
    let start = glib::home_dir();
    let nav = Nav::new(start.clone());
    let path_bar = PathBar::new(&start);
    let title_bar = HeaderBar::new();
    title_bar.set_title_widget(Some(path_bar.widget()));
    let window = ApplicationWindow::builder()
        .application(app)
        .title("dalmation")
        .titlebar(&title_bar)
        .default_width(1100)
        .default_height(720)
        .build();
    window.add_css_class("dalmation");

    let root = GtkBox::new(Orientation::Vertical, 0);

    // Resizable split. The handle lets the user drag the sidebar width.
    let paned = Paned::new(Orientation::Horizontal);
    paned.set_position(240);
    paned.set_vexpand(true);

    paned.set_resize_start_child(false);
    paned.set_shrink_start_child(false);

    let grid = FileGrid::new();
    grid.load(&start);
    paned.set_end_child(Some(grid.widget()));

    root.append(&paned);
    window.set_child(Some(&root));

    // ----------------------------------------------------------------------
    // Navigation wiring
    //
    // We funnel every navigation source (path bar, grid, tree, Back) through two
    // shared closures so the views always stay in sync:
    //   * `show`     — update the views to display a directory (no history change)
    //   * `navigate` — record history, then `show` (ignores non-directories)
    //
    // They're `Rc<dyn Fn(..)>` so multiple signal handlers can share one copy.
    //
    // MEMORY NOTE: these closures hold strong handles to the widgets, and the
    // widgets hold the signal handlers that hold the closures — a reference
    // cycle. That's deliberately fine *here* because all of it lives for the
    // whole app (one window, never destroyed), so it's freed at exit anyway. For
    // transient widgets (dialogs, list rows) you'd break the cycle with
    // `glib::clone!(#[weak] ...)`, which captures a weak ref and upgrades it.
    // ----------------------------------------------------------------------
    let show: Rc<dyn Fn(&Path)> = {
        let grid = grid.clone();
        let path_bar = path_bar.clone();
        Rc::new(move |path: &Path| {
            grid.load(path);
            path_bar.set_path(path);
        })
    };

    let navigate: Rc<dyn Fn(&Path)> = {
        let nav = nav.clone();
        let show = show.clone();
        Rc::new(move |path: &Path| {
            if !path.is_dir() {
                return;
            }
            nav.go_to(path);
            show(path);
        })
    };

    // Enter in the path bar → jump to the typed path.
    path_bar.connect_activated({
        let navigate = navigate.clone();
        move |text| navigate(Path::new(text.trim()))
    });

    // Back button → pop history and show it (without re-recording history).
    path_bar.connect_back({
        let nav = nav.clone();
        let show = show.clone();
        move || {
            if let Some(previous) = nav.go_back() {
                show(&previous);
            }
        }
    });

    // Double-click / Enter on a grid entry → enter it (if a directory).
    grid.connect_activated({
        let navigate = navigate.clone();
        move |path| navigate(&path)
    });

    // File-operation actions, accelerators, and the right-click context menu.
    commands::install(app, &window, &grid);

    window
}
