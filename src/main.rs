//! klip: a tiny native Wayland region screenshot tool.
//!
//! Flow: portal screenshot -> fullscreen selection window -> crop ->
//! clipboard, with an optional Save to a PNG file.

mod annotate;
mod capture;
mod geometry;
mod output;
mod selector;
mod ui;

use gtk::{gio, glib, prelude::*};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

/// `eprintln!` when the environment variable `KLIP_DEBUG` is set.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if $crate::debug_enabled() {
            eprintln!("klip[debug]: {}", format!($($arg)*));
        }
    };
}

pub fn debug_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("KLIP_DEBUG").is_some_and(|v| !v.is_empty() && v != "0"))
}

/// Version plus the git commit it was built from, to spot stale builds.
const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("KLIP_GIT_HASH"), ")");

const APP_ID: &str = "io.github.schonbeck.klip";

const USAGE: &str = "\
Usage: klip [OPTIONS]

Take a region screenshot: drag a rectangle, the PNG goes to the clipboard.
Then Save (Enter, Ctrl+S) writes it to a file; Cancel (Esc) closes.

Options:
  --file <PNG>   Use an existing image instead of taking a screenshot (testing)
  -h, --help     Show this help
  -V, --version  Show the version";

enum Source {
    Portal,
    File(PathBuf),
}

fn parse_args() -> Result<Source, String> {
    let mut args = std::env::args().skip(1);
    let mut source = Source::Portal;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("klip {VERSION}");
                std::process::exit(0);
            }
            "--file" => {
                let path = args.next().ok_or("--file needs a path")?;
                source = Source::File(path.into());
            }
            other => return Err(format!("unknown argument: {other}\n\n{USAGE}")),
        }
    }
    Ok(source)
}

fn main() -> glib::ExitCode {
    let source = match parse_args() {
        Ok(s) => Rc::new(s),
        Err(msg) => {
            eprintln!("klip: {msg}");
            return glib::ExitCode::FAILURE;
        }
    };

    debug!("klip {VERSION}");

    // NON_UNIQUE: every hotkey press is its own short-lived process. A new
    // capture taking the clipboard makes the previous klip exit.
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let failed = Rc::new(Cell::new(false));

    app.connect_activate(glib::clone!(
        #[strong]
        failed,
        move |app| {
            // No window exists while we wait for the portal; keep the app alive.
            let hold = app.hold();
            let (app, source, failed) = (app.clone(), source.clone(), failed.clone());
            glib::spawn_future_local(async move {
                let texture = match source.as_ref() {
                    Source::Portal => capture::from_portal().await,
                    Source::File(path) => capture::from_file(path),
                };
                match texture {
                    Ok(texture) => ui::show(&app, &texture),
                    Err(err) => {
                        eprintln!("klip: could not take screenshot: {err}");
                        failed.set(true);
                    }
                }
                drop(hold);
            });
        }
    ));

    // GApplication would reject our own options, so don't pass them on.
    let status = app.run_with_args(&["klip"]);
    if failed.get() {
        glib::ExitCode::FAILURE
    } else {
        status
    }
}
