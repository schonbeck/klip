//! The fullscreen window: the selection view plus the toolbar that appears
//! next to the selection (annotation tools, colors, settings, Copy / Save /
//! Cancel).

use crate::annotate::{self, Tool};
use crate::app::App;
use crate::geometry::{self, PixelRect, Rect};
use crate::selector::SelectionView;
use crate::{output, settings};
use gtk::{gdk, glib, prelude::*};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
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
}
.klip-toolbar button.action {
    min-width: 104px;
    padding: 0 16px;
    font-size: 1.15em;
    font-weight: bold;
}
.klip-toolbar button.tool {
    min-width: 40px;
    padding: 0 4px;
    font-size: 1.3em;
}
.klip-toolbar button.tool:checked {
    background: #3584e4;
    color: white;
}
.klip-toolbar button.swatch {
    min-width: 24px;
    min-height: 24px;
    margin: 8px 0;
    padding: 0;
    border-radius: 14px;
    border: 2px solid rgba(255, 255, 255, 0.85);
    box-shadow: none;
}
.klip-toolbar colorbutton button {
    min-width: 40px;
    padding: 0 6px;
}
";

/// Preset colors (red, yellow, green, blue); keys 1-4. The color button
/// next to them picks any other color.
const SWATCHES: [&str; 4] = ["#e01b24", "#f6d32d", "#2ec27e", "#3584e4"];

/// Annotation tools: tool, label, tooltip, shortcut key.
const TOOLS: [(Tool, &str, &str, char); 5] = [
    (Tool::Select, "⬚", "Select a new region (S)", 's'),
    (Tool::Arrow, "↗", "Arrow (A)", 'a'),
    (Tool::Line, "╱", "Line (L)", 'l'),
    (Tool::Rectangle, "▭", "Rectangle (R)", 'r'),
    (Tool::Ellipse, "◯", "Ellipse (E)", 'e'),
];

/// The toolbar's widgets.
struct Toolbar {
    root: gtk::Box,
    tools: Vec<(Tool, gtk::ToggleButton)>,
    swatches: Vec<(gdk::RGBA, gtk::Button)>,
    color: gtk::ColorDialogButton,
    settings: gtk::Button,
    copy: gtk::Button,
    save: gtk::Button,
    cancel: gtk::Button,
}

thread_local! {
    /// The capture window currently open (at most one).
    static CURRENT: RefCell<Option<Rc<Ui>>> = const { RefCell::new(None) };
}

struct Ui {
    app: Rc<App>,
    window: gtk::ApplicationWindow,
    overlay: gtk::Overlay,
    view: SelectionView,
    toolbar: Toolbar,
    /// Current selection in widget coordinates; the toolbar sits next to it.
    selection: Cell<Option<Rect>>,
    /// The full screenshot.
    texture: gdk::Texture,
    /// The cropped selection (without annotations), once there is one.
    image: RefCell<Option<gdk::Texture>>,
    /// Where `image` was cut from, in screenshot pixels.
    crop: Cell<Option<PixelRect>>,
    /// A dialog is open (folder chooser, error).
    busy: Cell<bool>,
    closed: Cell<bool>,
}

/// Show the frozen screenshot fullscreen and wait for a selection.
pub fn show(app: &Rc<App>, texture: &gdk::Texture) {
    load_css();
    let window = gtk::ApplicationWindow::builder()
        .application(&app.app)
        .title("klip")
        .decorated(false)
        .build();
    let view = SelectionView::new(texture);
    let toolbar = build_toolbar();
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&view));
    overlay.add_overlay(&toolbar.root);
    window.set_child(Some(&overlay));

    let ui = Rc::new(Ui {
        app: app.clone(),
        window,
        overlay: overlay.clone(),
        view: view.clone(),
        toolbar,
        selection: Cell::new(None),
        texture: texture.clone(),
        image: RefCell::default(),
        crop: Cell::new(None),
        busy: Cell::new(false),
        closed: Cell::new(false),
    });

    // `CURRENT` owns the Ui until `close()`.
    CURRENT.with(|c| c.replace(Some(ui.clone())));
    view.connect_selected(glib::clone!(
        #[weak]
        ui,
        move |widget_rect, image_rect| ui.on_selected(widget_rect, image_rect)
    ));
    // Position the toolbar at allocation time, from the overlay's real size.
    overlay.connect_get_child_position(glib::clone!(
        #[weak]
        ui,
        #[upgrade_or]
        None,
        move |_, child| (child == ui.toolbar.root.upcast_ref::<gtk::Widget>())
            .then(|| ui.toolbar_rect())
    ));
    ui.connect_buttons();
    ui.connect_keys();

    ui.window.connect_close_request(glib::clone!(
        #[weak]
        ui,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_| {
            ui.close(false);
            glib::Propagation::Stop
        }
    ));
    if let Some(token) = app.take_activation_token() {
        ui.window.set_startup_id(&token);
    }
    ui.window.fullscreen();
    ui.window.present();
    test_select_hook(&view);
}

