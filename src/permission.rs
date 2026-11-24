//! Screenshot permission. GNOME's screenshot portal asks "Allow klip to Take
//! Screenshots?" the first time an app uses it. Installing klip already
//! expresses that wish, so klip records the answer up front in the portal
//! permission store, the same entry GNOME's dialog writes (and that Settings
//! → Privacy → Screenshots shows and can revoke; `flatpak permission-set` is
//! the CLI equivalent). An existing answer, including "no", is left alone.

use gtk::{gio, glib, prelude::*};

const STORE: &str = "org.freedesktop.impl.portal.PermissionStore";
const STORE_PATH: &str = "/org/freedesktop/impl/portal/PermissionStore";
/// Table and entry id used by xdg-desktop-portal's Screenshot portal.
const TABLE: &str = "screenshot";
const ID: &str = "screenshot";

async fn call(method: &str, args: glib::Variant) -> Result<glib::Variant, glib::Error> {
    let bus = gio::bus_get_future(gio::BusType::Session).await?;
    bus.call_future(
        Some(STORE),
        STORE_PATH,
        STORE,
        method,
        Some(&args),
        None,
        gio::DBusCallFlags::NONE,
        5000,
    )
    .await
}

/// The stored screenshot permission of `app_id`, if any.
async fn lookup(app_id: &str) -> Result<Option<Vec<String>>, glib::Error> {
    let reply = match call("Lookup", (TABLE, ID).to_variant()).await {
        Ok(reply) => reply,
        // No such table yet: nothing stored.
        Err(err) if err.message().contains("No entry") || err.message().contains("NotFound") => {
            return Ok(None);
        }
        Err(err) => return Err(err),
    };
    // (a{sas}v): app id -> permissions, plus data.
    let perms: std::collections::HashMap<String, Vec<String>> =
        reply.child_value(0).get().unwrap_or_default();
    Ok(perms.get(app_id).cloned())
}

/// Record "yes" for `app_id` unless the user already answered. Errors are
/// only logged: worst case GNOME asks as usual.
pub async fn grant_screenshot(app_id: &str) {
    match lookup(app_id).await {
        Ok(Some(existing)) => {
            crate::debug!("screenshot permission for {app_id:?} already set: {existing:?}");
            return;
        }
        Ok(None) => {}
        Err(err) => {
            crate::debug!("permission store lookup failed: {err}");
            return;
        }
    }
    let args = (TABLE, true, ID, app_id, vec!["yes"]).to_variant();
    match call("SetPermission", args).await {
        Ok(_) => crate::debug!("screenshot permission granted for {app_id:?}"),
        Err(err) => crate::debug!("could not set the screenshot permission: {err}"),
    }
}
