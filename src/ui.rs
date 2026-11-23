//! The fullscreen window: the selection view plus the Save / Cancel toolbar
//! that appears next to the selection.

use crate::geometry::{self, Rect};
use crate::output;
use crate::selector::SelectionView;
use gtk::{gdk, gio, glib, prelude::*};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Space between the selection and the toolbar (logical pixels).
const TOOLBAR_GAP: f64 = 8.0;

const CSS: &str = "
.klip-toolbar {
    background: rgba(32, 32, 32, 0.92);
    border-radius: 12px;
    padding: 6px;
}
.klip-toolbar button {
    min-height: 40px;
    min-width: 104px;
    padding: 0 16px;
    font-size: 1.15em;
    font-weight: bold;
}
";

struct Ui {
    app: gtk::Application,
    window: gtk::ApplicationWindow,
    toolbar: gtk::Box,
    /// The full screenshot.
    texture: gdk::Texture,
    /// The cropped selection, once there is one.
    image: RefCell<Option<gdk::Texture>>,
    /// A file dialog is open.
    saving: Cell<bool>,
}

/// Show the frozen screenshot fullscreen and wait for a selection.
pub fn show(app: &gtk::Application, texture: &gdk::Texture) {
    load_css();
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("klip")
        .decorated(false)
        .build();
    let view = SelectionView::new(texture);
    let toolbar = build_toolbar();
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&view));
    overlay.add_overlay(&toolbar);
    window.set_child(Some(&overlay));

    let ui = Rc::new(Ui {
        app: app.clone(),
        window,
        toolbar,
        texture: texture.clone(),
        image: RefCell::default(),
        saving: Cell::new(false),
    });

    // This closure holds the only strong reference to `ui`, so it lives as
    // long as the window (i.e. the whole short-lived process).
    view.connect_selected(glib::clone!(
        #[strong]
        ui,
        move |widget_rect, image_rect| ui.on_selected(widget_rect, image_rect)
    ));
    ui.connect_buttons();
    ui.connect_keys();

    ui.window.fullscreen();
    ui.window.present();
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn button(label: &str, icon: &str, tooltip: &str) -> gtk::Button {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.set_halign(gtk::Align::Center);
    content.append(&gtk::Image::from_icon_name(icon));
    content.append(&gtk::Label::new(Some(label)));
    let button = gtk::Button::builder()
        .child(&content)
        .tooltip_text(tooltip)
        // Keyboard shortcuts are handled by the window, so Enter must never
        // activate whichever button happens to have focus.
        .focusable(false)
        .build();
    button.set_widget_name(label);
    button
}

fn build_toolbar() -> gtk::Box {
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    toolbar.add_css_class("klip-toolbar");
    toolbar.set_halign(gtk::Align::Start);
    toolbar.set_valign(gtk::Align::Start);
    toolbar.set_visible(false);

    // Milestone 3: annotation tool buttons (arrow, line, ellipse, color) go
    // in this box, left of Save / Cancel. Hidden while it is empty.
    let tools = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    tools.set_widget_name("tools");
    tools.set_visible(false);
    toolbar.append(&tools);

    let save = button(
        "Save",
        "document-save-symbolic",
        "Save as PNG (Enter, Ctrl+S)",
    );
    save.add_css_class("suggested-action");
    toolbar.append(&save);
    toolbar.append(&button("Cancel", "window-close-symbolic", "Close (Esc)"));
    toolbar
}

impl Ui {
    fn connect_buttons(self: &Rc<Self>) {
        let mut child = self.toolbar.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            let Ok(button) = widget.downcast::<gtk::Button>() else {
                continue;
            };
            let save = button.widget_name() == "Save";
            button.connect_clicked(glib::clone!(
                #[weak(rename_to = ui)]
                self,
                move |_| if save { ui.save() } else { ui.close() }
            ));
        }
    }

    fn connect_keys(self: &Rc<Self>) {
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, modifiers| {
                let ctrl = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
                match key {
                    gdk::Key::Escape => ui.close(),
                    gdk::Key::Return | gdk::Key::KP_Enter => ui.save(),
                    gdk::Key::s | gdk::Key::S if ctrl => ui.save(),
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            }
        ));
        self.window.add_controller(keys);
    }

    /// A selection was made (or replaced): copy it right away and show the
    /// toolbar next to it.
    fn on_selected(&self, widget_rect: Rect, image_rect: Rect) {
        let (w, h) = (self.texture.width(), self.texture.height());
        let Some(px) = geometry::to_pixels(image_rect, w, h) else {
            return;
        };
        let image = output::crop(&self.texture, px);
        output::copy_to_clipboard(&self.window.clipboard(), &image);
        self.image.replace(Some(image));
        self.place_toolbar(widget_rect);
    }

    fn place_toolbar(&self, sel: Rect) {
        self.toolbar.set_visible(true);
        let (_, tw, _, _) = self.toolbar.measure(gtk::Orientation::Horizontal, -1);
        let (_, th, _, _) = self.toolbar.measure(gtk::Orientation::Vertical, tw);
        let (x, y) = geometry::toolbar_position(
            sel,
            tw as f64,
            th as f64,
            self.window.width() as f64,
            self.window.height() as f64,
            TOOLBAR_GAP,
        );
        self.toolbar.set_margin_start(x.round() as i32);
        self.toolbar.set_margin_top(y.round() as i32);
    }

    /// Save the selection through a file dialog (the portal FileChooser on
    /// GNOME), then close.
    fn save(self: &Rc<Self>) {
        let Some(image) = self.image.borrow().clone() else {
            return;
        };
        if self.saving.replace(true) {
            return;
        }
        let dialog = gtk::FileDialog::builder()
            .title("Save screenshot")
            .initial_name(default_file_name())
            .modal(true)
            .build();
        if let Some(dir) = glib::user_special_dir(glib::UserDirectory::Pictures)
            .filter(|d| d.is_dir())
            .or_else(|| Some(glib::home_dir()))
        {
            dialog.set_initial_folder(Some(&gio::File::for_path(dir)));
        }
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let result = dialog.save_future(Some(&ui.window)).await;
            ui.saving.set(false);
            match result {
                Ok(file) => match output::save_png(&file, &image) {
                    Ok(()) => ui.close(),
                    Err(err) => ui.show_error("Could not save the screenshot", &err),
                },
                // The user closed the dialog: back to the selection.
                Err(err) if err.matches(gtk::DialogError::Dismissed) => {}
                Err(err) => ui.show_error("Could not open the file dialog", &err),
            }
        });
    }

    fn show_error(&self, message: &str, err: &glib::Error) {
        eprintln!("klip: {message}: {err}");
        gtk::AlertDialog::builder()
            .message(message)
            .detail(err.message())
            .modal(true)
            .build()
            .show(Some(&self.window));
    }

    /// Cancel / Esc / after saving. Without a selection klip just exits. With
    /// one, the copy stays on the clipboard: hide the window and keep serving
    /// it until another app takes over.
    fn close(&self) {
        if self.saving.get() {
            return;
        }
        if self.image.borrow().is_none() {
            self.app.quit();
            return;
        }
        self.window.set_visible(false);
        output::keep_clipboard_alive(&self.app, &self.window.clipboard());
    }
}

/// e.g. `klip-2026-11-23_20-59-03.png`
fn default_file_name() -> String {
    glib::DateTime::now_local()
        .and_then(|now| now.format("klip-%Y-%m-%d_%H-%M-%S.png"))
        .map(|name| name.to_string())
        .unwrap_or_else(|_| "klip.png".to_string())
}
