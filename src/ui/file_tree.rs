use std::path::{Path, PathBuf};

use gtk::gio::prelude::*;
use gtk::glib::BoxedAnyObject;
use gtk::prelude::*;
use gtk::{
    gio, Box as GtkBox, Image, Label, ListItem, ListView, Orientation, ScrolledWindow,
    SignalListItemFactory, SingleSelection, TreeExpander, TreeListModel, TreeListRow,
};

/// The sidebar directory tree.
///
/// Instead of writing a custom GObject subclass to hold each node's path, we use
/// `glib::BoxedAnyObject` — a ready-made GObject that can wrap *any* Rust value.
/// Here it wraps a `PathBuf`. We read it back out with `borrow::<PathBuf>()`,
/// which is just a `RefCell` borrow under the hood (runtime-checked).
#[derive(Clone)]
pub struct FileTree {
    root: ScrolledWindow,
    view: ListView,
}

impl FileTree {
    pub fn new(root_dir: &Path) -> Self {
        // The visible top level: the directories directly under `root_dir`.
        let root_model = build_store(read_subdirs(root_dir));

        // TreeListModel turns a flat root model into a lazily-expandable tree.
        // `passthrough = false` means the model yields TreeListRow items (which
        // know their depth/expansion); `autoexpand = false` keeps it collapsed.
        // The closure is called *only when a row is expanded*, to produce that
        // row's children — so we never scan the whole filesystem up front.
        let tree_model = TreeListModel::new(root_model, false, false, |item| {
            let obj = item.downcast_ref::<BoxedAnyObject>()?;
            let path = obj.borrow::<PathBuf>();
            let children = read_subdirs(path.as_path());
            if children.is_empty() {
                None // no expander arrow for empty/inaccessible dirs
            } else {
                Some(build_store(children).upcast())
            }
        });

        let selection = SingleSelection::new(Some(tree_model));
        selection.set_autoselect(false);
        selection.set_can_unselect(true);

        let factory = SignalListItemFactory::new();

        // Build a row: a TreeExpander (draws the indent + arrow) wrapping an
        // [icon | label] box.
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<ListItem>().unwrap();

            let content = GtkBox::new(Orientation::Horizontal, 6);
            content.append(&Image::from_icon_name("folder-symbolic"));
            content.append(&Label::new(None));

            let expander = TreeExpander::new();
            expander.set_child(Some(&content));
            item.set_child(Some(&expander));
        });

        // Bind a row to a TreeListRow: hand the row to the expander (so it can
        // manage expansion), then fill in the folder name.
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<ListItem>().unwrap();
            let Some(tree_row) = item.item().and_downcast::<TreeListRow>() else {
                return;
            };
            let Some(expander) = item.child().and_downcast::<TreeExpander>() else {
                return;
            };
            expander.set_list_row(Some(&tree_row));

            let Some(content) = expander.child().and_downcast::<GtkBox>() else {
                return;
            };
            let label = content.last_child().and_downcast::<Label>().unwrap();

            if let Some(obj) = tree_row.item().and_downcast::<BoxedAnyObject>() {
                let path = obj.borrow::<PathBuf>();
                label.set_text(&dir_label(path.as_path()));
            }
        });
                
        let view = ListView::new(Some(selection), Some(factory));
        view.set_single_click_activate(true);
        view.add_css_class("file-tree");

        let root = ScrolledWindow::new();
        root.set_child(Some(&view));

        FileTree { root, view }
    }

    /// Fire `f` with a directory's path when its row is activated.
    pub fn connect_activated<F: Fn(PathBuf) + 'static>(&self, f: F) {
        self.view.connect_activate(move |view, pos| {
            let Some(model) = view.model() else { return };
            let Some(row) = model.item(pos).and_downcast::<TreeListRow>() else {
                return;
            };
            let Some(obj) = row.item().and_downcast::<BoxedAnyObject>() else {
                return;
            };
            let path = obj.borrow::<PathBuf>().clone();
            f(path);
        });
    }

    pub fn widget(&self) -> &ScrolledWindow {
        &self.root
    }
}

/// Wrap each path in a BoxedAnyObject and collect into a GListStore.
fn build_store(paths: Vec<PathBuf>) -> gio::ListStore {
    let store = gio::ListStore::new::<BoxedAnyObject>();
    for path in paths {
        store.append(&BoxedAnyObject::new(path));
    }
    store
}

/// The immediate subdirectories of `dir`, hidden ones skipped, name-sorted.
/// Synchronous, which is fine for a local sidebar; errors (e.g. permission
/// denied) yield an empty list rather than failing.
fn read_subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir() && !is_hidden(p))
            .collect(),
        Err(_) => Vec::new(),
    };
    dirs.sort_by_key(|p| dir_label(p).to_lowercase());
    dirs
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.starts_with('.'))
}

/// Display name for a directory row: the final path component, or the whole
/// path for roots like "/" that have no file name.
fn dir_label(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}
