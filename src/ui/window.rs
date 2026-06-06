use std::path::PathBuf;

use gtk::prelude::*;
use gtk::{
    glib, Application, ApplicationWindow, Box as GtkBox, Orientation, Paned, ScrolledWindow,
};

use crate::nav::Nav;
use crate::ui::file_grid::FileGrid;
use crate::ui::path_bar::PathBar;

/// Build the main window: a vertical stack of [ path bar ] over a horizontal
/// [ sidebar | file grid ] split.
pub fn build_window(app: &Application) -> ApplicationWindow {
    let start = glib::home_dir();
    let nav = Nav::new(start.clone());

    let window = ApplicationWindow::builder()
        .application(app)
        .title("dalmation")
        .default_width(1100)
        .default_height(720)
        .build();
    // This class is the CSS hook for the whole window (transparency etc.).
    window.add_css_class("dalmation");

    let root = GtkBox::new(Orientation::Vertical, 0);

    let path_bar = PathBar::new(&nav);
    root.append(path_bar.widget());

    // Resizable split. The handle lets the user drag the sidebar width.
    let paned = Paned::new(Orientation::Horizontal);
    paned.set_position(240);
    paned.set_vexpand(true);

    // Sidebar placeholder for M1 — the real lazy file tree arrives in M2.
    let sidebar = ScrolledWindow::new();
    sidebar.add_css_class("sidebar");
    paned.set_start_child(Some(&sidebar));
    paned.set_resize_start_child(false);
    paned.set_shrink_start_child(false);

    let grid = FileGrid::new(&nav);
    grid.load(&start);
    paned.set_end_child(Some(grid.widget()));

    root.append(&paned);
    window.set_child(Some(&root));
    window
}

// Kept as a tiny seam so M2 navigation code has an obvious place to resolve a
// starting directory if we ever want something other than $HOME.
#[allow(dead_code)]
fn default_start_dir() -> PathBuf {
    glib::home_dir()
}
