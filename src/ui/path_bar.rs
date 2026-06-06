use gtk::prelude::*;
use gtk::{Box as GtkBox, Entry, Orientation};

use crate::nav::Nav;

/// The editable path input across the top of the window.
///
/// For M1 this just *displays* the current directory. M2 will connect the
/// `activate` signal (Enter pressed) to drive navigation.
pub struct PathBar {
    root: GtkBox,
    #[allow(dead_code)]
    entry: Entry,
}

impl PathBar {
    pub fn new(nav: &Nav) -> Self {
        let root = GtkBox::new(Orientation::Horizontal, 0);
        root.add_css_class("path-bar");

        let entry = Entry::new();
        entry.set_hexpand(true);
        entry.set_text(&nav.current().to_string_lossy());
        root.append(&entry);

        PathBar { root, entry }
    }

    /// The root widget to insert into a parent container.
    pub fn widget(&self) -> &GtkBox {
        &self.root
    }
}
