use std::path::{Path, PathBuf};

use gtk::gio::prelude::*;
use gtk::prelude::*;
use gtk::{
    gio, Box as GtkBox, DirectoryList, GridView, Image, Justification, Label, ListItem,
    Orientation, ScrolledWindow, SignalListItemFactory, SingleSelection,
};

// The metadata we ask GIO to fetch per entry. Requesting only what we use keeps
// enumeration cheap. `standard::icon` gives us a themed file/folder icon for
// free in M1; thumbnails replace it for images in M3.
const ATTRS: &str = "standard::name,standard::display-name,standard::icon,standard::content-type";
const ICON_SIZE: i32 = 64;

/// The main content pane: a grid of the current directory's entries.
///
/// All fields are GTK objects (reference-counted handles), so deriving `Clone`
/// gives us a cheap second *handle* to the same widgets — not a copy. That lets
/// us clone a `FileGrid` into closures in `window.rs`. This is the same trick as
/// `Nav`, just with widgets instead of `Rc<RefCell<..>>`.
#[derive(Clone)]
pub struct FileGrid {
    root: ScrolledWindow,
    dir_list: DirectoryList,
    grid_view: GridView,
}

impl FileGrid {
    pub fn new() -> Self {
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

        let grid_view = GridView::new(Some(selection), Some(factory));
        grid_view.set_min_columns(2);
        grid_view.set_max_columns(12);

        let root = ScrolledWindow::new();
        root.set_hexpand(true);
        root.set_child(Some(&grid_view));
        root.add_css_class("file-grid");

        FileGrid {
            root,
            dir_list,
            grid_view,
        }
    }

    /// Point the grid at a directory. DirectoryList re-enumerates asynchronously.
    pub fn load(&self, path: &Path) {
        self.dir_list.set_file(Some(&gio::File::for_path(path)));
    }

    /// Call `f` with the absolute path of an entry when it is activated
    /// (double-click or Enter). We resolve the path from the DirectoryList's own
    /// directory (`dir.child(name)`), so the grid needs no knowledge of `Nav`.
    pub fn connect_activated<F: Fn(PathBuf) + 'static>(&self, f: F) {
        let dir_list = self.dir_list.clone();
        self.grid_view.connect_activate(move |grid_view, pos| {
            let Some(model) = grid_view.model() else { return };
            let Some(info) = model.item(pos).and_downcast::<gio::FileInfo>() else {
                return;
            };
            let Some(dir) = dir_list.file() else { return };
            if let Some(path) = dir.child(info.name()).path() {
                f(path);
            }
        });
    }

    pub fn widget(&self) -> &ScrolledWindow {
        &self.root
    }
}
