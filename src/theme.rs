use gtk::gdk::Display;
use gtk::style_context_add_provider_for_display;
use gtk::{CssProvider, STYLE_PROVIDER_PRIORITY_APPLICATION};

// Compiled into the binary at build time — no runtime file to ship or lose.
const STYLE: &str = include_str!("style.css");

/// Load our CSS for the whole display. Called once at app startup.
///
/// We register at APPLICATION priority, which sits *above* the system theme but
/// *below* user overrides, so our rules win over the theme defaults while still
/// inheriting the theme's named colors (e.g. `@theme_bg_color`).
pub fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_string(STYLE);

    let display = Display::default().expect("a default display should exist");
    style_context_add_provider_for_display(
        &display,
        &provider,
        STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
