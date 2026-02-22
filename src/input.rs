#[cfg(target_arch = "wasm32")]
use glam::Vec2;
use glam::Vec3;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys as web;

use std::collections::HashMap;

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
    pub voice: usize,
    pub plane_z_world: f32,
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

/// Tracks all active pointer positions for multitouch gesture detection.
#[derive(Default, Clone)]
pub struct MultiTouchState {
    /// Active pointers keyed by pointer_id, storing canvas-pixel positions.
    pub pointers: HashMap<i32, [f32; 2]>,
    /// Whether a two-finger gesture is in progress.
    pub gesture_active: bool,
    /// Distance between the two fingers when the gesture started (px).
    pub initial_distance: f32,
    /// Angle of the line between the two fingers when the gesture started (rad).
    pub initial_angle: f32,
    /// BPM snapshot when the two-finger gesture started.
    pub initial_bpm: f32,
    /// Detune snapshot when the two-finger gesture started (cents).
    pub initial_detune: f32,
}

impl MultiTouchState {
    /// Returns (distance, angle) between the two tracked pointers, or None.
    pub fn two_finger_metrics(&self) -> Option<(f32, f32)> {
        if self.pointers.len() < 2 {
            return None;
        }
        let mut iter = self.pointers.values();
        let a = *iter.next().unwrap();
        let b = *iter.next().unwrap();
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
        let angle = dy.atan2(dx);
        Some((dist, angle))
    }

    /// Returns the midpoint UV of the two tracked pointers, given canvas size.
    pub fn midpoint_uv(&self, w_px: f32, h_px: f32) -> Option<[f32; 2]> {
        if self.pointers.len() < 2 {
            return None;
        }
        let mut iter = self.pointers.values();
        let a = *iter.next().unwrap();
        let b = *iter.next().unwrap();
        let mx = (a[0] + b[0]) * 0.5;
        let my = (a[1] + b[1]) * 0.5;
        Some([
            (mx / w_px).clamp(0.0, 1.0),
            (my / h_px).clamp(0.0, 1.0),
        ])
    }
}
#[inline]
pub fn ray_sphere(ray_origin: Vec3, ray_dir: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let oc = ray_origin - center;
    let b = oc.dot(ray_dir);
    let c = oc.dot(oc) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let sqrt_disc = disc.sqrt();
    let t_near = -b - sqrt_disc;
    if t_near >= 0.0 {
        return Some(t_near);
    }
    let t_far = -b + sqrt_disc;
    (t_far >= 0.0).then_some(t_far)
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

// ---------------- Selection helpers ----------------
#[inline]
pub fn nearest_index_by_uvx(normalized_voice_xs: &[f32], uvx: f32) -> usize {
    let mut best_i = 0usize;
    let mut best_dx = f32::MAX;
    for (i, vx) in normalized_voice_xs.iter().enumerate() {
        let dx = (uvx - *vx).abs();
        if dx < best_dx {
            best_dx = dx;
            best_i = i;
        }
    }
    best_i
}
