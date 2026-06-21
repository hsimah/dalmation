use gtk::gio::prelude::*;
use gtk::{
    gio, CustomSorter,
};
use std::cmp::Ordering;
use gtk::Ordering as GtkOrdering;

#[derive(Clone)]
pub struct DirectorySorter {
    sorter: CustomSorter,
}

impl DirectorySorter {
    pub fn new() -> Self {
        let sorter = CustomSorter::new(Self::sort_by_name_asc);

        DirectorySorter {
            sorter,
        }
    }
    
    pub fn sorter(&self) -> &CustomSorter {
        &self.sorter
    }

    fn sort_by_name_asc(a: &gtk::glib::Object, b: &gtk::glib::Object) -> GtkOrdering {
   
        let a = a.downcast_ref::<gio::FileInfo>().unwrap();
        let b = b.downcast_ref::<gio::FileInfo>().unwrap();

        let a_is_dir = a.file_type() == gio::FileType::Directory;
        let b_is_dir = b.file_type() == gio::FileType::Directory;

        match (a_is_dir, b_is_dir) {
            (true, false) => return Ordering::Less.into(),
            (false, true) => return Ordering::Greater.into(),
            _ => {}
        }

        let an = a.display_name();
        let bn = b.display_name();
        an.cmp(&bn).into()
    }
}
