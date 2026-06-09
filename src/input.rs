#[cfg(target_arch = "wasm32")]
use glam::Vec2;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys as web;

use std::collections::HashMap;

pub const MAX_TOUCH_POINTS: usize = 5;

#[derive(Default, Clone, Copy)]
pub struct MouseState {
    pub x: f32,
    pub y: f32,
    pub down: bool,
    pub gesture_energy: f32,
    pub gesture_flash: f32,
    pub gesture_spin: f32,
}
#[derive(Default, Clone, Copy)]
pub struct DragState {
    pub active: bool,
    pub pending: bool,
    pub start_x: f32,
    pub start_y: f32,
    pub last_x: f32,
    pub last_y: f32,
    pub travel_px: f32,
    pub spin_accum: f32,
    pub peak_motion: f32,
    pub last_ripple_travel: f32,
    pub last_reseed_time: f64,
}

#[derive(Default, Clone, Copy)]
pub struct RippleEvent {
    pub uv: [f32; 2],
    pub amp: f32,
}

/// The kind of multitouch gesture currently in progress.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TouchGestureKind {
    #[default]
    None,
    /// Two or more fingers: continuous performance surface.
    PerformanceSurface,
}

/// Tracks all active pointer positions for multitouch gesture detection.
#[derive(Default, Clone)]
pub struct MultiTouchState {
    /// Active pointers keyed by pointer_id, storing canvas-pixel positions.
    pub pointers: HashMap<i32, [f32; 2]>,
    /// The current gesture kind (locked when the gesture begins).
    pub gesture_kind: TouchGestureKind,
    /// Peak simultaneous pointer count during this gesture.
    pub peak_pointer_count: usize,
    // ── Performance-surface baseline state ──
    /// Distance between the baseline finger pair when the gesture started (px).
    pub initial_distance: f32,
    /// Angle of the line between the two fingers when the gesture started (rad).
    pub initial_angle: f32,
    /// BPM snapshot when the performance gesture started.
    pub initial_bpm: f32,
    /// Detune snapshot when the performance gesture started (cents).
    pub initial_detune: f32,
    /// Centroid of all pointers when the performance gesture began (px).
    pub initial_centroid: [f32; 2],
    /// Running centroid of all pointers, updated on every pointermove.
    pub current_centroid: Option<[f32; 2]>,
    /// Accumulated centroid travel during the current multi-finger gesture.
    pub motion_px: f32,
    /// Last accumulated motion value that emitted a surface ripple.
    pub last_ripple_motion: f32,
}

impl MultiTouchState {
    /// Returns the positions of the two pointers with the lowest IDs, sorted by ID.
    /// This ensures deterministic ordering regardless of HashMap iteration order.
    fn sorted_pair(&self) -> Option<([f32; 2], [f32; 2])> {
        if self.pointers.len() < 2 {
            return None;
        }
        let mut ids: Vec<i32> = self.pointers.keys().copied().collect();
        ids.sort_unstable();
        let a = self.pointers[&ids[0]];
        let b = self.pointers[&ids[1]];
        Some((a, b))
    }

    /// Returns (distance, angle) between the two lowest-ID tracked pointers, or None.
    pub fn two_finger_metrics(&self) -> Option<(f32, f32)> {
        let (a, b) = self.sorted_pair()?;
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
        let angle = dy.atan2(dx);
        Some((dist, angle))
    }

    /// Returns the midpoint UV of the two lowest-ID tracked pointers, given canvas size.
    pub fn midpoint_uv(&self, w_px: f32, h_px: f32) -> Option<[f32; 2]> {
        let (a, b) = self.sorted_pair()?;
        let mx = (a[0] + b[0]) * 0.5;
        let my = (a[1] + b[1]) * 0.5;
        Some([(mx / w_px).clamp(0.0, 1.0), (my / h_px).clamp(0.0, 1.0)])
    }

    /// Returns the centroid (average position) of all tracked pointers in pixels.
    pub fn centroid_px(&self) -> Option<[f32; 2]> {
        let n = self.pointers.len();
        if n == 0 {
            return None;
        }
        let (sx, sy) = self
            .pointers
            .values()
            .fold((0.0_f32, 0.0_f32), |(ax, ay), p| (ax + p[0], ay + p[1]));
        Some([sx / n as f32, sy / n as f32])
    }

