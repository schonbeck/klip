//! Annotations (arrow, line, ellipse): the shapes, how to draw them with
//! cairo, and baking them into the cropped image.

use gtk::{cairo, gdk, glib, prelude::*};
use std::f64::consts::TAU;

/// Stroke width in logical pixels (scaled with the image when baking).
pub const STROKE_WIDTH: f64 = 4.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// Dragging makes a new selection.
    #[default]
    Select,
    Arrow,
    Line,
    Ellipse,
}

/// One annotation, in widget coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub tool: Tool,
    pub start: (f64, f64),
    pub end: (f64, f64),
    pub color: gdk::RGBA,
}

impl Shape {
    /// A click rather than a drag.
    pub fn is_tiny(&self) -> bool {
        (self.end.0 - self.start.0).abs() < 3.0 && (self.end.1 - self.start.1).abs() < 3.0
    }

    pub fn draw(&self, cr: &cairo::Context) {
        let c = self.color;
        cr.set_source_rgba(
            c.red().into(),
            c.green().into(),
            c.blue().into(),
            c.alpha().into(),
        );
        cr.set_line_width(STROKE_WIDTH);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_join(cairo::LineJoin::Round);
        let ((x0, y0), (x1, y1)) = (self.start, self.end);
        match self.tool {
            Tool::Select => {}
            Tool::Line => {
                cr.move_to(x0, y0);
                cr.line_to(x1, y1);
                cr.stroke().ok();
            }
            Tool::Arrow => {
                let angle = (y1 - y0).atan2(x1 - x0);
                let len = (x1 - x0).hypot(y1 - y0);
                let head = (STROKE_WIDTH * 4.5).min(len * 0.6);
                let back =
                    |d: f64, a: f64| (x1 - d * (angle + a).cos(), y1 - d * (angle + a).sin());
                // Shaft stops inside the head so the tip stays sharp.
                let (bx, by) = back(head * 0.7, 0.0);
                cr.move_to(x0, y0);
                cr.line_to(bx, by);
                cr.stroke().ok();
                let (lx, ly) = back(head, 0.45);
                let (rx, ry) = back(head, -0.45);
                cr.move_to(x1, y1);
                cr.line_to(lx, ly);
                cr.line_to(rx, ry);
                cr.close_path();
                cr.fill_preserve().ok();
                cr.stroke().ok();
            }
            Tool::Ellipse => {
                let (rx, ry) = ((x1 - x0).abs() / 2.0, (y1 - y0).abs() / 2.0);
                if rx < 0.5 || ry < 0.5 {
                    return;
                }
                cr.save().ok();
                cr.translate((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                cr.scale(rx, ry);
                cr.arc(0.0, 0.0, 1.0, 0.0, TAU);
                cr.restore().ok();
                // Stroke after restore so the line width isn't stretched.
                cr.stroke().ok();
            }
        }
    }
}

/// Draw `shapes` onto `image`. A widget point `p` maps to image pixel
/// `offset + p * scale`.
pub fn bake(
    image: &gdk::Texture,
    shapes: &[Shape],
    offset: (f64, f64),
    scale: (f64, f64),
) -> Result<gdk::Texture, cairo::Error> {
    let (w, h) = (image.width(), image.height());
    // GDK's B8g8r8a8Premultiplied is cairo's ARGB32 on little-endian.
    let format = gdk::MemoryFormat::B8g8r8a8Premultiplied;
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h)?;
    let stride = surface.stride() as usize;
    {
        let mut downloader = gdk::TextureDownloader::new(image);
        downloader.set_format(format);
        let (src, src_stride) = downloader.download_bytes();
        let mut dst = surface.data().map_err(|_| cairo::Error::SurfaceFinished)?;
        let row = w as usize * 4;
        for y in 0..h as usize {
            dst[y * stride..y * stride + row]
                .copy_from_slice(&src[y * src_stride..y * src_stride + row]);
        }
    }
    {
        let cr = cairo::Context::new(&surface)?;
        cr.translate(offset.0, offset.1);
        cr.scale(scale.0, scale.1);
        for shape in shapes {
            shape.draw(&cr);
        }
    }
    surface.flush();
    let data = surface
        .take_data()
        .map_err(|_| cairo::Error::SurfaceFinished)?;
    Ok(gdk::MemoryTexture::new(w, h, format, &glib::Bytes::from(&data[..]), stride).upcast())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bake_draws_into_the_image() {
        let (w, h) = (40, 20);
        let white = glib::Bytes::from_owned(vec![255u8; (w * h * 4) as usize]);
        let image: gdk::Texture =
            gdk::MemoryTexture::new(w, h, gdk::MemoryFormat::R8g8b8a8, &white, w as usize * 4)
                .upcast();
        let line = Shape {
            tool: Tool::Line,
            start: (0.0, 5.0),
            end: (20.0, 5.0),
            color: gdk::RGBA::new(1.0, 0.0, 0.0, 1.0),
        };
        // Widget coords at half the image's pixel density, offset by 2 px.
        let baked = bake(&image, &[line], (2.0, 0.0), (2.0, 2.0)).unwrap();
        assert_eq!((baked.width(), baked.height()), (w, h));
        let mut dl = gdk::TextureDownloader::new(&baked);
        dl.set_format(gdk::MemoryFormat::R8g8b8a8);
        let (px, stride) = dl.download_bytes();
        let at = |x: usize, y: usize| &px[y * stride + x * 4..y * stride + x * 4 + 4];
        assert_eq!(at(20, 10), [255, 0, 0, 255]); // on the line
        assert_eq!(at(20, 2), [255, 255, 255, 255]); // above it
    }

    #[test]
    fn tiny_shapes() {
        let mut s = Shape {
            tool: Tool::Arrow,
            start: (10.0, 10.0),
            end: (11.0, 12.0),
            color: gdk::RGBA::RED,
        };
        assert!(s.is_tiny());
        s.end = (30.0, 10.0);
        assert!(!s.is_tiny());
    }
}
