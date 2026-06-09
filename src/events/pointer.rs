use crate::audio;
use crate::core::{
    Bpm, Cents, MusicEngine, AEOLIAN, DORIAN, IONIAN, LOCRIAN, LYDIAN, MIXOLYDIAN, PHRYGIAN,
};
use crate::input;
use crate::input::TouchGestureKind;
use crate::overlay;
use crate::perf;
use glam::Vec3;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys as web;

const DRAG_START_THRESHOLD_PX: f32 = 8.0;
const RIPPLE_INTERVAL_PX: f32 = 88.0;
const RESEED_INTERVAL_SEC: f64 = 0.22;
const ROOT_TABLE: [i32; 15] = [48, 50, 52, 53, 55, 57, 59, 60, 62, 64, 65, 67, 69, 71, 72];

const TOUCH_BPM_MIN: f32 = 46.0;
const TOUCH_BPM_MAX: f32 = 190.0;
const TOUCH_BPM_SPREAD_DEPTH: f32 = 0.42;
const TOUCH_ROTATE_DETUNE_SENSITIVITY: f32 = 76.0;
const TOUCH_PARAM_BLEND: f32 = 0.13;
const TOUCH_RIPPLE_INTERVAL_PX: f32 = 118.0;

/// Mode scales used when a single-finger carve drop chooses a new mode.
const MODES_ORDER: [&[f32]; 7] = [
    IONIAN, DORIAN, PHRYGIAN, LYDIAN, MIXOLYDIAN, AEOLIAN, LOCRIAN,
];

/// Mode names corresponding to MODES_ORDER.
const MODE_NAMES: [&str; 7] = [
    "Ionian (major)",
    "Dorian",
    "Phrygian",
    "Lydian",
    "Mixolydian",
    "Aeolian (minor)",
    "Locrian",
];

#[derive(Clone)]
pub struct InputWiring {
    pub canvas: web::HtmlCanvasElement,
    pub engine: Rc<RefCell<MusicEngine>>,
    pub mouse_state: Rc<RefCell<input::MouseState>>,
    pub drag_state: Rc<RefCell<input::DragState>>,
    pub multi_touch: Rc<RefCell<input::MultiTouchState>>,
    pub paused: Rc<RefCell<bool>>,
    pub voice_gains: Rc<Vec<web::GainNode>>,
    pub delay_sends: Rc<Vec<web::GainNode>>,
    pub reverb_sends: Rc<Vec<web::GainNode>>,
    pub audio_ctx: web::AudioContext,
    pub queued_ripple_uv: Rc<RefCell<Option<input::RippleEvent>>>,
}

pub fn wire_input_handlers(w: InputWiring) {
    wire_pointermove(&w);
    wire_pointerdown(&w);
    wire_pointerup(&w);
    wire_pointercancel(&w);
}

impl InputWiring {
    /// Raise the visual gesture energy/flash to at least the given floors.
    fn bump_gesture(&self, energy: f32, flash: f32) {
        let mut ms = self.mouse_state.borrow_mut();
        ms.gesture_energy = ms.gesture_energy.max(energy);
        ms.gesture_flash = ms.gesture_flash.max(flash);
    }

    /// Queue a surface ripple at the given UV with the given amplitude,
    /// replacing any ripple not yet consumed by the render loop. Also records the
    /// ripple for the optional perf bridge (a no-op unless armed), so every
    /// ripple source is captured from one place.
    fn queue_ripple(&self, uv: [f32; 2], amp: f32) {
        *self.queued_ripple_uv.borrow_mut() = Some(input::RippleEvent { uv, amp });
        perf::record_ripple(uv, amp);
    }

    /// Move the tracked cursor to a UV position expressed in canvas pixels,
    /// so single-pointer visuals follow a multitouch centroid or midpoint.
    fn set_mouse_uv(&self, uv: [f32; 2], w_px: f32, h_px: f32) {
        let mut ms = self.mouse_state.borrow_mut();
        ms.x = uv[0] * w_px;
        ms.y = uv[1] * h_px;
    }
}

