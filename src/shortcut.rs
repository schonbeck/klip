//! The global "take a screenshot" shortcut. Two ways:
//!
//! - On GNOME (default): a custom keybinding (gsettings, media-keys) that
//!   runs `klip`, which hands over to the background klip if one runs. No
//!   dialog, and it works even when klip isn't running.
//! - Elsewhere: the xdg-desktop-portal GlobalShortcuts portal, which works
//!   while klip runs in the background (the desktop may ask once to confirm).
//!   If the portal fails, the GNOME keybinding is tried as a fallback.

use crate::desktop;
use ashpd::desktop::Session;
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;
use gtk::{gio, glib, prelude::*};
use std::cell::RefCell;
use std::rc::Rc;

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
            Backend::Portal => "set in the desktop's keyboard settings; click to change it",
            Backend::Gnome => "active (GNOME keyboard shortcut)",
            Backend::None => "not registered (start klip in the background to use it)",
        }
    }
}

/// A registered portal shortcut; closing it unregisters.
pub struct PortalShortcut {
    portal: Rc<GlobalShortcuts>,
    session: Session<GlobalShortcuts>,
    /// What the desktop actually bound, e.g. "Press <Shift><Super>s".
    trigger: Rc<RefCell<String>>,
    tasks: Vec<glib::JoinHandle<()>>,
}

impl PortalShortcut {
    pub async fn close(self) {
        for task in &self.tasks {
            task.abort();
        }
        self.session.close().await.ok();
    }

    /// The desktop's description of the bound key.
    pub fn trigger_description(&self) -> String {
        self.trigger.borrow().clone()
    }

    /// Whether the desktop offers its own dialog to change the key
    /// (GlobalShortcuts portal version 2). Once bound, desktops like GNOME
    /// own the key: klip's preferred key is only a suggestion.
    pub fn can_configure(&self) -> bool {
        self.portal.version() >= 2
    }

    /// Open the desktop's dialog to change the shortcut.
    pub async fn configure(&self) -> Result<(), String> {
        self.portal
            .configure_shortcuts(&self.session, None, Default::default())
            .await
            .map_err(|e| e.to_string())
    }
}

/// Whether two GTK accelerators mean the same key combination.
pub fn same_accel(a: &str, b: &str) -> bool {
    let norm = |s: &str| portal_trigger(s).map(|t| t.to_lowercase());
    norm(a).is_some() && norm(a) == norm(b)
}

/// The accelerator in a portal trigger description such as
/// "Press <Shift><Super>s" (GNOME), if it has one.
pub fn accel_from_description(description: &str) -> Option<String> {
    let accel = description.trim().trim_start_matches("Press ").trim();
    portal_trigger(accel).map(|_| accel.to_string())
}

/// Human-readable form of a GTK accelerator, e.g. "Shift+Ctrl+P".
pub fn label(accel: &str) -> String {
    gtk::accelerator_parse(accel)
        .map(|(key, mods)| gtk::accelerator_get_label(key, mods).to_string())
        .unwrap_or_else(|| accel.to_string())
}

/// GTK accelerator (`<Control><Shift>p`) to the portal's trigger format
/// (`CTRL+SHIFT+p`, the XDG shortcuts spec).
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
    if rest.is_empty() || rest.contains(char::is_whitespace) {
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

/// Bind `accel` through the GlobalShortcuts portal. `on_activate` runs on
/// each press; `on_changed` when the user changes the key in the desktop's
/// settings (with the new trigger description).
pub async fn register_portal(
    accel: &str,
    on_activate: impl Fn() + 'static,
    on_changed: impl Fn(String) + 'static,
) -> Result<PortalShortcut, String> {
    let err = |e: ashpd::Error| e.to_string();
    let portal = Rc::new(GlobalShortcuts::new().await.map_err(err)?);
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
        "portal shortcut bound: {} (asked for {trigger:?}, portal version {})",
        bound.trigger_description(),
        portal.version()
    );
    let description = Rc::new(RefCell::new(bound.trigger_description().to_string()));
    let mut activated = portal.receive_activated().await.map_err(err)?;
    let mut changed = portal.receive_shortcuts_changed().await.map_err(err)?;
    let activated_task = glib::spawn_future_local(async move {
        while let Some(event) = activated.next().await {
            if event.shortcut_id() == SHORTCUT_ID {
                crate::debug!("portal shortcut activated");
                on_activate();
            }
        }
    });
    let changed_task = glib::spawn_future_local(glib::clone!(
        #[strong]
        description,
        async move {
            while let Some(event) = changed.next().await {
                if let Some(s) = event.shortcuts().iter().find(|s| s.id() == SHORTCUT_ID) {
                    crate::debug!("portal shortcut changed: {}", s.trigger_description());
                    description.replace(s.trigger_description().to_string());
                    on_changed(s.trigger_description().to_string());
                }
            }
        }
    ));
    Ok(PortalShortcut {
        portal,
        session,
        trigger: description,
        tasks: vec![activated_task, changed_task],
    })
}

/// Running on GNOME with its media-keys settings available.
pub fn is_gnome() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .is_ok_and(|d| d.split(':').any(|d| d.eq_ignore_ascii_case("GNOME")))
        && gnome_schemas_present()
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
    fn same_accel() {
        assert!(super::same_accel("<Shift><Super>s", "<Super><Shift>S"));
        assert!(!super::same_accel("<Shift><Super>s", "<Control><Alt>k"));
    }

    #[test]
    fn accel_from_gnome_description() {
        assert_eq!(
            super::accel_from_description("Press <Shift><Super>s").as_deref(),
            Some("<Shift><Super>s")
        );
        assert_eq!(super::accel_from_description("Press something odd"), None);
    }

    #[test]
    fn portal_trigger_format() {
        assert_eq!(
            super::portal_trigger(crate::config::DEFAULT_SHORTCUT).as_deref(),
            Some("CTRL+SHIFT+p")
        );
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
