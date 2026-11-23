//! Getting the full-screen image: from the xdg-desktop-portal Screenshot
//! API, or from a file (for testing without a desktop session).

use ashpd::desktop::screenshot::Screenshot;
use gtk::{gdk, gio, prelude::*};
use std::error::Error;
use std::path::Path;

/// Ask the portal for a non-interactive full-screen screenshot and load it.
pub async fn from_portal() -> Result<gdk::Texture, Box<dyn Error>> {
    let response = Screenshot::request()
        .interactive(false)
        .modal(false)
        .send()
        .await?
        .response()?;
    let file = gio::File::for_uri(response.uri().as_str());
    let texture = gdk::Texture::from_file(&file)?;

    // The portal hands us the shot as a file (GNOME writes it to
    // ~/Pictures/Screenshots). It is only a transfer file for klip, so remove
    // it to avoid piling up full-screen screenshots.
    if file.is_native()
        && let Err(err) = file.delete(gio::Cancellable::NONE)
    {
        eprintln!("klip: could not remove portal file {}: {err}", file.uri());
    }
    Ok(texture)
}

/// Load an existing image instead of taking a screenshot (`klip --file`).
pub fn from_file(path: &Path) -> Result<gdk::Texture, Box<dyn Error>> {
    Ok(gdk::Texture::from_file(&gio::File::for_path(path))?)
}
