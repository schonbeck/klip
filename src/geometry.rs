//! Pure geometry helpers: selection rectangles and mapping widget
//! coordinates (logical pixels) to screenshot pixels.

/// An axis-aligned rectangle with a non-negative width and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    /// The rectangle spanned by two corner points, in any order.
    pub fn from_points(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self::new(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs())
    }

    /// Intersection with `other` (empty rectangle if they don't overlap).
    pub fn clamp_to(self, other: Rect) -> Self {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.w).min(other.x + other.w);
        let y1 = (self.y + self.h).min(other.y + other.h);
        Self::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    }

    /// Union bounding box of several rectangles.
    pub fn bounding(rects: &[Rect]) -> Option<Self> {
        let first = *rects.first()?;
        Some(rects.iter().skip(1).fold(first, |a, b| {
            let x0 = a.x.min(b.x);
            let y0 = a.y.min(b.y);
            let x1 = (a.x + a.w).max(b.x + b.w);
            let y1 = (a.y + a.h).max(b.y + b.h);
            Rect::new(x0, y0, x1 - x0, y1 - y0)
        }))
    }
}

/// Integer pixel rectangle inside an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Which part of the screenshot (in image pixels) belongs to the monitor
/// the window is on.
///
/// The portal returns one image covering all monitors. We assume it spans
/// the bounding box of the monitors' logical layout, uniformly scaled.
/// With a single monitor this is simply the whole image.
pub fn monitor_region(image_w: f64, image_h: f64, monitors: &[Rect], current: Rect) -> Rect {
    let whole = Rect::new(0.0, 0.0, image_w, image_h);
    let Some(layout) = Rect::bounding(monitors) else {
        return whole;
    };
    if monitors.len() < 2 || layout.w <= 0.0 || layout.h <= 0.0 {
        return whole;
    }
    let sx = image_w / layout.w;
    let sy = image_h / layout.h;
    Rect::new(
        (current.x - layout.x) * sx,
        (current.y - layout.y) * sy,
        current.w * sx,
        current.h * sy,
    )
    .clamp_to(whole)
}

/// Map a selection in widget coordinates to image pixels, given the widget
/// size and the image region the widget displays (stretched to fill it).
/// This is where HiDPI scaling is handled: the ratio image/widget size is
/// the effective scale factor.
pub fn widget_to_image(sel: Rect, widget_w: f64, widget_h: f64, source: Rect) -> Rect {
    let kx = source.w / widget_w;
    let ky = source.h / widget_h;
    Rect::new(
        source.x + sel.x * kx,
        source.y + sel.y * ky,
        sel.w * kx,
        sel.h * ky,
    )
}

/// Round a rectangle outward to whole pixels and clamp it to the image.
/// Returns `None` if nothing is left.
pub fn to_pixels(r: Rect, image_w: i32, image_h: i32) -> Option<PixelRect> {
    let x0 = (r.x.floor() as i32).clamp(0, image_w);
    let y0 = (r.y.floor() as i32).clamp(0, image_h);
    let x1 = ((r.x + r.w).ceil() as i32).clamp(0, image_w);
    let y1 = ((r.y + r.h).ceil() as i32).clamp(0, image_h);
    (x1 > x0 && y1 > y0).then(|| PixelRect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    })
}

/// Top-left corner for a toolbar of size `tw` x `th` next to `sel`, inside
/// an area of `w` x `h`: below the selection, right-aligned with it; above it
/// if there is no room below; inside its bottom edge if neither fits. Always
/// kept on screen.
pub fn toolbar_position(sel: Rect, tw: f64, th: f64, w: f64, h: f64, gap: f64) -> (f64, f64) {
    let x = (sel.x + sel.w - tw).clamp(0.0, (w - tw).max(0.0));
    let below = sel.y + sel.h + gap;
    let above = sel.y - gap - th;
    let y = if below + th <= h {
        below
    } else if above >= 0.0 {
        above
    } else {
        sel.y + sel.h - th - gap
    };
    (x, y.clamp(0.0, (h - th).max(0.0)))
}

/// Fallback toolbar position: bottom center, 24 px above the edge.
pub fn bottom_center(tw: f64, th: f64, w: f64, h: f64) -> (f64, f64) {
    (((w - tw) / 2.0).max(0.0), (h - th - 24.0).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_points_normalizes() {
        assert_eq!(
            Rect::from_points(10.0, 20.0, 4.0, 2.0),
            Rect::new(4.0, 2.0, 6.0, 18.0)
        );
    }

    #[test]
    fn hidpi_scale_two() {
        // 1920x1080 logical monitor at scale 2 -> 3840x2160 screenshot.
        let source = Rect::new(0.0, 0.0, 3840.0, 2160.0);
        let r = widget_to_image(Rect::new(10.0, 20.0, 100.0, 50.0), 1920.0, 1080.0, source);
        assert_eq!(r, Rect::new(20.0, 40.0, 200.0, 100.0));
    }

    #[test]
    fn single_monitor_is_whole_image() {
        let m = Rect::new(0.0, 0.0, 1280.0, 800.0);
        assert_eq!(
            monitor_region(2560.0, 1600.0, &[m], m),
            Rect::new(0.0, 0.0, 2560.0, 1600.0)
        );
    }

    #[test]
    fn second_monitor_region() {
        let left = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let right = Rect::new(1920.0, 0.0, 1920.0, 1080.0);
        let r = monitor_region(3840.0, 1080.0, &[left, right], right);
        assert_eq!(r, Rect::new(1920.0, 0.0, 1920.0, 1080.0));
    }

    #[test]
    fn pixels_round_outward_and_clamp() {
        let p = to_pixels(Rect::new(-3.0, 1.5, 10.2, 2.0), 5, 5).unwrap();
        assert_eq!(
            p,
            PixelRect {
                x: 0,
                y: 1,
                w: 5,
                h: 3
            }
        );
        assert!(to_pixels(Rect::new(10.0, 10.0, 5.0, 5.0), 5, 5).is_none());
    }

    #[test]
    fn toolbar_below_above_inside() {
        let (w, h) = (1000.0, 800.0);
        // Room below: right-aligned under the selection.
        let sel = Rect::new(100.0, 100.0, 300.0, 200.0);
        assert_eq!(
            toolbar_position(sel, 200.0, 50.0, w, h, 8.0),
            (200.0, 308.0)
        );
        // At the bottom: goes above.
        let sel = Rect::new(100.0, 500.0, 300.0, 280.0);
        assert_eq!(
            toolbar_position(sel, 200.0, 50.0, w, h, 8.0),
            (200.0, 442.0)
        );
        // Whole screen selected: inside, bottom right.
        let sel = Rect::new(0.0, 0.0, w, h);
        assert_eq!(
            toolbar_position(sel, 200.0, 50.0, w, h, 8.0),
            (800.0, 742.0)
        );
        // Narrow selection at the left edge: kept on screen.
        let sel = Rect::new(0.0, 100.0, 50.0, 50.0);
        assert_eq!(toolbar_position(sel, 200.0, 50.0, w, h, 8.0).0, 0.0);
    }
}
