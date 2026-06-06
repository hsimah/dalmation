use std::path::Path;

use gtk::gio::prelude::*;
use gtk::glib;
use gtk::glib::clone;
use gtk::prelude::*;
use gtk::{gio, Align, Box as GtkBox, Button, Grid, Label, Orientation, Window};

/// Show a modal "Properties" window for `path`, parented to `parent`.
pub fn present(parent: &impl IsA<Window>, path: &Path) {
    let dialog = Window::builder()
        .title("Properties")
        .modal(true)
        .transient_for(parent)
        .default_width(400)
        .resizable(false)
        .build();
    dialog.add_css_class("dalmation");

    let content = GtkBox::new(Orientation::Vertical, 16);
    content.add_css_class("info-dialog");
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.set_margin_start(16);
    content.set_margin_end(16);

    let grid = Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(16);
    for (row, (key, value)) in collect_fields(path).into_iter().enumerate() {
        let key_label = Label::builder().label(key).halign(Align::End).build();
        key_label.add_css_class("dim-label");
        let value_label = Label::builder()
            .label(value)
            .halign(Align::Start)
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .build();
        grid.attach(&key_label, 0, row as i32, 1, 1);
        grid.attach(&value_label, 1, row as i32, 1, 1);
    }
    content.append(&grid);

    let close = Button::with_label("Close");
    close.set_halign(Align::End);
    // The dialog is transient; capture it weakly so the close handler doesn't
    // form a cycle that keeps the dialog alive after it's dismissed.
    close.connect_clicked(clone!(#[weak] dialog, move |_| dialog.close()));
    content.append(&close);

    dialog.set_child(Some(&content));
    dialog.present();
}

/// Gather the (label, value) rows to display. Filesystem details come from a
/// single synchronous gio `query_info` (fine for one local file).
fn collect_fields(path: &Path) -> Vec<(&'static str, String)> {
    let mut fields = Vec::new();

    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    fields.push(("Name", name));
    if let Some(parent) = path.parent() {
        fields.push(("Location", parent.display().to_string()));
    }

    let query = gio::File::for_path(path).query_info(
        "standard::content-type,standard::size,time::modified,unix::mode",
        gio::FileQueryInfoFlags::NONE,
        gio::Cancellable::NONE,
    );
    if let Ok(info) = query {
        if let Some(content_type) = info.content_type() {
            fields.push((
                "Type",
                gio::content_type_get_description(&content_type).to_string(),
            ));
        }
        fields.push(("Size", glib::format_size(info.size().max(0) as u64).to_string()));
        if let Some(modified) = info.modification_date_time() {
            if let Ok(text) = modified.format("%Y-%m-%d %H:%M") {
                fields.push(("Modified", text.to_string()));
            }
        }
        let mode = info.attribute_uint32("unix::mode");
        if mode != 0 {
            fields.push(("Permissions", format_mode(mode)));
        }
    }

    fields
}

/// Render the low 9 permission bits as `rwxr-xr-x (755)`.
fn format_mode(mode: u32) -> String {
    let perms = mode & 0o777;
    let bit = |mask: u32, ch: char| if perms & mask != 0 { ch } else { '-' };
    format!(
        "{}{}{}{}{}{}{}{}{} ({:03o})",
        bit(0o400, 'r'),
        bit(0o200, 'w'),
        bit(0o100, 'x'),
        bit(0o040, 'r'),
        bit(0o020, 'w'),
        bit(0o010, 'x'),
        bit(0o004, 'r'),
        bit(0o002, 'w'),
        bit(0o001, 'x'),
        perms,
    )
}
