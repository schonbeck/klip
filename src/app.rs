//! The application: single instance, command line, background (daemon) mode,
//! first-run question, shortcut registration and clipboard lifetime.

use crate::config::Config;
use crate::shortcut::{self, Backend, PortalShortcut};
use crate::{APP_ID, capture, desktop, settings, tray, ui};
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
                    match ashpd::register_host_app(id).await {
                        Ok(()) => crate::debug!("registered with the portal as {APP_ID}"),
                        Err(err) => crate::debug!("portal host registration failed: {err}"),
                    }
                }
                .boxed_local()
                .shared()
            })
            .clone();
        fut.await;
    }

    pub fn run(self: &Rc<Self>, action: Action) {
        let first_run = !crate::config::path().exists();
        // Keep the app alive while we work asynchronously.
        let hold = self.app.hold();
        let this = self.clone();
        glib::spawn_future_local(async move {
            if first_run && !matches!(action, Action::Settings) {
                this.ask_autostart().await;
            }
            match action {
                Action::Daemon if this.is_daemon() => {
                    println!("klip is already running in the background");
                }
                Action::Daemon => this.become_daemon().await,
                Action::Settings => settings::open(&this, None),
                Action::Capture(file) => this.capture(file).await,
            }
            drop(hold);
        });
    }

    /// First run: ask whether to start at login, and remember the answer.
    async fn ask_autostart(self: &Rc<Self>) {
        let shortcut = shortcut::label(&self.config.borrow().shortcut);
        let dialog = gtk::AlertDialog::builder()
            .message("Start klip automatically at login?")
            .detail(format!(
                "klip then waits in the background, so {shortcut} takes a screenshot \
                 at any time. You can change this later in klip's settings."
            ))
            .buttons(["Not Now", "Start at Login"])
            .cancel_button(0)
            .default_button(1)
            .modal(true)
            .build();
        let yes = dialog.choose_future(None::<&gtk::Window>).await == Ok(1);
        crate::debug!("first run: start at login = {yes}");
        self.config.borrow_mut().autostart = yes;
        self.save_config();
        self.set_autostart(yes).await;
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
            if shortcut::gnome_binding_exists() {
                self.set_gnome(Some(&accel));
            }
            return;
        }
        let old = self.portal_shortcut.take();
        if let Some(old) = old {
            old.close().await;
        }
        if forced != "gnome" {
            self.portal_ready().await;
            let this = Rc::downgrade(self);
            let on_activate = move || {
                if let Some(this) = this.upgrade() {
                    this.run(Action::Capture(None));
                }
            };
            match shortcut::register_portal(&accel, on_activate).await {
                Ok(handle) => {
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
                self.capture.take();
            }
        }
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
