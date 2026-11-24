//! The application: single instance, command line, background (daemon) mode,
//! first-run question, shortcut registration and clipboard lifetime.

use crate::config::Config;
use crate::shortcut::{self, Backend, PortalShortcut};
use crate::{APP_ID, capture, desktop, permission, settings, tray, ui};
use futures_util::FutureExt;
use futures_util::future::{LocalBoxFuture, Shared};
use gtk::{gio, glib, prelude::*};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// Stop serving the clipboard after this long (when not in daemon mode).
const CLIPBOARD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub enum Action {
    Capture(Option<PathBuf>),
    Daemon,
    Settings,
}

pub struct App {
    pub app: gtk::Application,
    pub config: RefCell<Config>,
    /// Running in the background (`--daemon`, or after enabling autostart).
    daemon: RefCell<Option<gio::ApplicationHoldGuard>>,
    /// Held while a capture is in progress.
    capture: RefCell<Option<gio::ApplicationHoldGuard>>,
    /// Held while we own the clipboard (outside daemon mode).
    clipboard: RefCell<Option<gio::ApplicationHoldGuard>>,
    clipboard_watched: Cell<bool>,
    /// Bumped on each copy, so an older timeout doesn't drop a newer hold.
    clipboard_generation: Cell<u32>,
    portal_shortcut: RefCell<Option<PortalShortcut>>,
    tray: RefCell<Option<tray::Tray>>,
    activation_token: RefCell<Option<String>>,
    pub shortcut_backend: Cell<Backend>,
    /// Portal host-app registration; must finish before other portal calls.
    registered: RefCell<Option<Shared<LocalBoxFuture<'static, ()>>>>,
}

impl App {
    pub fn new(app: &gtk::Application) -> Rc<Self> {
        // Desktops use it to name the app (notifications, portals).
        if let Err(err) = desktop::install_launcher() {
            eprintln!("klip: could not write the .desktop file: {err}");
        }
        // Homebrew's icon dir isn't one GTK searches by itself.
        if let (Some(dir), Some(display)) =
            (desktop::extra_icon_dir(), gtk::gdk::Display::default())
        {
            gtk::IconTheme::for_display(&display).add_search_path(dir);
        }
        Rc::new(Self {
            app: app.clone(),
            config: RefCell::new(Config::load().unwrap_or_default()),
            daemon: RefCell::default(),
            capture: RefCell::default(),
            clipboard: RefCell::default(),
            clipboard_watched: Cell::new(false),
            clipboard_generation: Cell::new(0),
            portal_shortcut: RefCell::default(),
            tray: RefCell::default(),
            activation_token: RefCell::default(),
            shortcut_backend: Cell::new(if shortcut::gnome_binding_exists() {
                Backend::Gnome
            } else {
                Backend::None
            }),
            registered: RefCell::default(),
        })
    }

    pub fn is_daemon(&self) -> bool {
        self.daemon.borrow().is_some()
    }

    pub fn save_config(&self) {
        if let Err(err) = self.config.borrow().save() {
            eprintln!("klip: could not save settings: {err}");
        }
    }

    /// Register as a host app with the portal (app id for permissions,
    /// GlobalShortcuts). Done once; every portal user awaits this first.
    pub async fn portal_ready(&self) {
        let fut = self
            .registered
            .borrow_mut()
            .get_or_insert_with(|| {
                async {
                    let id = ashpd::AppID::try_from(APP_ID).expect("valid app id");
                    // The app id the portal knows us by: ours once registered,
                    // otherwise "" (all unsandboxed apps).
                    let portal_id = match ashpd::register_host_app(id).await {
                        Ok(()) => {
                            crate::debug!("registered with the portal as {APP_ID}");
                            APP_ID
                        }
                        Err(err) => {
                            crate::debug!("portal host registration failed: {err}");
                            ""
                        }
                    };
                    permission::grant_screenshot(portal_id).await;
                }
                .boxed_local()
                .shared()
            })
            .clone();
        fut.await;
    }

