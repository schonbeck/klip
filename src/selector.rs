//! `SelectionView`: a widget that shows the frozen screenshot, lets the user
//! drag a rectangle with the left mouse button, dims everything outside it
//! and outlines the selection.

use crate::geometry::{self, Rect};
use gtk::{gdk, glib, graphene, gsk, prelude::*, subclass::prelude::*};

const DIM: gdk::RGBA = gdk::RGBA::new(0.0, 0.0, 0.0, 0.45);
const OUTLINE: gdk::RGBA = gdk::RGBA::new(0.21, 0.52, 0.89, 1.0);
const OUTLINE_WIDTH: f32 = 2.0;
/// Selections smaller than this (in logical pixels) are treated as a click
/// and ignored.
const MIN_SIZE: f64 = 3.0;

/// Callback receiving a finished selection: (widget coordinates, image pixels).
type SelectedFn = Box<dyn Fn(Rect, Rect)>;

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct SelectionView {
        pub texture: RefCell<Option<gdk::Texture>>,
        /// Current selection in widget coordinates.
        pub selection: Cell<Option<Rect>>,
        /// The last finished selection (widget coordinates).
        pub committed: Cell<Option<Rect>>,
        /// Called when a drag ends with a usable selection.
        pub on_selected: RefCell<Option<SelectedFn>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SelectionView {
        const NAME: &'static str = "KlipSelectionView";
        type Type = super::SelectionView;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for SelectionView {}

    impl WidgetImpl for SelectionView {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let Some(texture) = self.texture.borrow().clone() else {
                return;
            };
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            let full = graphene::Rect::new(0.0, 0.0, w, h);

            // Draw the screenshot so that `source` fills the widget.
            let source = obj.source_region();
            let kx = w / source.w as f32;
            let ky = h / source.h as f32;
            let bounds = graphene::Rect::new(
                -source.x as f32 * kx,
                -source.y as f32 * ky,
                texture.width() as f32 * kx,
                texture.height() as f32 * ky,
            );
            snapshot.push_clip(&full);
            snapshot.append_scaled_texture(&texture, gsk::ScalingFilter::Linear, &bounds);
            snapshot.pop();

            let Some(sel) = self.selection.get() else {
                snapshot.append_color(&DIM, &full);
                return;
            };
            let (sx, sy, sw, sh) = (sel.x as f32, sel.y as f32, sel.w as f32, sel.h as f32);
            // Dim the four bands around the selection.
            for band in [
                graphene::Rect::new(0.0, 0.0, w, sy),
                graphene::Rect::new(0.0, sy + sh, w, h - sy - sh),
                graphene::Rect::new(0.0, sy, sx, sh),
                graphene::Rect::new(sx + sw, sy, w - sx - sw, sh),
            ] {
                snapshot.append_color(&DIM, &band);
            }
            // Outline just outside the selection.
            let o = OUTLINE_WIDTH;
            let outline = graphene::Rect::new(sx - o, sy - o, sw + 2.0 * o, sh + 2.0 * o);
            snapshot.append_border(
                &gsk::RoundedRect::from_rect(outline, 0.0),
                &[o; 4],
                &[OUTLINE; 4],
            );
        }
    }
}

glib::wrapper! {
    pub struct SelectionView(ObjectSubclass<imp::SelectionView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl SelectionView {
    pub fn new(texture: &gdk::Texture) -> Self {
        let view: Self = glib::Object::new();
        view.imp().texture.replace(Some(texture.clone()));
        view.set_hexpand(true);
        view.set_vexpand(true);
        view.set_cursor_from_name(Some("crosshair"));
        view.setup_drag();
        view
    }

    /// `f` receives each finished selection, in widget coordinates and in
    /// image pixels. Dragging again replaces the selection.
    pub fn connect_selected(&self, f: impl Fn(Rect, Rect) + 'static) {
        self.imp().on_selected.replace(Some(Box::new(f)));
    }

    fn setup_drag(&self) {
        let drag = gtk::GestureDrag::builder()
            .button(gdk::BUTTON_PRIMARY)
            .build();
        drag.connect_drag_update(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |gesture, dx, dy| {
                if let Some((x, y)) = gesture.start_point() {
                    view.set_selection(Some(Rect::from_points(x, y, x + dx, y + dy)));
                }
            }
        ));
        drag.connect_drag_end(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |gesture, dx, dy| {
                let Some((x, y)) = gesture.start_point() else {
                    return;
                };
                view.set_selection(Some(Rect::from_points(x, y, x + dx, y + dy)));
                match view.imp().selection.get() {
                    Some(sel) if sel.w >= MIN_SIZE && sel.h >= MIN_SIZE => {
                        let image_rect = geometry::widget_to_image(
                            sel,
                            view.width() as f64,
                            view.height() as f64,
                            view.source_region(),
                        );
                        view.imp().committed.set(Some(sel));
                        if let Some(cb) = view.imp().on_selected.borrow().as_ref() {
                            cb(sel, image_rect);
                        }
                    }
                    // Just a click: keep the previous selection, if any.
                    _ => view.set_selection(view.imp().committed.get()),
                }
            }
        ));
        self.add_controller(drag);
    }

    fn set_selection(&self, sel: Option<Rect>) {
        let bounds = Rect::new(0.0, 0.0, self.width() as f64, self.height() as f64);
        self.imp().selection.set(sel.map(|r| r.clamp_to(bounds)));
        self.queue_draw();
    }

    /// The part of the screenshot (image pixels) shown in this widget: the
    /// region of the monitor the window is on.
    fn source_region(&self) -> Rect {
        let texture = self.imp().texture.borrow();
        let (tw, th) = texture
            .as_ref()
            .map_or((1.0, 1.0), |t| (t.width() as f64, t.height() as f64));
        let whole = Rect::new(0.0, 0.0, tw, th);

        let display = self.display();
        let Some(current) = self
            .native()
            .and_then(|n| n.surface())
            .and_then(|s| display.monitor_at_surface(&s))
        else {
            return whole;
        };
        let to_rect = |m: &gdk::Monitor| {
            let g = m.geometry();
            Rect::new(
                g.x() as f64,
                g.y() as f64,
                g.width() as f64,
                g.height() as f64,
            )
        };
        let monitors: Vec<Rect> = display
            .monitors()
            .iter::<gdk::Monitor>()
            .filter_map(Result::ok)
            .map(|m| to_rect(&m))
            .collect();
        geometry::monitor_region(tw, th, &monitors, to_rect(&current))
    }
}