// ─────────────────────────── pointermove ───────────────────────────

fn wire_pointermove(w: &InputWiring) {
    let w = w.clone();
    let canvas_connected = w.canvas.is_connected();

    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::PointerEvent| {
        let pos = input::pointer_canvas_px(&ev, &w.canvas);
        let w_px = w.canvas.width().max(1) as f32;
        let h_px = w.canvas.height().max(1) as f32;
        let pid = ev.pointer_id();

        if !canvas_connected {
            return;
        }

        // Always update pointer map and running centroid for tracked pointers.
        // During multi-finger play, centroid travel becomes the rate-limited
        // source for visible surface ripples.
        {
            let mut mt = w.multi_touch.borrow_mut();
            if mt.pointers.contains_key(&pid) {
                let prev_centroid = mt.current_centroid.or_else(|| mt.centroid_px());
                mt.pointers.insert(pid, [pos.x, pos.y]);
                let next_centroid = mt.centroid_px();
                if mt.gesture_kind == TouchGestureKind::PerformanceSurface {
                    if let (Some(prev), Some(next)) = (prev_centroid, next_centroid) {
                        let dx = next[0] - prev[0];
                        let dy = next[1] - prev[1];
                        mt.motion_px += (dx * dx + dy * dy).sqrt();
                    }
                }
                mt.current_centroid = next_centroid;
            }
        }

        let gesture_kind = w.multi_touch.borrow().gesture_kind;

        match gesture_kind {
            TouchGestureKind::PerformanceSurface => {
                handle_performance_touch_move(&w, w_px, h_px);
                ev.prevent_default();
                return;
            }
            TouchGestureKind::None => { /* fall through to single-pointer path */ }
        }

        // ── Single-pointer path (existing behavior) ──
        {
            let mut ms = w.mouse_state.borrow_mut();
            ms.x = pos.x;
            ms.y = pos.y;
        }

        let (
            drag_active,
            drag_just_started,
            start_x,
            start_y,
            delta_x,
            delta_y,
            travel_px,
            spin_accum,
            peak_motion,
        ) = {
            let mut ds = w.drag_state.borrow_mut();
            let was_active = ds.active;
            let dx = pos.x - ds.last_x;
            let dy = pos.y - ds.last_y;

            if ds.pending && !ds.active {
                let moved_x = pos.x - ds.start_x;
                let moved_y = pos.y - ds.start_y;
                let moved = (moved_x * moved_x + moved_y * moved_y).sqrt();
                if moved >= DRAG_START_THRESHOLD_PX {
                    ds.active = true;
                }
            }

            let step = (dx * dx + dy * dy).sqrt();
            if ds.active {
                ds.travel_px += step;

                let prev_ang = (ds.last_y - h_px * 0.5).atan2(ds.last_x - w_px * 0.5);
                let mut delta_ang = (pos.y - h_px * 0.5).atan2(pos.x - w_px * 0.5) - prev_ang;
                while delta_ang > std::f32::consts::PI {
                    delta_ang -= std::f32::consts::TAU;
                }
                while delta_ang < -std::f32::consts::PI {
                    delta_ang += std::f32::consts::TAU;
                }
                ds.spin_accum += delta_ang;
                ds.peak_motion = ds.peak_motion.max(step);
            }

            ds.last_x = pos.x;
            ds.last_y = pos.y;

            (
                ds.active,
                !was_active && ds.active,
                ds.start_x,
                ds.start_y,
                dx,
                dy,
                ds.travel_px,
                ds.spin_accum,
                ds.peak_motion,
            )
        };

        if drag_just_started {
            log::info!("[gesture] carve begin");
            let [uvx, uvy] = input::pointer_canvas_uv(&ev, &w.canvas);
            w.queue_ripple([uvx, uvy], 1.35);
            w.bump_gesture(0.55, 0.24);
        }

        if !drag_active {
            return;
        }

        let uvx = (pos.x / w_px).clamp(0.0, 1.0);
        let uvy = (pos.y / h_px).clamp(0.0, 1.0);
        let motion = ((delta_x * delta_x + delta_y * delta_y).sqrt() / 34.0).clamp(0.0, 1.0);
        let travel_n = (travel_px / 760.0).clamp(0.0, 1.0);
        let spin_n = (spin_accum.abs() / 7.0).clamp(0.0, 1.0);

        {
            let mut ms = w.mouse_state.borrow_mut();
            let energy_target =
                (0.35 + 0.55 * travel_n + 0.45 * motion + 0.20 * spin_n).clamp(0.0, 1.8);
            ms.gesture_energy = (ms.gesture_energy * 0.74 + energy_target * 0.26).clamp(0.0, 1.8);
            ms.gesture_flash = (ms.gesture_flash + 0.05 + 0.12 * motion).clamp(0.0, 1.6);
            ms.gesture_spin = (ms.gesture_spin + spin_accum * 0.015).clamp(-4.0, 4.0);
        }

        let reseed_due = {
            let mut ds = w.drag_state.borrow_mut();
            let now = w.audio_ctx.current_time();
            if ds.active && motion > 0.34 && now - ds.last_reseed_time > RESEED_INTERVAL_SEC {
                ds.last_reseed_time = now;
                true
            } else {
                false
            }
        };

        {
            let mut ds = w.drag_state.borrow_mut();
            if ds.active && ds.travel_px - ds.last_ripple_travel >= RIPPLE_INTERVAL_PX {
                ds.last_ripple_travel = ds.travel_px;
                let amp = (1.05 + 1.35 * motion + 0.65 * travel_n).clamp(0.6, 2.4);
                w.queue_ripple([uvx, uvy], amp);
            }
        }

        let mut eng = w.engine.borrow_mut();
        eng.set_bpm(Bpm::new(
            (50.0 + 122.0 * travel_n + 24.0 * motion).clamp(38.0, 180.0),
        ));
        let detune = ((0.5 - uvy) * 220.0 + spin_accum.sin() * 145.0 + (uvx - 0.5) * 90.0)
            .clamp(-200.0, 200.0);
        eng.set_detune_cents(Cents::new(detune));

        let voice_len = eng.voices.len().max(1);
        let base_radius = (0.22 + 0.86 * travel_n).clamp(0.20, 1.12);
        let carve_phase = spin_accum * 0.95
            + (uvx - 0.5) * std::f32::consts::TAU
            + ((pos.x - start_x) / w_px - (pos.y - start_y) / h_px) * 3.1;
        for i in 0..eng.voices.len() {
            let lane = i as f32 / voice_len as f32;
            let phase = carve_phase
                + i as f32 * (std::f32::consts::TAU / voice_len as f32)
                + travel_n * (2.2 + lane * 1.7)
                + spin_n * (0.8 + lane * 1.1);
            let fold = (phase * (1.8 + lane * 0.6)).sin();
            let x = base_radius * (0.46 + 0.52 * lane) * phase.cos() + 0.28 * fold;
            let z = base_radius * (0.78 - 0.30 * lane) * phase.sin() - 0.18 * fold;
            eng.set_voice_position(i, Vec3::new(x.clamp(-1.2, 1.2), 0.0, z.clamp(-1.2, 1.2)));

            let base_prob = match i {
                0 => 0.44,
                1 => 0.58,
                _ => 0.40,
            };
            let mod_prob =
                0.26 * motion + 0.22 * travel_n + 0.10 * (0.5 + 0.5 * (phase * 2.1).sin());
            eng.configs[i].trigger_probability = (base_prob + mod_prob).clamp(0.10, 0.95);
        }
        if reseed_due {
            let vi = ((travel_px / 52.0).floor() as usize + (spin_n * 13.0).round() as usize)
                % voice_len;
            eng.reseed_voice(vi, None);
        }

        if peak_motion > 18.0 {
            let mut ms = w.mouse_state.borrow_mut();
            ms.gesture_flash = (ms.gesture_flash + 0.12).clamp(0.0, 1.8);
        }
    }) as Box<dyn FnMut(_)>);

    if let Some(wnd) = web::window() {
        _ = wnd.add_event_listener_with_callback("pointermove", closure.as_ref().unchecked_ref());
    }

    closure.forget();
}