    /// Returns the centroid as UV coordinates given canvas pixel dimensions.
    pub fn centroid_uv(&self, w_px: f32, h_px: f32) -> Option<[f32; 2]> {
        self.centroid_px()
            .map(|[cx, cy]| [(cx / w_px).clamp(0.0, 1.0), (cy / h_px).clamp(0.0, 1.0)])
    }

    /// Mean distance from active pointers to their centroid, in canvas pixels.
    pub fn spread_px(&self) -> Option<f32> {
        let centroid = self.centroid_px()?;
        let n = self.pointers.len();
        if n < 2 {
            return None;
        }
        let sum = self
            .pointers
            .values()
            .map(|p| {
                let dx = p[0] - centroid[0];
                let dy = p[1] - centroid[1];
                (dx * dx + dy * dy).sqrt()
            })
            .sum::<f32>();
        Some((sum / n as f32).max(1.0))
    }

    /// Sorted active touches as shader-ready `[u, v, intensity, slot]` rows.
    pub fn touch_points_uv(&self, w_px: f32, h_px: f32) -> ([[f32; 4]; MAX_TOUCH_POINTS], usize) {
        let mut out = [[0.0_f32; 4]; MAX_TOUCH_POINTS];
        let mut ids: Vec<i32> = self.pointers.keys().copied().collect();
        ids.sort_unstable();
        let count = ids.len().min(MAX_TOUCH_POINTS);
        for (slot, id) in ids.into_iter().take(MAX_TOUCH_POINTS).enumerate() {
            let p = self.pointers[&id];
            out[slot] = [
                (p[0] / w_px).clamp(0.0, 1.0),
                (p[1] / h_px).clamp(0.0, 1.0),
                1.0,
                slot as f32,
            ];
        }
        (out, count)
    }

    /// Resets all gesture state, keeping the pointer map intact.
    pub fn reset_gesture(&mut self) {
        self.gesture_kind = TouchGestureKind::None;
        self.peak_pointer_count = 0;
        self.initial_distance = 0.0;
        self.initial_angle = 0.0;
        self.initial_bpm = 0.0;
        self.initial_detune = 0.0;
        self.initial_centroid = [0.0, 0.0];
        self.current_centroid = None;
        self.motion_px = 0.0;
        self.last_ripple_motion = 0.0;
    }
}
#[inline]
#[cfg(target_arch = "wasm32")]
pub fn pointer_canvas_px(ev: &web::PointerEvent, canvas: &web::HtmlCanvasElement) -> Vec2 {
    let el: web::Element = canvas.clone().unchecked_into();
    let rect = el.get_bounding_client_rect();
    let x_css = ev.client_x() as f32 - rect.left() as f32;
    let y_css = ev.client_y() as f32 - rect.top() as f32;
    let sx = (x_css / rect.width() as f32) * canvas.width() as f32;
    let sy = (y_css / rect.height() as f32) * canvas.height() as f32;
    Vec2::new(sx, sy)
}

#[inline]
#[cfg(target_arch = "wasm32")]
pub fn pointer_canvas_uv(ev: &web::PointerEvent, canvas: &web::HtmlCanvasElement) -> [f32; 2] {
    let el: web::Element = canvas.clone().unchecked_into();
    let rect = el.get_bounding_client_rect();
    let x_css = ev.client_x() as f32 - rect.left() as f32;
    let y_css = ev.client_y() as f32 - rect.top() as f32;
    let w = rect.width() as f32;
    let h = rect.height() as f32;
    if w > 0.0 && h > 0.0 {
        let u = (x_css / w).clamp(0.0, 1.0);
        let v = (y_css / h).clamp(0.0, 1.0);
        [u, v]
    } else {
        [0.5, 0.5]
    }
}

#[inline]
#[cfg(target_arch = "wasm32")]
pub fn mouse_uv(canvas: &web::HtmlCanvasElement, mouse: &MouseState) -> [f32; 2] {
    let w = canvas.width().max(1) as f32;
    let h = canvas.height().max(1) as f32;
    [(mouse.x / w).clamp(0.0, 1.0), (mouse.y / h).clamp(0.0, 1.0)]
}
