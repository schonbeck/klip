//! klip: a tiny native Wayland region screenshot tool.
//!
//! Flow: portal screenshot -> fullscreen selection window -> crop -> clipboard.

mod capture;
mod geometry;
mod output;
mod selector;

use gtk::{gdk, gio, glib, prelude::*};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

const APP_ID: &str = "io.github.schonbeck.klip";

const USAGE: &str = "\
Usage: klip [OPTIONS]

Take a region screenshot: drag a rectangle, the PNG goes to the clipboard.
Press Esc to cancel.

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
                println!("klip {}", env!("CARGO_PKG_VERSION"));
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
                    Ok(texture) => show_selector(&app, &texture),
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

/// Show the frozen screenshot fullscreen and wait for a selection.
fn show_selector(app: &gtk::Application, texture: &gdk::Texture) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("klip")
        .decorated(false)
        .build();

    let view = selector::SelectionView::new(
        texture,
        glib::clone!(
            #[weak]
            app,
            #[weak]
            window,
            #[strong]
            texture,
            move |rect| on_selected(&app, window.upcast_ref(), &texture, rect)
        ),
    );
    window.set_child(Some(&view));

    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(glib::clone!(
        #[weak]
        app,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_, key, _, _| {
            if key == gdk::Key::Escape {
                app.quit();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    ));
    window.add_controller(keys);

    window.fullscreen();
    window.present();
}

/// The user finished dragging. `rect` is in screenshot pixels.
///
/// Milestone 2 hook: show the Save / Cancel toolbar next to the selection here.
/// Milestone 3 hook: the annotation layer (arrows, lines, ellipses) is drawn on
/// top of the cropped image before it is copied or saved.
fn on_selected(
    app: &gtk::Application,
    window: &gtk::Window,
    texture: &gdk::Texture,
    rect: geometry::Rect,
) {
    let Some(px) = geometry::to_pixels(rect, texture.width(), texture.height()) else {
        return;
    };
    let image = output::crop(texture, px);
    output::copy_to_clipboard(app, window, &image);
}