/// Handle two-or-more-finger performance movement during pointermove.
fn handle_performance_touch_move(w: &InputWiring, w_px: f32, h_px: f32) {
    let (count, centroid_uv, spread_ratio, angle_delta, initial_bpm, initial_detune) = {
        let mt = w.multi_touch.borrow();
        if mt.pointers.len() < 2 {
            return;
        }
        let centroid_uv = mt.centroid_uv(w_px, h_px).unwrap_or([0.5, 0.5]);
        let spread = mt.spread_px().unwrap_or(mt.initial_distance.max(1.0));
        let ratio = (spread / mt.initial_distance.max(1.0)).clamp(0.58, 1.78);
        let angle_delta = if let Some((_dist, angle)) = mt.two_finger_metrics() {
            let mut da = angle - mt.initial_angle;
            while da > std::f32::consts::PI {
                da -= std::f32::consts::TAU;
            }
            while da < -std::f32::consts::PI {
                da += std::f32::consts::TAU;
            }
            da
        } else {
            0.0
        };
        (
            mt.pointers.len(),
            centroid_uv,
            ratio,
            angle_delta,
            mt.initial_bpm,
            mt.initial_detune,
        )
    };

    let count_n = ((count.saturating_sub(1)) as f32 / 4.0).clamp(0.0, 1.0);
    let spread_motion = (spread_ratio - 1.0).abs().clamp(0.0, 0.8);
    let rotate_n = (angle_delta.abs() / std::f32::consts::PI).clamp(0.0, 1.0);
    let bpm_target = (initial_bpm * (1.0 + (spread_ratio - 1.0) * TOUCH_BPM_SPREAD_DEPTH))
        .clamp(TOUCH_BPM_MIN, TOUCH_BPM_MAX);
    let detune_target =
        (initial_detune + angle_delta * TOUCH_ROTATE_DETUNE_SENSITIVITY).clamp(-160.0, 160.0);

    {
        let mut eng = w.engine.borrow_mut();
        let bpm = lerp(eng.params.bpm.get(), bpm_target, TOUCH_PARAM_BLEND);
        let detune = lerp(
            eng.params.detune_cents.get(),
            detune_target,
            TOUCH_PARAM_BLEND,
        );
        eng.set_bpm(Bpm::new(bpm));
        eng.set_detune_cents(Cents::new(detune));

        let voice_len = eng.voices.len().max(1);
        let center_x = (centroid_uv[0] - 0.5) * 1.55;
        let center_z = (0.5 - centroid_uv[1]) * 1.55;
        let base_radius = (0.30 + 0.22 * count_n + 0.30 * spread_motion).clamp(0.25, 0.86);
        let base_phase = angle_delta * 0.85 + centroid_uv[0] * std::f32::consts::TAU;
        for i in 0..eng.voices.len() {
            let lane = i as f32 / voice_len as f32;
            let phase = base_phase
                + i as f32 * (std::f32::consts::TAU / voice_len as f32)
                + count_n * (0.45 + lane * 0.36);
            let target = Vec3::new(
                (center_x * 0.48 + phase.cos() * base_radius * (0.72 + 0.20 * lane))
                    .clamp(-1.2, 1.2),
                0.0,
                (center_z * 0.48 + phase.sin() * base_radius * (0.92 - 0.18 * lane))
                    .clamp(-1.2, 1.2),
            );
            let current = eng.voices[i].position;
            eng.set_voice_position(i, current.lerp(target, 0.16));

            let base_prob = match i {
                0 => 0.42,
                1 => 0.55,
                _ => 0.38,
            };
            let prob_target = (base_prob
                + 0.08 * count_n
                + 0.07 * spread_motion
                + 0.04 * (0.5 + 0.5 * (phase * 1.7).sin()))
            .clamp(0.10, 0.82);
            eng.configs[i].trigger_probability =
                lerp(eng.configs[i].trigger_probability, prob_target, 0.10);
        }
    }

    {
        let mut ms = w.mouse_state.borrow_mut();
        ms.x = centroid_uv[0] * w_px;
        ms.y = centroid_uv[1] * h_px;
        ms.down = true;
        let energy_target =
            (0.34 + 0.36 * count_n + 0.34 * spread_motion + 0.22 * rotate_n).clamp(0.0, 1.8);
        ms.gesture_energy = (ms.gesture_energy * 0.82 + energy_target * 0.18).clamp(0.0, 1.8);
        ms.gesture_flash =
            (ms.gesture_flash + 0.025 + 0.032 * count_n + 0.045 * rotate_n).clamp(0.0, 1.4);
        ms.gesture_spin = lerp(ms.gesture_spin, angle_delta * 0.32, 0.08).clamp(-4.0, 4.0);
    }

    let ripple = {
        let mut mt = w.multi_touch.borrow_mut();
        if mt.motion_px - mt.last_ripple_motion >= TOUCH_RIPPLE_INTERVAL_PX {
            mt.last_ripple_motion = mt.motion_px;
            Some(centroid_uv)
        } else {
            None
        }
    };
    if let Some(uv) = ripple {
        let amp =
            (0.72 + 0.30 * count_n + 0.26 * spread_motion + 0.20 * rotate_n).clamp(0.55, 1.55);
        w.queue_ripple(uv, amp);
    }
}