/// Raise the open capture window, if any. Returns whether there was one.
pub fn present_current() -> bool {
    CURRENT.with(|c| c.borrow().as_ref().map(|ui| ui.window.present()).is_some())
}

/// Test hook: `KLIP_TEST_SELECT=x,y,w,h` (widget coordinates) makes that
/// selection shortly after startup, for testing without pointer input.
fn test_select_hook(view: &SelectionView) {
    let Ok(spec) = std::env::var("KLIP_TEST_SELECT") else {
        return;
    };
    let n: Vec<f64> = spec
        .split(',')
        .filter_map(|v| v.trim().parse().ok())
        .collect();
    let [x, y, w, h] = n[..] else {
        eprintln!("klip: KLIP_TEST_SELECT must be x,y,w,h");
        return;
    };
    let view = view.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(800), move || {
        view.finish_selection(Rect::new(x, y, w, h));
    });
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
    gtk::Button::builder()
        .child(&content)
        .tooltip_text(tooltip)
        // Keyboard shortcuts are handled by the window, so Enter must never
        // activate whichever button happens to have focus.
        .focusable(false)
        .css_classes(["action"])
        .build()
}

fn separator() -> gtk::Separator {
    gtk::Separator::new(gtk::Orientation::Vertical)
}

fn build_toolbar() -> Toolbar {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    root.add_css_class("klip-toolbar");
    root.set_visible(false);

    let mut group: Option<gtk::ToggleButton> = None;
    let tools = TOOLS
        .iter()
        .map(|&(tool, label, tooltip, _)| {
            let b = gtk::ToggleButton::builder()
                .label(label)
                .tooltip_text(tooltip)
                .focusable(false)
                .css_classes(["tool"])
                .active(tool == Tool::Select)
                .build();
            b.set_group(group.as_ref());
            group.get_or_insert_with(|| b.clone());
            root.append(&b);
            (tool, b)
        })
        .collect();
    root.append(&separator());

    let css = gtk::CssProvider::new();
    let mut swatch_css = String::new();
    let swatches = SWATCHES
        .iter()
        .enumerate()
        .map(|(i, hex)| {
            let class = format!("swatch-{i}");
            swatch_css.push_str(&format!("button.{class} {{ background: {hex}; }}\n"));
            let b = gtk::Button::builder()
                .tooltip_text(format!("Color ({})", i + 1))
                .focusable(false)
                .valign(gtk::Align::Center)
                .css_classes(["swatch", class.as_str()])
                .build();
            root.append(&b);
            (gdk::RGBA::parse(*hex).expect("valid color"), b)
        })
        .collect::<Vec<_>>();
    css.load_from_string(&swatch_css);
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    let color = gtk::ColorDialogButton::new(Some(
        gtk::ColorDialog::builder()
            .title("Annotation color")
            .with_alpha(false)
            .build(),
    ));
    color.set_tooltip_text(Some("Pick any color"));
    color.set_focusable(false);
    color.set_rgba(&swatches[0].0);
    root.append(&color);
    root.append(&separator());

    let settings = gtk::Button::builder()
        .icon_name("emblem-system-symbolic")
        .tooltip_text("Settings")
        .focusable(false)
        .css_classes(["tool"])
        .build();
    root.append(&settings);
    root.append(&separator());

    let copy = button(
        "Copy",
        "edit-copy-symbolic",
        "Copy to the clipboard and close (Enter, Ctrl+C)",
    );
    copy.add_css_class("suggested-action");
    root.append(&copy);
    let save = button(
        "Save",
        "document-save-symbolic",
        "Save as PNG in the screenshot folder and close (Ctrl+S)",
    );
    root.append(&save);
    let cancel = button(
        "Cancel",
        "window-close-symbolic",
        "Close without copying (Esc)",
    );
    root.append(&cancel);
    Toolbar {
        root,
        tools,
        swatches,
        color,
        settings,
        copy,
        save,
        cancel,
    }
}

