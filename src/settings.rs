//! The settings window: save folder, start at login, shortcut.

use crate::app::App;
use crate::shortcut;
use gtk::{gdk, gio, glib, prelude::*};
use std::path::Path;
use std::rc::Rc;

thread_local! {
    /// The open settings window, so a second request just raises it.
    static WINDOW: std::cell::RefCell<Option<gtk::Window>> = const { std::cell::RefCell::new(None) };
}

fn folder_label(dir: Option<&Path>) -> String {
    dir.map_or("Not set yet (asked on the first Save)".into(), |d| {
        d.display().to_string()
    })
}

fn row(grid: &gtk::Grid, y: i32, title: &str, widget: &impl IsA<gtk::Widget>) {
    let label = gtk::Label::builder()
        .label(title)
        .halign(gtk::Align::Start)
        .build();
    grid.attach(&label, 0, y, 1, 1);
    grid.attach(widget, 1, y, 1, 1);
}

pub fn open(app: &Rc<App>, parent: Option<&gtk::Window>) {
    if let Some(win) = WINDOW.with(|w| w.borrow().clone()) {
        win.present();
        return;
    }
    let window = gtk::Window::builder()
        .application(&app.app)
        .title("klip settings")
        .resizable(false)
        .modal(parent.is_some())
        .build();
    window.set_transient_for(parent);
    let grid = gtk::Grid::builder()
        .row_spacing(14)
        .column_spacing(18)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();

    // Save folder.
    let folder = gtk::Label::builder()
        .label(folder_label(app.config.borrow().save_dir.as_deref()))
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .max_width_chars(36)
        .halign(gtk::Align::Start)
        .hexpand(true)
        .build();
    let change = gtk::Button::with_label("Change…");
    let folder_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    folder_box.append(&folder);
    folder_box.append(&change);
    row(&grid, 0, "Save folder", &folder_box);
    change.connect_clicked(glib::clone!(
        #[weak]
        app,
        #[weak]
        window,
        #[weak]
        folder,
        move |_| {
            glib::spawn_future_local(async move {
                if let Some(dir) = choose_folder(&app, Some(&window)).await {
                    folder.set_label(&folder_label(Some(&dir)));
                }
            });
        }
    ));

    // Start at login.
    let autostart = gtk::Switch::builder()
        .active(app.config.borrow().autostart)
        .halign(gtk::Align::Start)
        .build();
    row(&grid, 1, "Start at login", &autostart);
    autostart.connect_active_notify(glib::clone!(
        #[weak]
        app,
        move |s| {
            let on = s.is_active();
            app.config.borrow_mut().autostart = on;
            app.save_config();
            glib::spawn_future_local(async move { app.set_autostart(on).await });
        }
    ));

    // Shortcut: click, then press the new combination.
    let accel_button = gtk::Button::with_label(&shortcut::label(&app.config.borrow().shortcut));
    accel_button.set_halign(gtk::Align::Start);
    row(&grid, 2, "Screenshot shortcut", &accel_button);
    let status = gtk::Label::builder()
        .label(app.shortcut_backend.get().describe())
        .halign(gtk::Align::Start)
        .css_classes(["dim-label"])
        .build();
    grid.attach(&status, 1, 3, 1, 1);
    let capturing = Rc::new(std::cell::Cell::new(false));
    accel_button.connect_clicked(glib::clone!(
        #[strong]
        capturing,
        move |b| {
            capturing.set(true);
            b.set_label("Press the new shortcut… (Esc to cancel)");
        }
    ));
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(glib::clone!(
        #[weak]
        app,
        #[weak]
        accel_button,
        #[weak]
        status,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_, key, _, mods| {
            if !capturing.get() {
                return glib::Propagation::Proceed;
            }
            let mods = mods & gtk::accelerator_get_default_mod_mask();
            if key == gdk::Key::Escape {
                capturing.set(false);
                accel_button.set_label(&shortcut::label(&app.config.borrow().shortcut));
                return glib::Propagation::Stop;
            }
            // Wait for a real key; also require a modifier (or a function key).
            let is_modifier = key.to_unicode().is_none()
                && key.name().is_some_and(|n| {
                    ["Shift", "Control", "Alt", "Super", "Meta", "ISO_Level3"]
                        .iter()
                        .any(|m| n.starts_with(m))
                });
            if is_modifier || (mods.is_empty() && key.to_unicode().is_some()) {
                return glib::Propagation::Stop;
            }
            let accel = gtk::accelerator_name(key.to_lower(), mods).to_string();
            capturing.set(false);
            accel_button.set_label(&shortcut::label(&accel));
            app.config.borrow_mut().shortcut = accel;
            app.save_config();
            glib::spawn_future_local(async move {
                app.apply_shortcut().await;
                status.set_label(app.shortcut_backend.get().describe());
            });
            glib::Propagation::Stop
        }
    ));
    window.add_controller(keys);

    let close = gtk::Button::with_label("Close");
    close.set_halign(gtk::Align::End);
    close.connect_clicked(glib::clone!(
        #[weak]
        window,
        move |_| window.close()
    ));
    grid.attach(&close, 1, 4, 1, 1);

    window.set_child(Some(&grid));
    window.connect_close_request(|_| {
        WINDOW.with(|w| w.borrow_mut().take());
        glib::Propagation::Proceed
    });
    WINDOW.with(|w| w.replace(Some(window.clone())));
    if let Some(token) = app.take_activation_token() {
        window.set_startup_id(&token);
    }
    window.present();
}

/// Ask for the save folder and remember it.
pub async fn choose_folder(
    app: &Rc<App>,
    parent: Option<&gtk::Window>,
) -> Option<std::path::PathBuf> {
    let dialog = gtk::FileDialog::builder()
        .title("Folder for saved screenshots")
        .modal(true)
        .build();
    let start = app
        .config
        .borrow()
        .save_dir
        .clone()
        .or_else(|| glib::user_special_dir(glib::UserDirectory::Pictures))
        .filter(|d| d.is_dir())
        .unwrap_or_else(glib::home_dir);
    dialog.set_initial_folder(Some(&gio::File::for_path(start)));
    let dir = dialog.select_folder_future(parent).await.ok()?.path()?;
    app.config.borrow_mut().save_dir = Some(dir.clone());
    app.save_config();
    Some(dir)
}