// ─────────────────────────── pointerdown ───────────────────────────

fn wire_pointerdown(w: &InputWiring) {
    let w = w.clone();
    let canvas_for_listener = w.canvas.clone();

    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::PointerEvent| {
        let pos = input::pointer_canvas_px(&ev, &w.canvas);
        let now = w.audio_ctx.current_time();
        let pid = ev.pointer_id();

        // Register this pointer
        let pointer_count = {
            let mut mt = w.multi_touch.borrow_mut();
            mt.pointers.insert(pid, [pos.x, pos.y]);
            mt.current_centroid = mt.centroid_px();
            let count = mt.pointers.len();
            mt.peak_pointer_count = mt.peak_pointer_count.max(count);
            count
        };

        if pointer_count >= 2 {
            // Cancel any single-finger drag
            {
                let mut ds = w.drag_state.borrow_mut();
                ds.active = false;
                ds.pending = false;
            }

            start_or_update_performance_touch(&w, pointer_count);
        } else {
            // ── Single pointer: existing behavior ──
            {
                let mut ds = w.drag_state.borrow_mut();
                ds.pending = true;
                ds.active = false;
                ds.start_x = pos.x;
                ds.start_y = pos.y;
                ds.last_x = pos.x;
                ds.last_y = pos.y;
                ds.travel_px = 0.0;
                ds.spin_accum = 0.0;
                ds.peak_motion = 0.0;
                ds.last_ripple_travel = 0.0;
                ds.last_reseed_time = now;
            }

            {
                let mut ms = w.mouse_state.borrow_mut();
                ms.down = true;
                ms.x = pos.x;
                ms.y = pos.y;
                ms.gesture_energy = ms.gesture_energy.max(0.15);
                ms.gesture_flash = ms.gesture_flash.max(0.10);
            }

            let [uvx, uvy] = input::pointer_canvas_uv(&ev, &w.canvas);
            w.queue_ripple([uvx, uvy], 1.15);
        }

        _ = w.canvas.set_pointer_capture(pid);
        ev.prevent_default();
    }) as Box<dyn FnMut(_)>);

    _ = canvas_for_listener
        .add_event_listener_with_callback("pointerdown", closure.as_ref().unchecked_ref());
    closure.forget();
}

