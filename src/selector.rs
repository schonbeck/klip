//! `SelectionView`: a widget that shows the frozen screenshot, lets the user
//! drag a rectangle with the left mouse button, dims everything outside it
//! and outlines the selection. With an annotation tool active, dragging
//! inside the selection draws a shape instead.

use crate::annotate::{Shape, Tool};
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
        pub tool: Cell<Tool>,
        pub color: Cell<Option<gdk::RGBA>>,
        /// Finished annotations, oldest first.
        pub shapes: RefCell<Vec<Shape>>,
        /// The shape being dragged right now.
        pub preview: Cell<Option<Shape>>,
        /// Bumped on every change to `shapes`.
        pub revision: Cell<u64>,
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
            // Annotations, clipped to the selection.
            let shapes = self.shapes.borrow();
            let preview = self.preview.get();
            if !shapes.is_empty() || preview.is_some() {
                let cr = snapshot.append_cairo(&graphene::Rect::new(sx, sy, sw, sh));
                for shape in shapes.iter().chain(preview.iter()) {
                    shape.draw(&cr);
                }
            }
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

    pub fn set_tool(&self, tool: Tool) {
        self.imp().tool.set(tool);
    }

    pub fn set_color(&self, color: gdk::RGBA) {
        self.imp().color.set(Some(color));
    }

    pub fn shapes(&self) -> Vec<Shape> {
        self.imp().shapes.borrow().clone()
    }

    /// Changes whenever the annotations change.
    pub fn revision(&self) -> u64 {
        self.imp().revision.get()
    }

    /// Remove the newest annotation.
    pub fn undo(&self) {
        if self.imp().shapes.borrow_mut().pop().is_some() {
            self.bump();
        }
    }

    fn bump(&self) {
        let imp = self.imp();
        imp.revision.set(imp.revision.get() + 1);
        self.queue_draw();
    }

    /// Maps widget coordinates to screenshot pixels:
    /// `image = offset + widget * scale`.
    pub fn image_transform(&self) -> ((f64, f64), (f64, f64)) {
        let src = self.source_region();
        let scale = (src.w / self.width() as f64, src.h / self.height() as f64);
        ((src.x, src.y), scale)
    }

    fn drawing(&self) -> bool {
        self.imp().tool.get() != Tool::Select
    }

    fn setup_drag(&self) {
        let drag = gtk::GestureDrag::builder()
            .button(gdk::BUTTON_PRIMARY)
            .build();
        drag.connect_drag_begin(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |_, x, y| {
                crate::debug!("drag begin at ({x}, {y}), tool {:?}", view.imp().tool.get());
                let imp = view.imp();
                let inside = imp.committed.get().is_some_and(|r| r.contains(x, y));
                if view.drawing() && inside {
                    imp.preview.set(Some(Shape {
                        tool: imp.tool.get(),
                        start: (x, y),
                        end: (x, y),
                        color: imp.color.get().unwrap_or(gdk::RGBA::RED),
                    }));
                }
            }
        ));
        drag.connect_drag_update(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |gesture, dx, dy| {
                let Some((x, y)) = gesture.start_point() else {
                    return;
                };
                if !view.drawing() {
                    view.set_selection(Some(Rect::from_points(x, y, x + dx, y + dy)));
                } else if let Some(mut shape) = view.imp().preview.get() {
                    shape.end = (x + dx, y + dy);
                    view.imp().preview.set(Some(shape));
                    view.queue_draw();
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
                if !view.drawing() {
                    view.finish_selection(Rect::from_points(x, y, x + dx, y + dy));
                } else if let Some(mut shape) = view.imp().preview.take() {
                    shape.end = (x + dx, y + dy);
                    if !shape.is_tiny() {
                        view.imp().shapes.borrow_mut().push(shape);
                    }
                    view.bump();
                }
            }
        ));
        self.add_controller(drag);
    }

    /// End a selection (from a drag, or programmatically for tests).
    pub fn finish_selection(&self, sel: Rect) {
        self.set_selection(Some(sel));
        match self.imp().selection.get() {
            Some(sel) if sel.w >= MIN_SIZE && sel.h >= MIN_SIZE => {
                let image_rect = geometry::widget_to_image(
                    sel,
                    self.width() as f64,
                    self.height() as f64,
                    self.source_region(),
                );
                self.imp().committed.set(Some(sel));
                // Annotations belong to the old selection.
                self.imp().shapes.borrow_mut().clear();
                self.bump();
                if let Some(cb) = self.imp().on_selected.borrow().as_ref() {
                    cb(sel, image_rect);
                }
            }
            // Just a click: keep the previous selection, if any.
            _ => self.set_selection(self.imp().committed.get()),
        }
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
