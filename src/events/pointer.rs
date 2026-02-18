use crate::audio;
use crate::core::{
    midi_to_hz, MusicEngine, AEOLIAN, C_MAJOR_PENTATONIC, DORIAN, IONIAN, LOCRIAN, LYDIAN,
    MIXOLYDIAN, PHRYGIAN, TET19_PENTATONIC, TET24_PENTATONIC, TET31_PENTATONIC,
};
use crate::input;
use crate::overlay;
use glam::Vec3;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys as web;

const DRAG_START_THRESHOLD_PX: f32 = 8.0;
const ROOT_TABLE: [i32; 15] = [48, 50, 52, 53, 55, 57, 59, 60, 62, 64, 65, 67, 69, 71, 72];

#[derive(Clone)]
pub struct InputWiring {
    pub canvas: web::HtmlCanvasElement,
    pub engine: Rc<RefCell<MusicEngine>>,
    pub mouse_state: Rc<RefCell<input::MouseState>>,
    pub hover_index: Rc<RefCell<Option<usize>>>,
    pub drag_state: Rc<RefCell<input::DragState>>,
    pub voice_gains: Rc<Vec<web::GainNode>>,
    pub delay_sends: Rc<Vec<web::GainNode>>,
    pub reverb_sends: Rc<Vec<web::GainNode>>,
    pub audio_ctx: web::AudioContext,
    pub queued_ripple_uv: Rc<RefCell<Option<[f32; 2]>>>,
}

pub fn wire_input_handlers(w: InputWiring) {
    wire_pointermove(&w);
    wire_pointerdown(&w);
    wire_pointerup(&w);
}

fn wire_pointermove(w: &InputWiring) {
    let w = w.clone();
    let canvas_connected = w.canvas.is_connected();

    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::PointerEvent| {
        let pos = input::pointer_canvas_px(&ev, &w.canvas);

        if !canvas_connected {
            return;
        }

        {
            let mut ms = w.mouse_state.borrow_mut();
            ms.x = pos.x;
            ms.y = pos.y;
        }

        *w.hover_index.borrow_mut() = None;

        let (drag_active, drag_just_started, start_x, start_y, delta_x, delta_y) = {
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

            ds.last_x = pos.x;
            ds.last_y = pos.y;

            (
                ds.active,
                !was_active && ds.active,
                ds.start_x,
                ds.start_y,
                dx,
                dy,
            )
        };

        if drag_just_started {
            log::info!("[gesture] begin sweep");
        }

        if !drag_active {
            return;
        }

        let w_px = w.canvas.width().max(1) as f32;
        let h_px = w.canvas.height().max(1) as f32;
        let uvx = (pos.x / w_px).clamp(0.0, 1.0);
        let uvy = (pos.y / h_px).clamp(0.0, 1.0);
        let motion = ((delta_x * delta_x + delta_y * delta_y).sqrt() / 28.0).clamp(0.0, 1.0);

        let radius = (0.24 + (1.0 - uvy) * 0.66).clamp(0.18, 1.0);
        let base_phase = uvx * std::f32::consts::TAU
            + (pos.x - start_x) / w_px * std::f32::consts::TAU
            - (pos.y - start_y) / h_px * (std::f32::consts::TAU * 0.7);

        let mut eng = w.engine.borrow_mut();
        eng.set_bpm((72.0 + uvx * 132.0 + motion * 10.0).clamp(40.0, 240.0));
        eng.set_detune_cents(((0.5 - uvy) * 180.0).clamp(-180.0, 180.0));

        let voice_len = eng.voices.len().max(1);
        for i in 0..eng.voices.len() {
            let lane = i as f32 / voice_len as f32;
            let phase = base_phase
                + i as f32 * (std::f32::consts::TAU / voice_len as f32)
                + motion * (0.45 + lane * 0.65);
            let x = radius * phase.cos();
            let z = radius * phase.sin();
            eng.set_voice_position(i, Vec3::new(x, 0.0, z));

            let base_prob = match i {
                0 => 0.56,
                1 => 0.70,
                _ => 0.46,
            };
            let mod_prob = 0.20 * motion + 0.12 * (0.5 + 0.5 * (phase * 1.7).sin());
            eng.configs[i].trigger_probability = (base_prob + mod_prob).clamp(0.10, 0.95);
        }
    }) as Box<dyn FnMut(_)>);

    if let Some(wnd) = web::window() {
        _ = wnd.add_event_listener_with_callback("pointermove", closure.as_ref().unchecked_ref());
    }

    closure.forget();
}

fn wire_pointerdown(w: &InputWiring) {
    let w = w.clone();
    let canvas_for_listener = w.canvas.clone();

    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::PointerEvent| {
        let pos = input::pointer_canvas_px(&ev, &w.canvas);

        {
            let mut ds = w.drag_state.borrow_mut();
            ds.pending = true;
            ds.active = false;
            ds.start_x = pos.x;
            ds.start_y = pos.y;
            ds.last_x = pos.x;
            ds.last_y = pos.y;
        }

        {
            let mut ms = w.mouse_state.borrow_mut();
            ms.down = true;
            ms.x = pos.x;
            ms.y = pos.y;
        }

        let [uvx, uvy] = input::pointer_canvas_uv(&ev, &w.canvas);
        *w.queued_ripple_uv.borrow_mut() = Some([uvx, uvy]);
        *w.hover_index.borrow_mut() = None;

        _ = w.canvas.set_pointer_capture(ev.pointer_id());
        ev.prevent_default();
    }) as Box<dyn FnMut(_)>);

    _ = canvas_for_listener
        .add_event_listener_with_callback("pointerdown", closure.as_ref().unchecked_ref());
    closure.forget();
}