/// Start or continue the continuous multi-finger performance surface.
fn start_or_update_performance_touch(w: &InputWiring, pointer_count: usize) {
    let w_px = w.canvas.width().max(1) as f32;
    let h_px = w.canvas.height().max(1) as f32;

    let started = {
        let mut mt = w.multi_touch.borrow_mut();
        if mt.gesture_kind == TouchGestureKind::None {
            let eng = w.engine.borrow();
            mt.gesture_kind = TouchGestureKind::PerformanceSurface;
            if let Some((dist, angle)) = mt.two_finger_metrics() {
                mt.initial_distance = dist;
                mt.initial_angle = angle;
            } else {
                mt.initial_distance = mt.spread_px().unwrap_or(1.0);
                mt.initial_angle = 0.0;
            }
            mt.initial_bpm = eng.params.bpm.get();
            mt.initial_detune = eng.params.detune_cents.get();
            mt.initial_centroid = mt.centroid_px().unwrap_or([w_px * 0.5, h_px * 0.5]);
            mt.current_centroid = Some(mt.initial_centroid);
            mt.motion_px = 0.0;
            mt.last_ripple_motion = 0.0;
            log::info!(
                "[gesture] multitouch surface begin fingers={} spread={:.0}px",
                pointer_count,
                mt.initial_distance
            );
            true
        } else {
            false
        }
    };

    if let Some(c_uv) = w.multi_touch.borrow().centroid_uv(w_px, h_px) {
        w.set_mouse_uv(c_uv, w_px, h_px);
        w.mouse_state.borrow_mut().down = true;
        let count_n = ((pointer_count.saturating_sub(1)) as f32 / 4.0).clamp(0.0, 1.0);
        w.bump_gesture(0.42 + 0.28 * count_n, 0.22 + 0.16 * count_n);
        if started {
            w.queue_ripple(c_uv, 1.12 + 0.22 * count_n);
        }
    }
}

