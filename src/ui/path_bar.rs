use std::path::Path;

use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, Entry, Orientation};

/// The top bar: a Back button plus an editable path input.
///
/// Like `FileGrid`, every field is a GTK handle, so `Clone` is cheap and shares
/// the same widgets.
#[derive(Clone)]
pub struct PathBar {
    root: GtkBox,
    entry: Entry,
    back: Button,
}

impl PathBar {
    pub fn new(start: &Path) -> Self {
        let root = GtkBox::new(Orientation::Horizontal, 6);
        root.add_css_class("path-bar");

        let back = Button::from_icon_name("go-previous-symbolic");
        back.add_css_class("flat");
        back.set_tooltip_text(Some("Back"));

        let entry = Entry::new();
        entry.set_hexpand(true);
        entry.set_text(&start.to_string_lossy());

        root.append(&back);
        root.append(&entry);

        PathBar { root, entry, back }
    }

    /// Reflect the current directory in the entry (called after navigation).
    pub fn set_path(&self, path: &Path) {
        self.entry.set_text(&path.to_string_lossy());
    }

    /// Fire `f` with the typed text when the user presses Enter in the entry.
    pub fn connect_activated<F: Fn(String) + 'static>(&self, f: F) {
        self.entry
            .connect_activate(move |entry| f(entry.text().to_string()));
    }

    /// Fire `f` when the Back button is clicked.
    pub fn connect_back<F: Fn() + 'static>(&self, f: F) {
        self.back.connect_clicked(move |_| f());
    }

    pub fn widget(&self) -> &GtkBox {
        &self.root
    }
}