    pub fn run(self: &Rc<Self>, action: Action) {
        // Keep the app alive while we work asynchronously.
        let hold = self.app.hold();
        let this = self.clone();
        glib::spawn_future_local(async move {
            this.first_run_defaults();
            match action {
                Action::Daemon if this.is_daemon() => {
                    println!("klip is already running in the background");
                }
                Action::Daemon => this.become_daemon().await,
                Action::Settings => {
                    // Normally the detached background klip runs this; if it
                    // couldn't be started, this process stays in the
                    // background so closing the window doesn't quit klip.
                    if !this.is_daemon() {
                        this.become_daemon().await;
                    }
                    settings::open(&this, None)
                }
                Action::Capture(file) => {
                    // If the detached background klip couldn't be started,
                    // this process stays in the tray after the capture window
                    // closes (Copy, Save or Esc).
                    if !this.is_daemon() {
                        this.become_daemon().await;
                    }
                    this.capture(file).await
                }
            }
            drop(hold);
        });
    }

    /// First launch (no config file yet): save the defaults and turn on
    /// start at login, without asking. Later launches leave the user's
    /// choices alone.
    fn first_run_defaults(&self) {
        if crate::config::path().exists() {
            return;
        }
        crate::debug!("first launch: saving default settings");
        self.save_config();
        if self.config.borrow().autostart
            && let Err(err) = desktop::set_autostart(true)
        {
            eprintln!("klip: could not enable start at login: {err}");
        }
    }

    /// Apply the "start at login" setting; turning it on also keeps this
    /// process running in the background right away.
    pub async fn set_autostart(self: &Rc<Self>, enabled: bool) {
        if let Err(err) = desktop::set_autostart(enabled) {
            eprintln!("klip: could not update autostart: {err}");
        }
        if enabled && !self.is_daemon() {
            self.become_daemon().await;
        }
    }

    async fn become_daemon(self: &Rc<Self>) {
        self.daemon.replace(Some(self.app.hold()));
        crate::debug!("running in the background");
        // Keep the autostart entry pointing at this binary (an older one may
        // name a path that's gone, e.g. a Homebrew Cellar version).
        if self.config.borrow().autostart
            && let Err(err) = desktop::set_autostart(true)
        {
            eprintln!("klip: could not update start at login: {err}");
        }
        self.apply_shortcut().await;
        let this = Rc::downgrade(self);
        let tray = tray::spawn(move |cmd, token| {
            let Some(this) = this.upgrade() else { return };
            if let Some(token) = token {
                // Lets the next window take focus on Wayland.
                this.activation_token.replace(Some(token));
            }
            match cmd {
                // Let the tray menu close first, so it's not in the shot.
                tray::Command::Capture => {
                    glib::timeout_add_local_once(Duration::from_millis(300), move || {
                        this.run(Action::Capture(None))
                    });
                }
                tray::Command::Settings => this.run(Action::Settings),
                tray::Command::About => {
                    let token = this.take_activation_token();
                    crate::about::show(&this.app, None, token.as_deref());
                }
                tray::Command::Quit => this.app.quit(),
            }
        })
        .await;
        self.tray.replace(tray);
    }

    /// The activation token from a tray click, if any, for the next window.
    pub fn take_activation_token(&self) -> Option<String> {
        self.activation_token.take()
    }

