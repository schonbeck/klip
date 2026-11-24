//! What happens with the selected region: crop it, put it on the clipboard,
//! and save it as a PNG file.

use crate::geometry::PixelRect;
use gtk::{gdk, gio, glib, prelude::*};
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