// ─────────────────────────── pointerup ───────────────────────────

fn wire_pointerup(w: &InputWiring) {
    let w = w.clone();

    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::PointerEvent| {
        let pos = input::pointer_canvas_px(&ev, &w.canvas);
        let [uvx, uvy] = input::pointer_canvas_uv(&ev, &w.canvas);
        let pid = ev.pointer_id();

        let gesture_kind = w.multi_touch.borrow().gesture_kind;
        let was_multitouch = gesture_kind != TouchGestureKind::None;

        // Remove pointer and check if a multi-finger surface should end.
        let multitouch_release_uv = {
            let mut mt = w.multi_touch.borrow_mut();
            mt.pointers.remove(&pid);
            mt.current_centroid = mt.centroid_px();

            if was_multitouch && mt.pointers.is_empty() {
                // Snapshot centroid before reset (prefer stored centroid over last pointer position).
                let final_pos = mt.current_centroid.unwrap_or([pos.x, pos.y]);
                mt.reset_gesture();
                Some([
                    (final_pos[0] / w.canvas.width().max(1) as f32).clamp(0.0, 1.0),
                    (final_pos[1] / w.canvas.height().max(1) as f32).clamp(0.0, 1.0),
                ])
            } else if was_multitouch {
                let count = mt.pointers.len();
                if count >= 2 {
                    let eng = w.engine.borrow();
                    if let Some((dist, angle)) = mt.two_finger_metrics() {
                        mt.initial_distance = dist;
                        mt.initial_angle = angle;
                    } else {
                        mt.initial_distance = mt.spread_px().unwrap_or(1.0);
                        mt.initial_angle = 0.0;
                    }
                    mt.initial_bpm = eng.params.bpm.get();
                    mt.initial_detune = eng.params.detune_cents.get();
                    mt.initial_centroid = mt.centroid_px().unwrap_or(mt.initial_centroid);
                    mt.motion_px = 0.0;
                    mt.last_ripple_motion = 0.0;
                } else {
                    mt.reset_gesture();
                }
                None
            } else {
                None
            }
        };

        if let Some(uv) = multitouch_release_uv {
            w.queue_ripple(uv, 0.78);
            w.mouse_state.borrow_mut().down = false;
            log::info!("[gesture] multitouch surface end");
            ev.prevent_default();
            return;
        }

        if was_multitouch {
            ev.prevent_default();
            return;
        }

        // ── Single-pointer release (existing behavior) ──
        let (
            had_pointer_gesture,
            was_dragging,
            drag_started_on_release,
            start_x,
            start_y,
            travel_px,
            spin_accum,
            peak_motion,
        ) = {
            let mut ds = w.drag_state.borrow_mut();
            let had_gesture = ds.pending || ds.active;
            let mut dragging = ds.active;
            let mut started_on_release = false;
            if ds.pending && !dragging {
                let moved_x = pos.x - ds.start_x;
                let moved_y = pos.y - ds.start_y;
                if (moved_x * moved_x + moved_y * moved_y).sqrt() >= DRAG_START_THRESHOLD_PX {
                    dragging = true;
                    started_on_release = true;
                }
            }
            let sx = ds.start_x;
            let sy = ds.start_y;
            let travel = ds.travel_px;
            let spin = ds.spin_accum;
            let peak = ds.peak_motion;
            ds.active = false;
            ds.pending = false;
            (
                had_gesture,
                dragging,
                started_on_release,
                sx,
                sy,
                travel,
                spin,
                peak,
            )
        };

        // Also clean up multitouch state for the single pointer
        {
            let mut mt = w.multi_touch.borrow_mut();
            mt.pointers.remove(&pid);
            mt.reset_gesture();
        }

        if !had_pointer_gesture {
            w.mouse_state.borrow_mut().down = false;
            ev.prevent_default();
            return;
        }

        if drag_started_on_release {
            log::info!("[gesture] carve begin");
        }

        if was_dragging {
            let travel_n = (travel_px / 760.0).clamp(0.0, 1.0);
            let motion_n = (peak_motion / 34.0).clamp(0.0, 1.0);
            let delta_x = pos.x - start_x;
            let delta_y = pos.y - start_y;
            let drag_angle = delta_y.atan2(delta_x);
            let angle01 =
                ((drag_angle + std::f32::consts::PI) / std::f32::consts::TAU).clamp(0.0, 1.0);
            let root_idx = ((angle01 * (ROOT_TABLE.len() as f32 - 1.0)).round() as usize)
                .clamp(0, ROOT_TABLE.len() - 1);
            let root = ROOT_TABLE[root_idx];
            let mode_band =
                (((travel_px / 95.0).round() as usize) + ((spin_accum.abs() * 1.6) as usize)) % 7;
            let (mode, mode_name) = mode_for_vertical_band(mode_band);

            let (bpm, detune) = {
                let mut eng = w.engine.borrow_mut();
                eng.params.root_midi = root;
                eng.params.scale = mode;
                eng.set_bpm(Bpm::new(
                    (50.0 + 122.0 * travel_n + 22.0 * motion_n).clamp(38.0, 180.0),
                ));
                let detune = ((0.5 - uvy) * 220.0 + spin_accum.sin() * 160.0 + (uvx - 0.5) * 90.0)
                    .clamp(-200.0, 200.0);
                eng.set_detune_cents(Cents::new(detune));
                let voice_len = eng.voices.len();
                for i in 0..voice_len {
                    eng.reseed_voice(i, None);
                }
                (eng.params.bpm.get(), eng.params.detune_cents.get())
            };

            {
                let eng = w.engine.borrow();
                audio::emit_carve_chord(
                    &w.audio_ctx,
                    &eng,
                    &w.voice_gains,
                    &w.delay_sends,
                    &w.reverb_sends,
                    uvx,
                    uvy,
                    motion_n,
                    angle01,
                );
            }

            if let Some(window) = web::window() {
                if let Some(document) = window.document() {
                    overlay::update_hint(&document, detune, bpm, mode_name);
                    overlay::show_hint(&document);
                }
            }

            let amp = (1.05 + 0.70 * motion_n + 0.50 * travel_n).clamp(0.6, 2.1);
            w.queue_ripple([uvx, uvy], amp);
            perf::record_carve(uvx, uvy, motion_n, angle01);

            {
                let mut ms = w.mouse_state.borrow_mut();
                ms.gesture_energy = ms.gesture_energy.max(0.70 + 0.33 * travel_n);
                ms.gesture_flash = ms.gesture_flash.max(0.78 + 0.33 * motion_n);
                ms.gesture_spin = (ms.gesture_spin + spin_accum * 0.30).clamp(-4.0, 4.0);
            }

            log::info!(
                "[gesture] carve drop root={} mode={} travel={:.0}px spin={:.2}",
                root,
                mode_name,
                travel_px,
                spin_accum
            );
        } else {
            {
                let eng = w.engine.borrow();
                audio::emit_flare_chord(
                    &w.audio_ctx,
                    &eng,
                    &w.voice_gains,
                    &w.delay_sends,
                    &w.reverb_sends,
                    uvx,
                    uvy,
                );
            }

            w.queue_ripple([uvx, uvy], 1.45);
            perf::record_flare(uvx, uvy);
            {
                let mut ms = w.mouse_state.borrow_mut();
                ms.gesture_energy = ms.gesture_energy.max(0.46);
                ms.gesture_flash = ms.gesture_flash.max(0.54);
                ms.gesture_spin =
                    (ms.gesture_spin + ((uvx - 0.5) * (0.5 - uvy) * 1.2)).clamp(-4.0, 4.0);
            }
            log::info!("[gesture] flare uv=({:.2},{:.2})", uvx, uvy);
        }

        w.mouse_state.borrow_mut().down = false;
        ev.prevent_default();
    }) as Box<dyn FnMut(_)>);

    if let Some(wnd) = web::window() {
        _ = wnd.add_event_listener_with_callback("pointerup", closure.as_ref().unchecked_ref());
    }

    closure.forget();
}

