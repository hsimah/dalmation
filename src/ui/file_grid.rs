use std::path::Path;

use gtk::prelude::*;
use gtk::{
    gio, Box as GtkBox, DirectoryList, GridView, Image, Justification, Label, ListItem,
    Orientation, ScrolledWindow, SignalListItemFactory, SingleSelection,
};

use crate::nav::Nav;

// The metadata we ask GIO to fetch per entry. Requesting only what we use keeps
// enumeration cheap. `standard::icon` gives us a themed file/folder icon for
// free in M1; thumbnails replace it for images in M3.
const ATTRS: &str = "standard::name,standard::display-name,standard::icon,standard::content-type";
const ICON_SIZE: i32 = 64;

/// The main content pane: a grid of the current directory's entries.
pub struct FileGrid {
    root: ScrolledWindow,
    dir_list: DirectoryList,
}

impl FileGrid {
    pub fn new(_nav: &Nav) -> Self {
        // DirectoryList is a GListModel that enumerates a directory *asynchronously*
        // and emits one GFileInfo per entry — so the IO never blocks the UI. We
        // start it empty and point it at a path in `load()`.
        let dir_list = DirectoryList::new(Some(ATTRS), gio::File::NONE);
        dir_list.set_monitored(true); // live-update when the directory changes on disk

        // SingleSelection adapts the model for a view and tracks the selected row.
        let selection = SingleSelection::new(Some(dir_list.clone()));

        // A factory builds and recycles the widget for each visible cell. GTK only
        // realizes cells that are on screen, so this scales to huge directories.
        let factory = SignalListItemFactory::new();

        // `setup`: build an empty cell once; it gets reused for many rows.
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<ListItem>().unwrap();

            let cell = GtkBox::new(Orientation::Vertical, 4);
            cell.add_css_class("file-cell");

            let image = Image::new();
            image.set_pixel_size(ICON_SIZE);

            let label = Label::new(None);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_max_width_chars(12);
            label.set_justify(Justification::Center);

            cell.append(&image);
            cell.append(&label);
            item.set_child(Some(&cell));
        });

        // `bind`: fill an existing cell with a specific row's data. Called whenever
        // a recycled cell is pointed at a (new) GFileInfo.
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<ListItem>().unwrap();
            let Some(info) = item.item().and_downcast::<gio::FileInfo>() else {
                return;
            };
            let Some(cell) = item.child().and_downcast::<GtkBox>() else {
                return;
            };
            let image = cell.first_child().and_downcast::<Image>().unwrap();
            let label = image.next_sibling().and_downcast::<Label>().unwrap();

            label.set_text(&info.display_name());
            if let Some(icon) = info.icon() {
                image.set_from_gicon(&icon);
            }
        });

        let grid = GridView::new(Some(selection), Some(factory));
        grid.set_min_columns(2);
        grid.set_max_columns(12);

        let root = ScrolledWindow::new();
        root.set_hexpand(true);
        root.set_child(Some(&grid));
        root.add_css_class("file-grid");

        FileGrid { root, dir_list }
    }

    /// Point the grid at a directory. DirectoryList re-enumerates asynchronously.
    pub fn load(&self, path: &Path) {
        self.dir_list.set_file(Some(&gio::File::for_path(path)));
    }

    pub fn widget(&self) -> &ScrolledWindow {
        &self.root
    }
}
