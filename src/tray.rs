//! Tray icon (StatusNotifierItem) while klip runs in the background. Needs a
//! StatusNotifierWatcher, e.g. GNOME's AppIndicator extension (enabled on
//! Ubuntu) or KDE; without one klip silently goes without a tray icon.

use futures_channel::mpsc::{UnboundedSender, unbounded};
use futures_util::StreamExt;
use gtk::glib;
use ksni::TrayMethods;

#[derive(Debug, Clone, Copy)]
pub enum Command {
    Capture,
    Settings,
    About,
    Quit,
}

struct KlipTray {
    tx: UnboundedSender<(Command, Option<String>)>,
    icon: String,
}

impl KlipTray {
    fn send(&self, cmd: Command) {
        // On Wayland, raising a window needs the activation token of the click.
        let token = ksni::current_xdg_activation_token();
        self.tx.unbounded_send((cmd, token)).ok();
    }
}

impl ksni::Tray for KlipTray {
    fn id(&self) -> String {
        "klip".into()
    }
    fn title(&self) -> String {
        "klip".into()
    }
    fn icon_name(&self) -> String {
        self.icon.clone()
    }
    fn icon_theme_path(&self) -> String {
        crate::desktop::extra_icon_dir()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "klip".into(),
            description: "Click to take a screenshot".into(),
            ..Default::default()
        }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Command::Capture);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        let item = |label: &str, icon: &str, cmd: Command| {
            StandardItem {
                label: label.into(),
                icon_name: icon.into(),
                activate: Box::new(move |t: &mut Self| t.send(cmd)),
                ..Default::default()
            }
            .into()
        };
        vec![
            item(
                "Take Screenshot",
                "applets-screenshooter-symbolic",
                Command::Capture,
            ),
            item("Settings", "emblem-system-symbolic", Command::Settings),
            item("About klip", "help-about-symbolic", Command::About),
            ksni::MenuItem::Separator,
            item("Quit", "application-exit-symbolic", Command::Quit),
        ]
    }
}

/// A running tray icon; dropping it removes the icon.
pub struct Tray {
    handle: ksni::Handle<KlipTray>,
    _task: glib::JoinHandle<()>,
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.handle.shutdown();
    }
}

/// Show the tray icon; `on_command` runs on the GTK main thread with the
/// command and the click's activation token. `None` if there is no tray.
pub async fn spawn(on_command: impl Fn(Command, Option<String>) + 'static) -> Option<Tray> {
    let (tx, mut rx) = unbounded();
    let handle = match (KlipTray {
        tx,
        icon: crate::desktop::icon_name().into(),
    })
    .spawn()
    .await
    {
        Ok(handle) => handle,
        Err(err) => {
            crate::debug!("no tray icon: {err}");
            return None;
        }
    };
    crate::debug!("tray icon registered");
    let task = glib::spawn_future_local(async move {
        while let Some((cmd, token)) = rx.next().await {
            crate::debug!("tray: {cmd:?} (activation token: {})", token.is_some());
            on_command(cmd, token);
        }
    });
    Some(Tray {
        handle,
        _task: task,
    })
}
