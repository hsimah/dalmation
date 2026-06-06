mod app;
mod fs;
mod nav;
mod theme;
mod thumbnail;
mod ui;

use gtk::glib;
use gtk::prelude::*;

fn main() -> glib::ExitCode {
    // The Application is GTK's app singleton: it owns the main loop, handles
    // activation, and tracks our top-level windows. `build()` wires up the
    // startup/activate callbacks; `run()` enters the main loop and blocks until
    // the last window closes.
    let app = app::build();
    app.run()
}
