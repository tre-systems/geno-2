use crate::audio;
use crate::core::{
    midi_to_hz, MusicEngine, AEOLIAN, C_MAJOR_PENTATONIC, DORIAN, IONIAN, LOCRIAN, LYDIAN,
    MIXOLYDIAN, PHRYGIAN, TET19_PENTATONIC, TET24_PENTATONIC, TET31_PENTATONIC,
};
use crate::input;
use crate::input::TouchGestureKind;
use crate::overlay;
use glam::Vec3;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys as web;

const DRAG_START_THRESHOLD_PX: f32 = 8.0;
const RIPPLE_INTERVAL_PX: f32 = 88.0;
const RESEED_INTERVAL_SEC: f64 = 0.22;
const ROOT_TABLE: [i32; 15] = [48, 50, 52, 53, 55, 57, 59, 60, 62, 64, 65, 67, 69, 71, 72];

/// Minimum BPM for pinch gesture.
const PINCH_BPM_MIN: f32 = 38.0;
/// Maximum BPM for pinch gesture.
const PINCH_BPM_MAX: f32 = 220.0;
/// Sensitivity for rotation→detune mapping (cents per radian).
const ROTATE_DETUNE_SENSITIVITY: f32 = 180.0;
/// Minimum centroid displacement (px) for a 3-finger swipe to register.
const THREE_FINGER_SWIPE_THRESHOLD_PX: f32 = 40.0;

/// Root notes in circle-of-fifths order for 3-finger swipe cycling.
const ROOTS_MUSICAL_ORDER: [i32; 7] = [60, 67, 62, 69, 64, 71, 65]; // C G D A E B F

/// Mode scales in order for 3-finger swipe cycling.
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
    pub hover_index: Rc<RefCell<Option<usize>>>,
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

        // Always update pointer map and running centroid for tracked pointers
        {
            let mut mt = w.multi_touch.borrow_mut();
            if mt.pointers.contains_key(&pid) {
                mt.pointers.insert(pid, [pos.x, pos.y]);
                mt.current_centroid = mt.centroid_px();
            }
        }

        let gesture_kind = w.multi_touch.borrow().gesture_kind;

        match gesture_kind {
            TouchGestureKind::TwoFingerPinchRotate => {
                handle_two_finger_move(&w, w_px, h_px);
                ev.prevent_default();
                return;
            }
            TouchGestureKind::ThreeFingerSwipe => {
                // Swirl tracks centroid during 3-finger gesture
                if let Some(c_uv) = w.multi_touch.borrow().centroid_uv(w_px, h_px) {
                    let mut ms = w.mouse_state.borrow_mut();
                    ms.x = c_uv[0] * w_px;
                    ms.y = c_uv[1] * h_px;
                    ms.gesture_energy = (ms.gesture_energy * 0.82 + 0.18 * 0.55).clamp(0.0, 1.8);
                }
                ev.prevent_default();
                return;
            }
            TouchGestureKind::FourFingerTap | TouchGestureKind::FiveFingerTap => {
                // Visual feedback only; action already committed on pointerdown
                if let Some(c_uv) = w.multi_touch.borrow().centroid_uv(w_px, h_px) {
                    let mut ms = w.mouse_state.borrow_mut();
                    ms.x = c_uv[0] * w_px;
                    ms.y = c_uv[1] * h_px;
                }
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

        *w.hover_index.borrow_mut() = None;

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
            *w.queued_ripple_uv.borrow_mut() = Some(input::RippleEvent {
                uv: [uvx, uvy],
                amp: 1.35,
            });
            let mut ms = w.mouse_state.borrow_mut();
            ms.gesture_energy = ms.gesture_energy.max(0.55);
            ms.gesture_flash = ms.gesture_flash.max(0.24);
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
                *w.queued_ripple_uv.borrow_mut() = Some(input::RippleEvent {
                    uv: [uvx, uvy],
                    amp: (1.05 + 1.35 * motion + 0.65 * travel_n).clamp(0.6, 2.4),
                });
            }
        }

        let mut eng = w.engine.borrow_mut();
        eng.set_bpm((50.0 + 122.0 * travel_n + 24.0 * motion).clamp(38.0, 180.0));
        let detune = ((0.5 - uvy) * 220.0 + spin_accum.sin() * 145.0 + (uvx - 0.5) * 90.0)
            .clamp(-280.0, 280.0);
        eng.set_detune_cents(detune);

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