    /// (Re-)register the configured shortcut: portal first, GNOME keybinding
    /// as fallback. `KLIP_SHORTCUT_BACKEND=portal|gnome` forces one.
    pub async fn apply_shortcut(self: &Rc<Self>) {
        let accel = self.config.borrow().shortcut.clone();
        let forced = std::env::var("KLIP_SHORTCUT_BACKEND").unwrap_or_default();
        if !self.is_daemon() {
            // Only the GNOME keybinding works without a running daemon.
            if shortcut::gnome_binding_exists() || shortcut::is_gnome() {
                self.set_gnome(Some(&accel));
            }
            return;
        }
        let old = self.portal_shortcut.take();
        if let Some(old) = old {
            old.close().await;
        }
        // On GNOME the custom keybinding needs no confirmation dialog (the
        // portal shows "Add Keyboard Shortcuts" and then keeps its own copy
        // of the key), so it's the default there; the portal elsewhere.
        let prefer_gnome = forced.is_empty() && shortcut::is_gnome();
        if forced != "gnome" && !prefer_gnome {
            self.portal_ready().await;
            let this = Rc::downgrade(self);
            let on_activate = move || {
                if let Some(this) = this.upgrade() {
                    this.run(Action::Capture(None));
                }
            };
            let this = Rc::downgrade(self);
            let on_changed = move |description: String| {
                let Some(this) = this.upgrade() else { return };
                // Keep the config in step with what the desktop bound.
                if let Some(accel) = shortcut::accel_from_description(&description) {
                    this.config.borrow_mut().shortcut = accel;
                    this.save_config();
                }
                settings::refresh(&this);
            };
            match shortcut::register_portal(&accel, on_activate, on_changed).await {
                Ok(handle) => {
                    // Once the user accepted a key, GNOME keeps it and treats
                    // klip's key as a mere suggestion. If it differs from the
                    // configured one, use the GNOME keybinding instead, so the
                    // key set in klip's settings is the one that works.
                    let bound = shortcut::accel_from_description(&handle.trigger_description());
                    if let Some(bound) = bound
                        && !shortcut::same_accel(&bound, &accel)
                        && forced != "portal"
                    {
                        crate::debug!(
                            "the desktop keeps {bound}, not {accel}: using GNOME's keybinding"
                        );
                        handle.close().await;
                        self.set_gnome(Some(&accel));
                        return;
                    }
                    self.portal_shortcut.replace(Some(handle));
                    self.shortcut_backend.set(Backend::Portal);
                    // Don't trigger twice.
                    if shortcut::gnome_binding_exists() {
                        shortcut::set_gnome_binding(None).ok();
                    }
                    return;
                }
                Err(err) => eprintln!("klip: global shortcut portal: {err}"),
            }
        }
        if forced != "portal" {
            self.set_gnome(Some(&accel));
        }
    }

    /// The shortcut as the user sees it, e.g. "Ctrl+Alt+P".
    pub fn shortcut_label(&self) -> String {
        shortcut::label(&self.config.borrow().shortcut)
    }

    /// With the portal, the desktop owns the key and has its own dialog to
    /// change it. True if that's the case (then use `configure_shortcut`).
    pub fn desktop_owns_shortcut(&self) -> bool {
        self.portal_shortcut
            .borrow()
            .as_ref()
            .is_some_and(|p| p.can_configure())
    }

    /// Open the desktop's dialog for changing the shortcut.
    pub async fn configure_shortcut(&self) -> Result<(), String> {
        // Don't hold the RefCell borrow across the await.
        let portal = self.portal_shortcut.take();
        let result = match &portal {
            Some(p) => p.configure().await,
            None => Err("no portal shortcut".into()),
        };
        if self.portal_shortcut.borrow().is_none() {
            self.portal_shortcut.replace(portal);
        }
        result
    }

    fn set_gnome(&self, accel: Option<&str>) {
        match shortcut::set_gnome_binding(accel) {
            Ok(()) => {
                crate::debug!("GNOME keybinding set to {accel:?}");
                self.shortcut_backend.set(if accel.is_some() {
                    Backend::Gnome
                } else {
                    Backend::None
                });
            }
            Err(err) => {
                eprintln!("klip: could not set the GNOME keyboard shortcut: {err}");
                self.shortcut_backend.set(Backend::None);
            }
        }
    }

    async fn capture(self: &Rc<Self>, file: Option<PathBuf>) {
        if self.capture.borrow().is_some() {
            crate::debug!("capture already in progress");
            ui::present_current();
            return;
        }
        self.capture.replace(Some(self.app.hold()));
        let from_portal = file.is_none();
        let texture = match file {
            Some(path) => capture::from_file(&path),
            None => {
                self.portal_ready().await;
                capture::from_portal().await
            }
        };
        match texture {
            Ok(texture) => ui::show(self, &texture),
            Err(err) => {
                eprintln!("klip: could not take screenshot: {err}");
                // With a hold but no window, a one-shot klip stays alive for the dialog.
                if from_portal && self.screenshot_failed(&err.to_string()).await {
                    self.capture.take();
                    Box::pin(self.capture(None)).await;
                    return;
                }
                self.capture.take();
            }
        }
    }

