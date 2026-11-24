//! The global "take a screenshot" shortcut. Two ways:
//!
//! 1. The xdg-desktop-portal GlobalShortcuts portal: works while the daemon
//!    runs; the desktop may ask the user to confirm the shortcut once.
//! 2. Fallback on GNOME: a custom keybinding (gsettings, media-keys) that runs
//!    `klip`, which hands over to the daemon if one runs.

use crate::desktop;
use ashpd::desktop::Session;
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;
use gtk::{gio, glib, prelude::*};

const MEDIA_KEYS: &str = "org.gnome.settings-daemon.plugins.media-keys";
const CUSTOM: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const CUSTOM_PATH: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/klip/";
const SHORTCUT_ID: &str = "capture";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Portal,
    Gnome,
    None,
}

impl Backend {
    pub fn describe(self) -> &'static str {
        match self {
            Backend::Portal => "active (desktop portal, while klip runs in the background)",
            Backend::Gnome => "active (GNOME keyboard shortcut)",
            Backend::None => "not registered (start klip in the background to use it)",
        }
    }
}

/// A registered portal shortcut; dropping or closing it unregisters.
pub struct PortalShortcut {
    session: Session<GlobalShortcuts>,
    task: glib::JoinHandle<()>,
}

impl PortalShortcut {
    pub async fn close(self) {
        self.task.abort();
        self.session.close().await.ok();
    }
}

/// Human-readable form of a GTK accelerator, e.g. "Shift+Super+S".
pub fn label(accel: &str) -> String {
    gtk::accelerator_parse(accel)
        .map(|(key, mods)| gtk::accelerator_get_label(key, mods).to_string())
        .unwrap_or_else(|| accel.to_string())
}

/// GTK accelerator (`<Super><Shift>s`) to the portal's trigger format
/// (`LOGO+SHIFT+s`, the XDG shortcuts spec).
pub fn portal_trigger(accel: &str) -> Option<String> {
    let mut rest = accel.trim();
    let mut mods = [false; 4]; // CTRL, ALT, SHIFT, LOGO
    while let Some(tail) = rest.strip_prefix('<') {
        let (name, tail) = tail.split_once('>')?;
        let i = match name.to_ascii_lowercase().as_str() {
            "control" | "ctrl" | "primary" => 0,
            "alt" | "mod1" => 1,
            "shift" => 2,
            "super" | "meta" | "hyper" => 3,
            _ => return None,
        };
        mods[i] = true;
        rest = tail;
    }
    if rest.is_empty() {
        return None;
    }
    let mut parts: Vec<&str> = ["CTRL", "ALT", "SHIFT", "LOGO"]
        .iter()
        .zip(mods)
        .filter_map(|(n, on)| on.then_some(*n))
        .collect();
    parts.push(rest);
    Some(parts.join("+"))
}

/// Bind `accel` through the GlobalShortcuts portal; `on_activate` runs on
/// each press.
pub async fn register_portal(
    accel: &str,
    on_activate: impl Fn() + 'static,
) -> Result<PortalShortcut, String> {
    let err = |e: ashpd::Error| e.to_string();
    let portal = GlobalShortcuts::new().await.map_err(err)?;
    let session = portal
        .create_session(Default::default())
        .await
        .map_err(err)?;
    let trigger = portal_trigger(accel);
    let shortcut = NewShortcut::new(SHORTCUT_ID, "Take a region screenshot")
        .preferred_trigger(trigger.as_deref());
    let bound = portal
        .bind_shortcuts(&session, &[shortcut], None, Default::default())
        .await
        .map_err(err)?
        .response()
        .map_err(err)?;
    let Some(bound) = bound.shortcuts().iter().find(|s| s.id() == SHORTCUT_ID) else {
        session.close().await.ok();
        return Err("the shortcut was not accepted".into());
    };
    crate::debug!(
        "portal shortcut bound: {} (asked for {trigger:?})",
        bound.trigger_description()
    );
    let mut activated = portal.receive_activated().await.map_err(err)?;
    let task = glib::spawn_future_local(async move {
        let _portal = portal;
        while let Some(event) = activated.next().await {
            if event.shortcut_id() == SHORTCUT_ID {
                crate::debug!("portal shortcut activated");
                on_activate();
            }
        }
    });
    Ok(PortalShortcut { session, task })
}

fn gnome_schemas_present() -> bool {
    gio::SettingsSchemaSource::default()
        .is_some_and(|s| s.lookup(MEDIA_KEYS, true).is_some() && s.lookup(CUSTOM, true).is_some())
}

/// Whether klip's GNOME custom keybinding exists.
pub fn gnome_binding_exists() -> bool {
    gnome_schemas_present()
        && gio::Settings::new(MEDIA_KEYS)
            .strv("custom-keybindings")
            .iter()
            .any(|p| p == CUSTOM_PATH)
}

/// Create/update (`Some`) or remove (`None`) klip's GNOME custom keybinding.
pub fn set_gnome_binding(accel: Option<&str>) -> Result<(), String> {
    if !gnome_schemas_present() {
        return Err("GNOME media-keys settings are not installed".into());
    }
    let media_keys = gio::Settings::new(MEDIA_KEYS);
    let mut paths: Vec<String> = media_keys
        .strv("custom-keybindings")
        .iter()
        .map(|p| p.to_string())
        .collect();
    let custom = gio::Settings::with_path(CUSTOM, CUSTOM_PATH);
    let set = |key: &str, value: &str| custom.set_string(key, value).map_err(|e| e.to_string());
    match accel {
        Some(accel) => {
            set("name", "klip: take a screenshot")?;
            set("command", &desktop::exe())?;
            set("binding", accel)?;
            if !paths.iter().any(|p| p == CUSTOM_PATH) {
                paths.push(CUSTOM_PATH.into());
            }
        }
        None => {
            for key in ["name", "command", "binding"] {
                custom.reset(key);
            }
            paths.retain(|p| p != CUSTOM_PATH);
        }
    }
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    media_keys
        .set_strv("custom-keybindings", paths.as_slice())
        .map_err(|e| e.to_string())?;
    gio::Settings::sync();
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn portal_trigger_format() {
        assert_eq!(
            super::portal_trigger("<Super><Shift>s").as_deref(),
            Some("SHIFT+LOGO+s")
        );
        assert_eq!(
            super::portal_trigger("<Control>Print").as_deref(),
            Some("CTRL+Print")
        );
    }
}
