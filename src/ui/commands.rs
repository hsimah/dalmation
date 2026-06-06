use std::path::{Path, PathBuf};

use gtk::gdk;
use gtk::gio;
use gtk::gio::prelude::*;
use gtk::glib;
use gtk::glib::clone;
use gtk::prelude::*;
use gtk::{
    Align, AlertDialog, Application, ApplicationWindow, Box as GtkBox, Button, Entry, GestureClick,
    Orientation, PopoverMenu, Window,
};

use crate::fs;
use crate::ui::file_grid::FileGrid;
use crate::ui::info_dialog;

/// Install file-operation actions, keyboard accelerators, and the right-click
/// context menu onto `window` / `grid`.
///
/// Actions live on the window under the `win.` prefix; the menu and the
/// accelerators both just reference those action names, so there's a single
/// source of truth for each command.
pub fn install(app: &Application, window: &ApplicationWindow, grid: &FileGrid) {
    // Own our handles so the `clone!` captures below are unambiguous.
    let window = window.clone();
    let grid = grid.clone();

    // Each action operates on the grid's current selection. `window` is captured
    // weakly (it owns the action, so a strong ref would be a cycle); `grid` is a
    // cheap clone handle (not a GObject, so `#[strong]`).
    add_action(
        &window,
        "properties",
        clone!(
            #[weak]
            window,
            #[strong]
            grid,
            move || {
                if let Some(path) = grid.selected_paths().into_iter().next() {
                    info_dialog::present(&window, &path);
                }
            }
        ),
    );

    add_action(
        &window,
        "rename",
        clone!(
            #[weak]
            window,
            #[strong]
            grid,
            move || {
                if let Some(path) = grid.selected_paths().into_iter().next() {
                    prompt_rename(&window, &path);
                }
            }
        ),
    );

    add_action(
        &window,
        "trash",
        clone!(
            #[weak]
            window,
            #[strong]
            grid,
            move || {
                let errors: Vec<String> = grid
                    .selected_paths()
                    .iter()
                    .filter_map(|p| fs::trash(p).err())
                    .collect();
                if !errors.is_empty() {
                    show_error(&window, &errors.join("\n"));
                }
            }
        ),
    );

    add_action(
        &window,
        "delete-permanent",
        clone!(
            #[weak]
            window,
            #[strong]
            grid,
            move || {
                let paths = grid.selected_paths();
                if !paths.is_empty() {
                    confirm_delete(&window, paths);
                }
            }
        ),
    );

    app.set_accels_for_action("win.rename", &["F2"]);
    app.set_accels_for_action("win.trash", &["Delete"]);
    app.set_accels_for_action("win.delete-permanent", &["<Shift>Delete"]);
    app.set_accels_for_action("win.properties", &["<Alt>Return"]);

    install_context_menu(&grid);
}

fn add_action<F: Fn() + 'static>(window: &ApplicationWindow, name: &str, callback: F) {
    let action = gio::SimpleAction::new(name, None);
    action.connect_activate(move |_, _| callback());
    window.add_action(&action);
}

fn install_context_menu(grid: &FileGrid) {
    let menu = gio::Menu::new();
    menu.append(Some("Rename"), Some("win.rename"));
    menu.append(Some("Move to Trash"), Some("win.trash"));
    menu.append(Some("Delete permanently…"), Some("win.delete-permanent"));
    menu.append(Some("Properties"), Some("win.properties"));

    let popover = PopoverMenu::from_model(Some(&menu));
    popover.set_parent(grid.widget());
    popover.set_has_arrow(false);
    popover.set_halign(Align::Start);

    let gesture = GestureClick::new();
    gesture.set_button(gdk::BUTTON_SECONDARY);
    gesture.connect_pressed(clone!(
        #[weak]
        popover,
        move |_, _, x, y| {
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.popup();
        }
    ));
    grid.widget().add_controller(gesture);
}

// --- Dialogs -----------------------------------------------------------------

fn prompt_rename(window: &ApplicationWindow, path: &Path) {
    let dialog = Window::builder()
        .title("Rename")
        .modal(true)
        .transient_for(window)
        .default_width(360)
        .resizable(false)
        .build();
    dialog.add_css_class("dalmation");

    let vbox = GtkBox::new(Orientation::Vertical, 12);
    vbox.add_css_class("info-dialog");
    vbox.set_margin_top(16);
    vbox.set_margin_bottom(16);
    vbox.set_margin_start(16);
    vbox.set_margin_end(16);

    let entry = Entry::new();
    let current = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    entry.set_text(&current);
    vbox.append(&entry);

    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(Align::End);
    let cancel = Button::with_label("Cancel");
    let confirm = Button::with_label("Rename");
    confirm.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&confirm);
    vbox.append(&buttons);

    dialog.set_child(Some(&vbox));

    let path = path.to_path_buf();

    cancel.connect_clicked(clone!(#[weak] dialog, move |_| dialog.close()));

    // The dialog is transient and OWNS these button closures, so it MUST be
    // captured weakly — a strong ref would form a cycle (dialog -> button ->
    // closure -> dialog) and leak the dialog every time it's opened. `entry` is
    // likewise weak; `window` is app-lifetime so a strong capture is fine.
    confirm.connect_clicked(clone!(
        #[weak] dialog,
        #[weak] entry,
        #[strong] window,
        #[strong] path,
        move |_| do_rename(&window, &dialog, &path, &entry.text())
    ));
    entry.connect_activate(clone!(
        #[weak] dialog,
        #[strong] window,
        #[strong] path,
        move |entry| do_rename(&window, &dialog, &path, &entry.text())
    ));

    dialog.present();
    entry.grab_focus();
    entry.select_region(0, -1); // preselect the name for quick editing
}

fn do_rename(window: &ApplicationWindow, dialog: &Window, path: &Path, name: &str) {
    match fs::rename(path, name) {
        Ok(()) => dialog.close(),
        Err(message) => show_error(window, &message),
    }
}

fn confirm_delete(window: &ApplicationWindow, paths: Vec<PathBuf>) {
    let detail = if paths.len() == 1 {
        format!(
            "Permanently delete “{}”?\nThis cannot be undone.",
            file_label(&paths[0])
        )
    } else {
        format!(
            "Permanently delete {} items?\nThis cannot be undone.",
            paths.len()
        )
    };

    let dialog = AlertDialog::builder()
        .modal(true)
        .message("Delete permanently")
        .detail(detail)
        .buttons(["Cancel", "Delete"])
        .cancel_button(0)
        .default_button(0)
        .build();

    dialog.choose(
        Some(window),
        gio::Cancellable::NONE,
        clone!(
            #[weak]
            window,
            move |result| {
                if let Ok(1) = result {
                    let errors: Vec<String> = paths
                        .iter()
                        .filter_map(|p| fs::delete_permanent(p).err())
                        .collect();
                    if !errors.is_empty() {
                        show_error(&window, &errors.join("\n"));
                    }
                }
            }
        ),
    );
}

fn show_error(window: &ApplicationWindow, message: &str) {
    AlertDialog::builder()
        .modal(true)
        .message("Operation failed")
        .detail(message)
        .build()
        .show(Some(window));
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
