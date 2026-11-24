//! klip: a tiny native Wayland region screenshot tool.
//!
//! Flow: portal screenshot -> fullscreen selection window -> annotate ->
//! Copy (clipboard) or Save (PNG in the configured folder).
//!
//! klip is a single-instance app: `klip --daemon` stays in the background
//! with a global shortcut; a plain `klip` while it runs asks it to capture.

mod annotate;
mod app;
mod capture;
mod config;
mod desktop;
mod geometry;
mod output;
mod permission;
mod selector;
mod settings;
mod shortcut;
mod tray;
mod ui;

use app::Action;
use gtk::{gio, glib, prelude::*};
use std::cell::RefCell;
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

Take a region screenshot: drag a rectangle, annotate it, then
Copy (Enter) puts it on the clipboard, Save (Ctrl+S) writes a PNG to the
screenshot folder, Cancel (Esc) closes.

Options:
  --daemon       Stay in the background and take a screenshot on the global
                 shortcut (or whenever `klip` is run)
  --settings     Open the settings (save folder, start at login, shortcut)
  --file <PNG>   Use an existing image instead of taking a screenshot (testing)
  -h, --help     Show this help
  -V, --version  Show the version";

/// Parse arguments (without the program name). `--file` paths are kept as
/// given; `resolve` turns them into a path relative to the caller.
fn parse_args(
    args: &[String],
    resolve: impl Fn(&str) -> Option<std::path::PathBuf>,
) -> Result<Action, String> {
    let mut action = Action::Capture(None);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        action = match arg.as_str() {
            "--daemon" => Action::Daemon,
            "--settings" => Action::Settings,
            "--file" => {
                let path = it.next().ok_or("--file needs a path")?;
                Action::Capture(Some(resolve(path).ok_or("--file: bad path")?))
            }
            other => return Err(format!("unknown argument: {other}\n\n{USAGE}")),
        };
    }
    Ok(action)
}

fn main() -> glib::ExitCode {
    // Answer these locally, without contacting a running instance.
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return glib::ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("klip {VERSION}");
                return glib::ExitCode::SUCCESS;
            }
            _ => {}
        }
    }
    // Reject bad arguments here, before a running instance would get them.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(msg) = parse_args(&args, |p| Some(p.into())) {
        eprintln!("klip: {msg}");
        return glib::ExitCode::FAILURE;
    }
    debug!("klip {VERSION}");

    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let state: Rc<RefCell<Option<Rc<app::App>>>> = Rc::default();
    app.connect_startup(glib::clone!(
        #[strong]
        state,
        move |app| {
            state.replace(Some(app::App::new(app)));
        }
    ));
    let activate_state = state.clone();
    app.connect_command_line(move |_, cmd| {
        let Some(state) = state.borrow().clone() else {
            return glib::ExitCode::FAILURE;
        };
        let args: Vec<String> = cmd
            .arguments()
            .iter()
            .skip(1)
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        if cmd.is_remote() {
            debug!("request from another klip process: {args:?}");
        }
        match parse_args(&args, |p| cmd.create_file_for_arg(p).path()) {
            Ok(action) => {
                state.run(action);
                glib::ExitCode::SUCCESS
            }
            Err(msg) => {
                eprintln!("klip: {msg}");
                glib::ExitCode::FAILURE
            }
        }
    });
    // Desktop launcher / D-Bus activation without arguments: capture.
    app.connect_activate(move |_| {
        debug!("activated");
        if let Some(state) = activate_state.borrow().clone() {
            state.run(Action::Capture(None));
        }
    });
    app.run()
}