impl Ui {
    fn connect_buttons(self: &Rc<Self>) {
        let tb = &self.toolbar;
        tb.copy.connect_clicked(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| ui.copy()
        ));
        tb.save.connect_clicked(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| ui.save()
        ));
        tb.cancel.connect_clicked(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| ui.close(false)
        ));
        tb.settings.connect_clicked(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| settings::open(&ui.app, Some(ui.window.upcast_ref()))
        ));
        for (tool, b) in &tb.tools {
            let tool = *tool;
            b.connect_toggled(glib::clone!(
                #[weak(rename_to = ui)]
                self,
                move |b| if b.is_active() {
                    ui.view.set_tool(tool)
                }
            ));
        }
        for (rgba, b) in &tb.swatches {
            let rgba = *rgba;
            b.connect_clicked(glib::clone!(
                #[weak(rename_to = ui)]
                self,
                move |_| ui.pick_color(rgba)
            ));
        }
        tb.color.connect_rgba_notify(glib::clone!(
            #[weak(rename_to = ui)]
            self,
            move |c| ui.view.set_color(c.rgba())
        ));
        self.view.set_color(tb.color.rgba());
    }

    /// Choosing a color also switches from Select to the arrow tool, since
    /// that's clearly what the user is about to do.
    fn pick_color(&self, rgba: gdk::RGBA) {
        self.toolbar.color.set_rgba(&rgba);
        if self.toolbar.tools[0].1.is_active() {
            self.toolbar.tools[1].1.set_active(true);
        }
    }

    /// Shortcut keys for tools (S/A/L/E) and colors (1-4).
    fn handle_tool_key(&self, key: gdk::Key) -> bool {
        let Some(c) = key.to_unicode().map(|c| c.to_ascii_lowercase()) else {
            return false;
        };
        if let Some((_, b)) = TOOLS
            .iter()
            .zip(&self.toolbar.tools)
            .find_map(|(t, b)| (t.3 == c).then_some(b))
        {
            b.set_active(true);
            return true;
        }
        if let Some(rgba) = c
            .to_digit(10)
            .and_then(|d| self.toolbar.swatches.get((d as usize).checked_sub(1)?))
            .map(|(rgba, _)| *rgba)
        {
            self.pick_color(rgba);
            return true;
        }
        false
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
                if ui.busy.get() {
                    return glib::Propagation::Proceed;
                }
                match key {
                    gdk::Key::Escape => ui.close(false),
                    gdk::Key::Return | gdk::Key::KP_Enter => ui.copy(),
                    gdk::Key::c | gdk::Key::C if ctrl => ui.copy(),
                    gdk::Key::s | gdk::Key::S if ctrl => ui.save(),
                    gdk::Key::z | gdk::Key::Z if ctrl => ui.view.undo(),
                    _ if !ctrl && ui.image.borrow().is_some() && ui.handle_tool_key(key) => {}
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            }
        ));
        self.window.add_controller(keys);
    }

    /// A selection was made (or replaced): show the toolbar next to it.
    fn on_selected(&self, widget_rect: Rect, image_rect: Rect) {
        let (w, h) = (self.texture.width(), self.texture.height());
        let Some(px) = geometry::to_pixels(image_rect, w, h) else {
            return;
        };
        crate::debug!("selection {widget_rect:?} -> image pixels {px:?}");
        let image = output::crop(&self.texture, px);
        self.image.replace(Some(image));
        self.crop.set(Some(px));
        self.place_toolbar(widget_rect);
    }

    fn place_toolbar(&self, sel: Rect) {
        self.selection.set(Some(sel));
        self.toolbar.root.set_visible(true);
        self.overlay.queue_allocate();
    }

    /// Where the toolbar goes, in overlay coordinates: next to the selection,
    /// or bottom-center if there is no selection or no usable size yet.
    fn toolbar_rect(&self) -> gdk::Rectangle {
        let bar = &self.toolbar.root;
        let (_, tw, _, _) = bar.measure(gtk::Orientation::Horizontal, -1);
        let (_, th, _, _) = bar.measure(gtk::Orientation::Vertical, tw);
        let (w, h) = (self.overlay.width(), self.overlay.height());
        let (x, y) = match self.selection.get() {
            Some(sel) if w > 0 && h > 0 => geometry::toolbar_position(
                sel,
                tw as f64,
                th as f64,
                w as f64,
                h as f64,
                TOOLBAR_GAP,
            ),
            _ => geometry::bottom_center(tw as f64, th as f64, w as f64, h as f64),
        };
        let rect = gdk::Rectangle::new(x.round() as i32, y.round() as i32, tw, th);
        crate::debug!(
            "window {}x{} scale {} (surface {:?}), selection {:?}, toolbar {:?}",
            w,
            h,
            self.window.scale_factor(),
            self.window.surface().map(|s| s.scale()),
            self.selection.get(),
            rect
        );
        rect
    }

    /// Copy the (annotated) selection to the clipboard and close.
    fn copy(&self) {
        let Some(image) = self.final_image() else {
            return;
        };
        crate::debug!("copying with {} annotations", self.view.shapes().len());
        output::copy_to_clipboard(&self.window.clipboard(), &image);
        self.close(true);
    }

    /// Save to the configured folder (default `~/Pictures/Screenshots`,
    /// created if needed), then close.
    fn save(&self) {
        if self.busy.get() {
            return;
        }
        let Some(image) = self.final_image() else {
            return;
        };
        let dir = self
            .app
            .config
            .borrow()
            .save_dir
            .clone()
            .unwrap_or_else(crate::config::default_save_dir);
        let result = std::fs::create_dir_all(&dir)
            .map_err(|e| e.to_string())
            .and_then(|()| {
                let path = unique_path(&dir, &default_file_name());
                output::save_png(&gtk::gio::File::for_path(&path), &image)
                    .map(|()| path)
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok(path) => {
                crate::debug!("saved {}", path.display());
                println!("{}", path.display());
                self.app.notify_saved(&path);
                self.close(false);
            }
            Err(err) => self.show_error("Could not save the screenshot", &err),
        }
    }

    fn show_error(&self, message: &str, err: &str) {
        eprintln!("klip: {message}: {err}");
        gtk::AlertDialog::builder()
            .message(message)
            .detail(err)
            .modal(true)
            .build()
            .show(Some(&self.window));
    }

    /// The cropped image with the annotations baked in.
    fn final_image(&self) -> Option<gdk::Texture> {
        let image = self.image.borrow().clone()?;
        let shapes = self.view.shapes();
        let crop = self.crop.get()?;
        if shapes.is_empty() {
            return Some(image);
        }
        let ((ox, oy), scale) = self.view.image_transform();
        let offset = (ox - crop.x as f64, oy - crop.y as f64);
        match annotate::bake(&image, &shapes, offset, scale) {
            Ok(baked) => Some(baked),
            Err(err) => {
                eprintln!("klip: could not draw annotations: {err}");
                Some(image)
            }
        }
    }

    /// Copy / Save / Cancel / Esc: close the window. `copied`: the image is
    /// on the clipboard, which this process must keep serving.
    fn close(&self, copied: bool) {
        if self.busy.get() || self.closed.replace(true) {
            return;
        }
        crate::debug!("closing (copied: {copied})");
        self.window.destroy();
        let ui = CURRENT.with(|c| c.take());
        self.app.capture_done(copied);
        drop(ui);
    }
}

/// `dir/name`, or `dir/name-2.png` etc. if that exists already.
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    if !path.exists() {
        return path;
    }
    let stem = name.trim_end_matches(".png");
    (2..)
        .map(|i| dir.join(format!("{stem}-{i}.png")))
        .find(|p| !p.exists())
        .expect("some free name")
}

/// e.g. `klip-2026-11-23_20-59-03.png`
fn default_file_name() -> String {
    glib::DateTime::now_local()
        .and_then(|now| now.format("klip-%Y-%m-%d_%H-%M-%S.png"))
        .map(|name| name.to_string())
        .unwrap_or_else(|_| "klip.png".to_string())
}