    /// Tell the user the screenshot failed and offer to try again; returns
    /// true once a retry worked (permission granted), so the caller takes the
    /// real screenshot.
    ///
    /// The first time, GNOME asks "Allow screenshots?", but GNOME Shell can't
    /// show that dialog while no app has focus (e.g. shortcut pressed on the
    /// empty desktop). So the retry runs while this window is still open and
    /// focused; its screenshot is thrown away.
    async fn screenshot_failed(&self, err: &str) -> bool {
        let (tx, rx) = futures_channel::oneshot::channel::<bool>();
        let tx = Rc::new(RefCell::new(Some(tx)));
        let window = gtk::Window::builder()
            .application(&self.app)
            .title("klip")
            .resizable(false)
            .build();
        let label = gtk::Label::builder()
            .label(format!(
                "Could not take a screenshot:\n{err}\n\n\
                 If the desktop has not asked yet whether klip may take \
                 screenshots, click Try Again."
            ))
            .wrap(true)
            .max_width_chars(50)
            .build();
        let retry = gtk::Button::with_label("Try Again");
        retry.add_css_class("suggested-action");
        let close = gtk::Button::with_label("Close");
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.set_halign(gtk::Align::End);
        buttons.append(&close);
        buttons.append(&retry);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
        for m in ["top", "bottom", "start", "end"] {
            content.set_property(&format!("margin-{m}"), 20);
        }
        content.append(&label);
        content.append(&buttons);
        window.set_child(Some(&content));
        let finish = {
            let tx = tx.clone();
            move |ok: bool| {
                if let Some(tx) = tx.borrow_mut().take() {
                    tx.send(ok).ok();
                }
            }
        };
        window.connect_close_request({
            let finish = finish.clone();
            move |_| {
                finish(false);
                glib::Propagation::Proceed
            }
        });
        close.connect_clicked(glib::clone!(
            #[weak]
            window,
            move |_| window.close()
        ));
        retry.connect_clicked(glib::clone!(
            #[weak]
            label,
            move |b| {
                b.set_sensitive(false);
                let (b, label, finish) = (b.clone(), label.clone(), finish.clone());
                glib::spawn_future_local(async move {
                    match capture::from_portal().await {
                        Ok(_) => finish(true),
                        Err(err) => {
                            label.set_label(&format!("Still failing:\n{err}"));
                            b.set_sensitive(true);
                        }
                    }
                });
            }
        ));
        window.present();
        let ok = rx.await.unwrap_or(false);
        window.destroy();
        if ok {
            // Let the window disappear before the real screenshot.
            glib::timeout_future(Duration::from_millis(400)).await;
        }
        ok
    }

    /// The capture window closed. `copied`: we now own the clipboard.
    pub fn capture_done(self: &Rc<Self>, copied: bool) {
        if copied {
            self.watch_clipboard();
            if !self.is_daemon() {
                // On Wayland the clipboard is served by its owner: stay alive.
                let hold = self.app.hold();
                self.clipboard.replace(Some(hold));
                let generation = self.clipboard_generation.get().wrapping_add(1);
                self.clipboard_generation.set(generation);
                let this = Rc::downgrade(self);
                glib::timeout_add_local_once(CLIPBOARD_TIMEOUT, move || {
                    if let Some(this) = this.upgrade()
                        && this.clipboard_generation.get() == generation
                    {
                        crate::debug!("clipboard timeout");
                        this.clipboard.take();
                    }
                });
            }
        }
        self.capture.take();
    }

    /// "Screenshot saved" desktop notification.
    pub fn notify_saved(&self, path: &std::path::Path) {
        let n = gio::Notification::new("Screenshot saved");
        n.set_body(Some(&path.display().to_string()));
        self.app.send_notification(Some("saved"), &n);
        // Give the message a moment to go out before a one-shot klip exits.
        let hold = self.app.hold();
        glib::timeout_add_local_once(Duration::from_millis(500), move || drop(hold));
    }

    /// Let go of the clipboard hold when another app takes over.
    fn watch_clipboard(self: &Rc<Self>) {
        if self.clipboard_watched.replace(true) {
            return;
        }
        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };
        let this = Rc::downgrade(self);
        display.clipboard().connect_local_notify(move |cb| {
            if !cb.is_local() {
                crate::debug!("clipboard taken over by another app");
                // Quits unless the daemon or a capture still holds the app.
                if let Some(this) = this.upgrade() {
                    this.clipboard.take();
                }
            }
        });
    }
}