/// Handle two-finger pinch→BPM and rotate→detune during pointermove.
fn handle_two_finger_move(w: &InputWiring, w_px: f32, h_px: f32) {
    let (ratio, angle_delta) = {
        let mt = w.multi_touch.borrow();
        if let Some((dist, angle)) = mt.two_finger_metrics() {
            let ratio = dist / mt.initial_distance.max(1.0);
            let mut da = angle - mt.initial_angle;
            while da > std::f32::consts::PI {
                da -= std::f32::consts::TAU;
            }
            while da < -std::f32::consts::PI {
                da += std::f32::consts::TAU;
            }
            (ratio, da)
        } else {
            return;
        }
    };

    let (initial_bpm, initial_detune) = {
        let mt = w.multi_touch.borrow();
        (mt.initial_bpm, mt.initial_detune)
    };

    let new_bpm = (initial_bpm * ratio).clamp(PINCH_BPM_MIN, PINCH_BPM_MAX);
    let new_detune =
        (initial_detune + angle_delta * ROTATE_DETUNE_SENSITIVITY).clamp(-280.0, 280.0);

    {
        let mut eng = w.engine.borrow_mut();
        eng.set_bpm(new_bpm);
        eng.set_detune_cents(new_detune);
    }

    {
        let mut ms = w.mouse_state.borrow_mut();
        let pinch_intensity = (ratio - 1.0).abs().clamp(0.0, 1.0);
        let rotate_intensity = angle_delta.abs().clamp(0.0, 1.0);
        let energy_target =
            (0.40 + 0.55 * pinch_intensity + 0.45 * rotate_intensity).clamp(0.0, 1.8);
        ms.gesture_energy = (ms.gesture_energy * 0.78 + energy_target * 0.22).clamp(0.0, 1.8);
        ms.gesture_flash = (ms.gesture_flash + 0.04 + 0.08 * rotate_intensity).clamp(0.0, 1.6);
        ms.gesture_spin = (ms.gesture_spin + angle_delta * 0.02).clamp(-4.0, 4.0);
    }

    if let Some(mid_uv) = w.multi_touch.borrow().midpoint_uv(w_px, h_px) {
        let mut ms = w.mouse_state.borrow_mut();
        ms.x = mid_uv[0] * w_px;
        ms.y = mid_uv[1] * h_px;
    }

    if let Some(window) = web::window() {
        if let Some(document) = window.document() {
            overlay::update_hint(&document, new_detune, new_bpm, "");
            overlay::show_hint(&document);
        }
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

            start_or_upgrade_multitouch(&w, pointer_count);
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
            *w.queued_ripple_uv.borrow_mut() = Some(input::RippleEvent {
                uv: [uvx, uvy],
                amp: 1.15,
            });
            *w.hover_index.borrow_mut() = None;
        }

        _ = w.canvas.set_pointer_capture(pid);
        ev.prevent_default();
    }) as Box<dyn FnMut(_)>);

    _ = canvas_for_listener
        .add_event_listener_with_callback("pointerdown", closure.as_ref().unchecked_ref());
    closure.forget();
}