fn wire_pointerup(w: &InputWiring) {
    let w = w.clone();

    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::PointerEvent| {
        let pos = input::pointer_canvas_px(&ev, &w.canvas);
        let [uvx, uvy] = input::pointer_canvas_uv(&ev, &w.canvas);

        let (had_pointer_gesture, was_dragging, drag_started_on_release, start_x, start_y) = {
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
            ds.active = false;
            ds.pending = false;
            (had_gesture, dragging, started_on_release, sx, sy)
        };

        if !had_pointer_gesture {
            return;
        }

        if drag_started_on_release {
            log::info!("[gesture] begin sweep");

            let delta_x = pos.x - start_x;
            let delta_y = pos.y - start_y;
            let w_px = w.canvas.width().max(1) as f32;
            let h_px = w.canvas.height().max(1) as f32;
            let uvx = (pos.x / w_px).clamp(0.0, 1.0);
            let uvy = (pos.y / h_px).clamp(0.0, 1.0);
            let motion = ((delta_x * delta_x + delta_y * delta_y).sqrt() / 28.0).clamp(0.0, 1.0);

            let radius = (0.24 + (1.0 - uvy) * 0.66).clamp(0.18, 1.0);
            let base_phase = uvx * std::f32::consts::TAU + delta_x / w_px * std::f32::consts::TAU
                - delta_y / h_px * (std::f32::consts::TAU * 0.7);

            let mut eng = w.engine.borrow_mut();
            eng.set_bpm((72.0 + uvx * 132.0 + motion * 10.0).clamp(40.0, 240.0));
            eng.set_detune_cents(((0.5 - uvy) * 180.0).clamp(-180.0, 180.0));

            let voice_len = eng.voices.len().max(1);
            for i in 0..eng.voices.len() {
                let lane = i as f32 / voice_len as f32;
                let phase = base_phase
                    + i as f32 * (std::f32::consts::TAU / voice_len as f32)
                    + motion * (0.45 + lane * 0.65);
                let x = radius * phase.cos();
                let z = radius * phase.sin();
                eng.set_voice_position(i, Vec3::new(x, 0.0, z));

                let base_prob = match i {
                    0 => 0.56,
                    1 => 0.70,
                    _ => 0.46,
                };
                let mod_prob = 0.20 * motion + 0.12 * (0.5 + 0.5 * (phase * 1.7).sin());
                eng.configs[i].trigger_probability = (base_prob + mod_prob).clamp(0.10, 0.95);
            }
        }

        if was_dragging {
            let root_idx = ((uvx * (ROOT_TABLE.len() as f32 - 1.0)).round() as usize)
                .clamp(0, ROOT_TABLE.len() - 1);
            let root = ROOT_TABLE[root_idx];
            let band = ((uvy * 7.0).floor() as usize).clamp(0, 6);
            let (mode, mode_name) = mode_for_vertical_band(band);

            let (bpm, detune) = {
                let mut eng = w.engine.borrow_mut();
                eng.params.root_midi = root;
                eng.params.scale = mode;
                let voice_len = eng.voices.len();
                for i in 0..voice_len {
                    eng.reseed_voice(i, None);
                }
                (eng.params.bpm, eng.params.detune_cents)
            };

            if let Some(window) = web::window() {
                if let Some(document) = window.document() {
                    overlay::update_hint(&document, detune, bpm, mode_name);
                    overlay::show_hint(&document);
                }
            }

            log::info!(
                "[gesture] sweep apply root={} mode={} bpm={:.1} detune={:.0}",
                root,
                mode_name,
                bpm,
                detune
            );
        } else {
            let base_midi = 45.0 + uvx * 36.0 + (0.5 - uvy) * 6.0;
            let intervals: [f32; 3] = [0.0, 5.0, 10.0];
            let duration_base = 0.22 + 0.22 * (1.0 - uvy as f64);

            let eng = w.engine.borrow();
            let voice_len = eng.voices.len();
            for i in 0..voice_len {
                let wf = eng.configs[i].waveform;
                let freq = midi_to_hz(base_midi + intervals[i % intervals.len()]);
                let vel = (0.34 + 0.42 * (1.0 - uvy) + i as f32 * 0.06).clamp(0.0, 1.0);
                let dur = duration_base * (0.78 + i as f64 * 0.22);
                audio::trigger_one_shot(
                    &w.audio_ctx,
                    wf,
                    freq,
                    vel,
                    dur,
                    &w.voice_gains[i],
                    &w.delay_sends[i],
                    &w.reverb_sends[i],
                );
            }

            *w.queued_ripple_uv.borrow_mut() = Some([uvx, uvy]);
            log::info!("[gesture] burst uv=({:.2},{:.2})", uvx, uvy);
        }

        w.mouse_state.borrow_mut().down = false;
        ev.prevent_default();
    }) as Box<dyn FnMut(_)>);

    if let Some(wnd) = web::window() {
        _ = wnd.add_event_listener_with_callback("pointerup", closure.as_ref().unchecked_ref());
    }

    closure.forget();
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

#[allow(dead_code)]
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
