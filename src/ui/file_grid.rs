use std::path::{Path, PathBuf};

use gtk::gio::prelude::*;
use gtk::prelude::*;
use gtk::{
    gio, Box as GtkBox, DirectoryList, GridView, Image, Justification, Label, ListItem,
    MultiSelection, Orientation, ScrolledWindow, SignalListItemFactory, SortListModel,
};
use gtk::glib;
use crate::thumbnail::Thumbnailer;
use crate::fs::directory_sorter::{DirectorySorter, Sort};

const ATTRS: &str = "standard::type,standard::name,standard::display-name,standard::icon,standard::content-type,time::modified";
const ICON_SIZE: i32 = 64;

#[derive(Clone)]
pub struct FileGrid {
    root: ScrolledWindow,
    dir_list: DirectoryList,
    grid_view: GridView,
    sorter: DirectorySorter,
    _thumbs: Thumbnailer,
}

impl FileGrid {
    pub fn new() -> Self {
        let thumbs = Thumbnailer::new();

        let dir_list = DirectoryList::new(Some(ATTRS), gio::File::NONE);
        dir_list.set_monitored(true);

        
        let sorter = DirectorySorter::new();
        let sorted = SortListModel::builder()
            .model(&dir_list)
            .sorter(sorter.sorter())
            .build();
        
        let selection = MultiSelection::new(Some(sorted.clone()));

        let factory = SignalListItemFactory::new();

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

        let thumbs_for_bind = thumbs.clone();
        let dir_for_bind = dir_list.clone();
        factory.connect_bind(move |_, item| {
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

            image.set_widget_name("");
            if let Some(icon) = info.icon() {
                image.set_from_gicon(&icon);
            }

            let is_image = info
                .content_type()
                .is_some_and(|ct| ct.starts_with("image/"));
            if is_image {
                if let Some(dir) = dir_for_bind.file() {
                    if let Some(path) = dir.child(info.name()).path() {
                        thumbs_for_bind.request(&path, &image);
                    }
                }
            }
        });
        
        let grid_view = GridView::new(Some(selection), Some(factory));
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
        gesture.connect_pressed(glib::clone!(
          #[weak] grid_view,
          move |_gesture, _n_press, x, y| {
              let hit = grid_view.pick(x, y, gtk::PickFlags::DEFAULT);
              let on_background = hit.map_or(true, |w| w == *grid_view.upcast_ref::<gtk::Widget>());
              if on_background {
                  if let Some(model) = grid_view.model() {
                      model.unselect_all();
                  }
              }
          }
        ));

        grid_view.add_controller(gesture);
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
            sorter,
            _thumbs: thumbs,
        }
    }

    /// Re-sort the grid by `sort`.
    pub fn set_sort(&self, sort: Sort) {
        self.sorter.sort(sort);
    }

    pub fn load(&self, path: &Path) {
        self.dir_list.set_file(Some(&gio::File::for_path(path)));
    }

    pub fn current_dir(&self) -> Option<PathBuf> {
        self.dir_list.file().and_then(|file| file.path())
    }

    pub fn selected_paths(&self) -> Vec<PathBuf> {
        let Some(dir) = self.dir_list.file() else {
            return Vec::new();
        };
        let Some(model) = self.grid_view.model() else {
            return Vec::new();
        };
        let mut paths = Vec::new();
        for pos in 0..model.n_items() {
            if model.is_selected(pos) {
                if let Some(info) = model.item(pos).and_downcast::<gio::FileInfo>() {
                    if let Some(path) = dir.child(info.name()).path() {
                        paths.push(path);
                    }
                }
            }
        }
        paths
    }

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
