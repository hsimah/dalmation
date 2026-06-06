use gtk::prelude::*;
use gtk::Application;

use crate::theme;
use crate::ui::window::build_window;

const APP_ID: &str = "dev.hsimah.Dalmation";

/// Construct the GTK Application and connect its lifecycle callbacks.
pub fn build() -> Application {
    let app = Application::builder().application_id(APP_ID).build();

    // `startup` fires exactly once, before any window exists — the correct place
    // to install global CSS so every window inherits it.
    app.connect_startup(|_| theme::load_css());

    // `activate` fires each time the app is launched. We build a fresh window and
    // present it. (`build_window` borrows `app` so the window joins this app.)
    app.connect_activate(|app| {
        let window = build_window(app);
        window.present();
    });

    app
}