// ─────────────────────────── pointercancel ───────────────────────────

fn wire_pointercancel(w: &InputWiring) {
    let w = w.clone();

    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::PointerEvent| {
        let pid = ev.pointer_id();

        {
            let mut mt = w.multi_touch.borrow_mut();
            mt.pointers.remove(&pid);
            mt.current_centroid = mt.centroid_px();
            if mt.pointers.len() < 2 {
                mt.reset_gesture();
            }
        }

        {
            let mut ds = w.drag_state.borrow_mut();
            ds.active = false;
            ds.pending = false;
        }

        w.mouse_state.borrow_mut().down = false;
    }) as Box<dyn FnMut(_)>);

    if let Some(wnd) = web::window() {
        _ = wnd.add_event_listener_with_callback("pointercancel", closure.as_ref().unchecked_ref());
    }

    closure.forget();
}

// ─────────────────────────── helpers ───────────────────────────

fn mode_for_vertical_band(band: usize) -> (&'static [f32], &'static str) {
    let idx = band.min(MODES_ORDER.len() - 1);
    (MODES_ORDER[idx], MODE_NAMES[idx])
}

#[inline]
fn lerp(current: f32, target: f32, alpha: f32) -> f32 {
    current + (target - current) * alpha.clamp(0.0, 1.0)
}
