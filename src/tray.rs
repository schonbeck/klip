//! Tray icon (StatusNotifierItem) while klip runs in the background. Needs a
//! StatusNotifierWatcher, e.g. GNOME's AppIndicator extension (enabled on
//! Ubuntu) or KDE; without one klip silently goes without a tray icon.

use futures_channel::mpsc::{UnboundedSender, unbounded};
use futures_util::StreamExt;
use gtk::glib;
use gtk::prelude::*;
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
    pixmaps: Vec<ksni::Icon>,
}

/// klip's icon, rendered from `data/io.github.schonbeck.klip.svg`, embedded so
/// the tray shows it however klip was installed: a tray host may not find a
/// themed icon by name (Homebrew's icon dir isn't in the desktop's search
/// path, and hosts handle `IconThemePath` differently).
const PIXMAP_PNGS: [&[u8]; 6] = [
    include_bytes!("../data/tray/klip-16.png"),
    include_bytes!("../data/tray/klip-22.png"),
    include_bytes!("../data/tray/klip-24.png"),
    include_bytes!("../data/tray/klip-32.png"),
    include_bytes!("../data/tray/klip-48.png"),
    include_bytes!("../data/tray/klip-64.png"),
];

/// The embedded icon as StatusNotifierItem pixmaps: ARGB32 in network byte
/// order (bytes A, R, G, B), not premultiplied, which is GDK's `A8r8g8b8`.
fn pixmaps() -> Vec<ksni::Icon> {
    PIXMAP_PNGS
        .iter()
        .filter_map(|png| {
            let texture = gtk::gdk::Texture::from_bytes(&glib::Bytes::from_static(png)).ok()?;
            let (w, h) = (texture.width(), texture.height());
            let mut dl = gtk::gdk::TextureDownloader::new(&texture);
            dl.set_format(gtk::gdk::MemoryFormat::A8r8g8b8);
            let (bytes, stride) = dl.download_bytes();
            let row = w as usize * 4;
            let data = (0..h as usize)
                .flat_map(|y| bytes[y * stride..y * stride + row].iter().copied())
                .collect();
            Some(ksni::Icon {
                width: w,
                height: h,
                data,
            })
        })
        .collect()
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
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.pixmaps.clone()
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
        // Hosts try the name first and the pixmaps if it isn't found. GNOME's
        // AppIndicator extension shows a placeholder instead when looking up
        // a name fails, e.g. one only in a theme path it can't use, so only
        // name the icon where every host finds it (the .deb); otherwise
        // (Homebrew, self-built) the pixmaps alone.
        icon: if crate::desktop::themed_icon_installed() {
            crate::APP_ID.into()
        } else {
            String::new()
        },
        pixmaps: pixmaps(),
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

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_pixmaps_decode_as_argb() {
        let icons = super::pixmaps();
        let sizes: Vec<i32> = icons.iter().map(|i| i.width).collect();
        assert_eq!(sizes, [16, 22, 24, 32, 48, 64]);
        for icon in &icons {
            assert_eq!(icon.width, icon.height);
            assert_eq!(icon.data.len(), (icon.width * icon.height * 4) as usize);
        }
        // The middle of the 32 px icon is opaque (alpha is the first byte).
        let i = &icons[3];
        let mid = ((16 * i.width + 16) * 4) as usize;
        assert_eq!(i.data[mid], 255);
    }
}
