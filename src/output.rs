//! What happens with the selected region: crop it, put it on the clipboard,
//! save it as a PNG file, and stay alive while we own the clipboard.

use crate::geometry::PixelRect;
use gtk::{gdk, gio, glib, prelude::*};
use std::time::Duration;

/// Give up holding the clipboard after this long.
const CLIPBOARD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Cut `r` out of `texture`.
pub fn crop(texture: &gdk::Texture, r: PixelRect) -> gdk::Texture {
    let mut downloader = gdk::TextureDownloader::new(texture);
    downloader.set_format(gdk::MemoryFormat::R8g8b8a8);
    let (data, stride) = downloader.download_bytes();

    let row = r.w as usize * 4;
    let mut out = Vec::with_capacity(row * r.h as usize);
    for y in r.y..r.y + r.h {
        let start = y as usize * stride + r.x as usize * 4;
        out.extend_from_slice(&data[start..start + row]);
    }
    gdk::MemoryTexture::new(
        r.w,
        r.h,
        gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_owned(out),
        row,
    )
    .upcast()
}

/// Put `image` on the clipboard as `image/png`.
pub fn copy_to_clipboard(clipboard: &gdk::Clipboard, image: &gdk::Texture) {
    let png = image.save_to_png_bytes();
    let provider = gdk::ContentProvider::new_union(&[
        gdk::ContentProvider::for_bytes("image/png", &png),
        // Also offer the texture itself, so GTK apps can paste it directly.
        gdk::ContentProvider::for_value(&image.to_value()),
    ]);
    if let Err(err) = clipboard.set_content(Some(&provider)) {
        eprintln!("klip: could not set clipboard: {err}");
    }
}

/// Write `image` to `file` as PNG.
pub fn save_png(file: &gio::File, image: &gdk::Texture) -> Result<(), glib::Error> {
    let png = image.save_to_png_bytes();
    file.replace_contents(
        &png,
        None,
        false,
        gio::FileCreateFlags::REPLACE_DESTINATION,
        gio::Cancellable::NONE,
    )?;
    Ok(())
}

/// Keep the process running (without a window) while we own the clipboard.
/// On Wayland the clipboard content is served by the owning process, so
/// exiting would empty it. Quits when another app takes over the clipboard,
/// or after the timeout. Quits right away if we don't own it.
pub fn keep_clipboard_alive(app: &gtk::Application, clipboard: &gdk::Clipboard) {
    if !clipboard.is_local() {
        app.quit();
        return;
    }
    clipboard.connect_local_notify(glib::clone!(
        #[weak]
        app,
        move |clipboard| {
            if !clipboard.is_local() {
                app.quit();
            }
        }
    ));
    // The hold keeps the app alive without a visible window until then.
    let hold = app.hold();
    glib::timeout_add_local_once(
        CLIPBOARD_TIMEOUT,
        glib::clone!(
            #[weak]
            app,
            move || {
                drop(hold);
                app.quit();
            }
        ),
    );
}