/// Start a new multitouch gesture or upgrade an existing one when more fingers arrive.
fn start_or_upgrade_multitouch(w: &InputWiring, pointer_count: usize) {
    let w_px = w.canvas.width().max(1) as f32;
    let h_px = w.canvas.height().max(1) as f32;

    let mut mt = w.multi_touch.borrow_mut();

    match pointer_count {
        2 if mt.gesture_kind == TouchGestureKind::None => {
            // Start two-finger pinch/rotate
            if let Some((dist, angle)) = mt.two_finger_metrics() {
                let eng = w.engine.borrow();
                mt.gesture_kind = TouchGestureKind::TwoFingerPinchRotate;
                mt.initial_distance = dist;
                mt.initial_angle = angle;
                mt.initial_bpm = eng.params.bpm;
                mt.initial_detune = eng.params.detune_cents;
                log::info!(
                    "[gesture] multitouch-2 begin dist={:.0}px angle={:.2}rad",
                    dist,
                    angle
                );
            }
        }
        3 => {
            // Upgrade to three-finger swipe
            mt.gesture_kind = TouchGestureKind::ThreeFingerSwipe;
            if let Some(centroid) = mt.centroid_px() {
                mt.initial_centroid = centroid;
            }
            log::info!("[gesture] multitouch-3 begin (swipe)");
        }
        4 if !mt.gesture_committed => {
            // Four-finger tap: randomize root + mode + reseed
            mt.gesture_kind = TouchGestureKind::FourFingerTap;
            mt.gesture_committed = true;

            let ri = (js_sys::Math::random() * ROOTS_MUSICAL_ORDER.len() as f64).floor() as usize;
            let mi = (js_sys::Math::random() * MODES_ORDER.len() as f64).floor() as usize;

            {
                let mut eng = w.engine.borrow_mut();
                eng.params.root_midi = ROOTS_MUSICAL_ORDER[ri];
                eng.params.scale = MODES_ORDER[mi];
                let voice_len = eng.voices.len();
                for i in 0..voice_len {
                    eng.reseed_voice(i, None);
                }
            }
            log::info!(
                "[gesture] multitouch-4 randomize root={} mode={}",
                roots[ri],
                MODE_NAMES[mi]
            );

            // Update hint
            drop(mt);
            update_hint_from_engine(w);
            let mt = w.multi_touch.borrow();

            // Strong visual burst
            {
                let mut ms = w.mouse_state.borrow_mut();
                ms.gesture_energy = ms.gesture_energy.max(1.2);
                ms.gesture_flash = ms.gesture_flash.max(1.0);
            }
            if let Some(c_uv) = mt.centroid_uv(w_px, h_px) {
                *w.queued_ripple_uv.borrow_mut() = Some(input::RippleEvent { uv: c_uv, amp: 2.2 });
            }
            return;
        }
        n if n >= 5 && !mt.gesture_committed => {
            // Five-finger tap: toggle pause/resume
            mt.gesture_kind = TouchGestureKind::FiveFingerTap;
            mt.gesture_committed = true;

            {
                let mut p = w.paused.borrow_mut();
                *p = !*p;
                log::info!("[gesture] multitouch-5 paused={}", *p);
            }

            // Visual burst
            {
                let mut ms = w.mouse_state.borrow_mut();
                ms.gesture_energy = ms.gesture_energy.max(0.85);
                ms.gesture_flash = ms.gesture_flash.max(0.75);
            }
            if let Some(c_uv) = mt.centroid_uv(w_px, h_px) {
                *w.queued_ripple_uv.borrow_mut() = Some(input::RippleEvent { uv: c_uv, amp: 1.8 });
            }
            return;
        }
        _ => {}
    }

    // Visual feedback for any multitouch start
    {
        let mut ms = w.mouse_state.borrow_mut();
        ms.gesture_energy = ms.gesture_energy.max(0.55);
        ms.gesture_flash = ms.gesture_flash.max(0.30);
    }
    if let Some(c_uv) = mt.centroid_uv(w_px, h_px) {
        *w.queued_ripple_uv.borrow_mut() = Some(input::RippleEvent {
            uv: c_uv,
            amp: 1.45,
        });
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

        // Remove pointer and check if gesture should end
        {
            let mut mt = w.multi_touch.borrow_mut();
            mt.pointers.remove(&pid);

            if was_multitouch && mt.pointers.is_empty() {
                // All fingers lifted — commit deferred gestures and reset
                let kind = mt.gesture_kind;
                let initial_centroid = mt.initial_centroid;

                // Snapshot centroid before reset (prefer stored centroid over last pointer position)
                let final_pos = mt.current_centroid.unwrap_or([pos.x, pos.y]);

                mt.reset_gesture();
                drop(mt);

                if kind == TouchGestureKind::ThreeFingerSwipe {
                    handle_three_finger_release(w.clone(), initial_centroid, final_pos);
                } else if kind == TouchGestureKind::TwoFingerPinchRotate {
                    // Reseed voices on pinch/rotate release
                    let mut eng = w.engine.borrow_mut();
                    let voice_len = eng.voices.len();
                    for i in 0..voice_len {
                        eng.reseed_voice(i, None);
                    }
                    log::info!("[gesture] multitouch-2 end");
                }

                w.mouse_state.borrow_mut().down = false;
                ev.prevent_default();
                return;
            }
        }

        if was_multitouch {
            // If a multi-touch gesture has lost enough fingers, reset so the
            // remaining pointer(s) don't keep being treated as part of that gesture.
            {
                let mut mt = w.multi_touch.borrow_mut();
                if mt.gesture_kind == TouchGestureKind::TwoFingerPinchRotate
                    && mt.pointers.len() < 2
                {
                    mt.reset_gesture();
                }
            }
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
                eng.set_bpm((50.0 + 122.0 * travel_n + 22.0 * motion_n).clamp(38.0, 180.0));
                let detune = ((0.5 - uvy) * 220.0 + spin_accum.sin() * 160.0 + (uvx - 0.5) * 90.0)
                    .clamp(-280.0, 280.0);
                eng.set_detune_cents(detune);
                let voice_len = eng.voices.len();
                for i in 0..voice_len {
                    eng.reseed_voice(i, None);
                }
                (eng.params.bpm, eng.params.detune_cents)
            };

            let base_midi = 43.0 + angle01 * 30.0 + (0.5 - uvy) * 5.0;
            let accents: [f32; 5] = [0.0, 4.0, 9.0, 14.0, 19.0];
            let voice_len = w.voice_gains.len().max(1);
            let eng = w.engine.borrow();
            for (i, interval) in accents.iter().enumerate() {
                let vi = i % voice_len;
                let wf = eng.configs[vi].waveform;
                let freq = midi_to_hz(base_midi + *interval);
                let vel = (0.34 + 0.22 * motion_n + i as f32 * 0.03).clamp(0.0, 1.0);
                let dur = 0.24 + 0.11 * (i % 3) as f64 + 0.16 * (1.0 - uvy as f64);
                audio::trigger_one_shot(
                    &w.audio_ctx,
                    wf,
                    freq,
                    vel,
                    dur,
                    &w.voice_gains[vi],
                    &w.delay_sends[vi],
                    &w.reverb_sends[vi],
                );
            }

            if let Some(window) = web::window() {
                if let Some(document) = window.document() {
                    overlay::update_hint(&document, detune, bpm, mode_name);
                    overlay::show_hint(&document);
                }
            }

            *w.queued_ripple_uv.borrow_mut() = Some(input::RippleEvent {
                uv: [uvx, uvy],
                amp: (1.40 + 0.95 * motion_n + 0.70 * travel_n).clamp(0.8, 2.8),
            });

            {
                let mut ms = w.mouse_state.borrow_mut();
                ms.gesture_energy = ms.gesture_energy.max(0.95 + 0.45 * travel_n);
                ms.gesture_flash = ms.gesture_flash.max(1.05 + 0.45 * motion_n);
                ms.gesture_spin = (ms.gesture_spin + spin_accum * 0.42).clamp(-4.0, 4.0);
            }

            log::info!(
                "[gesture] carve drop root={} mode={} travel={:.0}px spin={:.2}",
                root,
                mode_name,
                travel_px,
                spin_accum
            );
        } else {
            let base_midi = 42.0 + uvx * 34.0 + (0.5 - uvy) * 8.0;
            let flare_steps: [f32; 5] = [0.0, 7.0, 12.0, 19.0, 24.0];
            let duration_base = 0.20 + 0.24 * (1.0 - uvy as f64);

            let eng = w.engine.borrow();
            let voice_len = eng.voices.len();
            for i in 0..flare_steps.len() {
                let vi = i % voice_len;
                let wf = eng.configs[vi].waveform;
                let freq = midi_to_hz(base_midi + flare_steps[i]);
                let vel = (0.30 + 0.24 * (1.0 - uvy) + i as f32 * 0.04).clamp(0.0, 1.0);
                let dur = duration_base + 0.12 * (i % 3) as f64;
                audio::trigger_one_shot(
                    &w.audio_ctx,
                    wf,
                    freq,
                    vel,
                    dur,
                    &w.voice_gains[vi],
                    &w.delay_sends[vi],
                    &w.reverb_sends[vi],
                );
            }

            *w.queued_ripple_uv.borrow_mut() = Some(input::RippleEvent {
                uv: [uvx, uvy],
                amp: 1.95,
            });
            {
                let mut ms = w.mouse_state.borrow_mut();
                ms.gesture_energy = ms.gesture_energy.max(0.62);
                ms.gesture_flash = ms.gesture_flash.max(0.72);
                ms.gesture_spin =
                    (ms.gesture_spin + ((uvx - 0.5) * (0.5 - uvy) * 1.8)).clamp(-4.0, 4.0);
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

/// Handle 3-finger swipe release: horizontal → cycle root, vertical → cycle mode.
fn handle_three_finger_release(w: InputWiring, initial_centroid: [f32; 2], final_pos: [f32; 2]) {
    let dx = final_pos[0] - initial_centroid[0];
    let dy = final_pos[1] - initial_centroid[1];
    let dist = (dx * dx + dy * dy).sqrt();

    if dist < THREE_FINGER_SWIPE_THRESHOLD_PX {
        log::info!("[gesture] multitouch-3 end (no swipe, below threshold)");
        return;
    }

    let horizontal = dx.abs() > dy.abs();

    if horizontal {
        // Swipe left/right → cycle root note
        let direction: i32 = if dx > 0.0 { 1 } else { -1 };
        let mut eng = w.engine.borrow_mut();
        let current_root = eng.params.root_midi;
        let current_idx = ROOTS_MUSICAL_ORDER
            .iter()
            .position(|&r| r == current_root)
            .unwrap_or(0);
        let new_idx =
            (current_idx as i32 + direction).rem_euclid(ROOTS_MUSICAL_ORDER.len() as i32) as usize;
        eng.params.root_midi = ROOTS_MUSICAL_ORDER[new_idx];
        let voice_len = eng.voices.len();
        for i in 0..voice_len {
            eng.reseed_voice(i, None);
        }
        drop(eng);
        log::info!(
            "[gesture] multitouch-3 swipe {} root={}",
            if direction > 0 { "right" } else { "left" },
            ROOTS_MUSICAL_ORDER[new_idx]
        );
    } else {
        // Swipe up/down → cycle mode
        // Down = next mode (positive index), Up = previous mode
        let direction: i32 = if dy > 0.0 { 1 } else { -1 };
        let mut eng = w.engine.borrow_mut();
        let current_scale = eng.params.scale;
        let current_idx = MODES_ORDER
            .iter()
            .position(|&m| m == current_scale)
            .unwrap_or(0);
        let new_idx =
            (current_idx as i32 + direction).rem_euclid(MODES_ORDER.len() as i32) as usize;
        eng.params.scale = MODES_ORDER[new_idx];
        let voice_len = eng.voices.len();
        for i in 0..voice_len {
            eng.reseed_voice(i, None);
        }
        drop(eng);
        log::info!(
            "[gesture] multitouch-3 swipe {} mode={}",
            if direction > 0 { "down" } else { "up" },
            MODE_NAMES[new_idx]
        );
    }

    update_hint_from_engine(&w);

    // Visual burst
    {
        let mut ms = w.mouse_state.borrow_mut();
        ms.gesture_energy = ms.gesture_energy.max(0.85);
        ms.gesture_flash = ms.gesture_flash.max(0.65);
    }
}

// ─────────────────────────── pointercancel ───────────────────────────

fn wire_pointercancel(w: &InputWiring) {
    let w = w.clone();

    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::PointerEvent| {
        let pid = ev.pointer_id();

        {
            let mut mt = w.multi_touch.borrow_mut();
            mt.pointers.remove(&pid);
            if mt.pointers.is_empty() {
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

/// Update the hint overlay from current engine state.
fn update_hint_from_engine(w: &InputWiring) {
    if let Some(window) = web::window() {
        if let Some(document) = window.document() {
            let eng = w.engine.borrow();
            let scale_name = scale_name(eng.params.scale);
            overlay::update_hint(
                &document,
                eng.params.detune_cents,
                eng.params.bpm,
                scale_name,
            );
            overlay::show_hint(&document);
        }
    }
}

fn mode_for_vertical_band(band: usize) -> (&'static [f32], &'static str) {
    match band {
        0 => (IONIAN, "Ionian (major)"),
        1 => (DORIAN, "Dorian"),
        2 => (PHRYGIAN, "Phrygian"),
        3 => (LYDIAN, "Lydian"),
        4 => (MIXOLYDIAN, "Mixolydian"),
        5 => (AEOLIAN, "Aeolian (minor)"),
        _ => (LOCRIAN, "Locrian"),
    }
}

fn scale_name(scale: &[f32]) -> &'static str {
    match scale {
        s if s == IONIAN => "Ionian (major)",
        s if s == DORIAN => "Dorian",
        s if s == PHRYGIAN => "Phrygian",
        s if s == LYDIAN => "Lydian",
        s if s == MIXOLYDIAN => "Mixolydian",
        s if s == AEOLIAN => "Aeolian (minor)",
        s if s == LOCRIAN => "Locrian",
        s if s == C_MAJOR_PENTATONIC => "C Major Pentatonic",
        s if s == TET19_PENTATONIC => "19-TET pentatonic",
        s if s == TET24_PENTATONIC => "24-TET pentatonic",
        s if s == TET31_PENTATONIC => "31-TET pentatonic",
        _ => "Custom",
    }
}
