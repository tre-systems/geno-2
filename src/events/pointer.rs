use crate::audio;
use crate::constants::{CAMERA_Z, ENGINE_DRAG_MAX_RADIUS, PICK_SPHERE_RADIUS, SPREAD, Z_OFFSET};
use crate::core::{midi_to_hz, MusicEngine};
use crate::input;
use crate::render;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys as web;

const DRAG_START_THRESHOLD_PX: f32 = 6.0;

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

        let (ro, rd) = render::screen_to_world_ray(&w.canvas, pos.x, pos.y, CAMERA_Z);
        let (drag_active, drag_voice, drag_plane_z, drag_just_started) = {
            let mut ds = w.drag_state.borrow_mut();
            let was_active = ds.active;
            if ds.pending && !ds.active {
                let dx = pos.x - ds.start_x;
                let dy = pos.y - ds.start_y;
                let moved = (dx * dx + dy * dy).sqrt();
                if moved >= DRAG_START_THRESHOLD_PX {
                    ds.active = true;
                }
            }
            (
                ds.active,
                ds.voice,
                ds.plane_z_world,
                !was_active && ds.active,
            )
        };

        if drag_just_started {
            log::info!("[mouse] begin drag on voice {}", drag_voice);
        }

        if drag_active {
            if rd.z.abs() > 1e-6 {
                let t = (drag_plane_z - ro.z) / rd.z;

                if t >= 0.0 {
                    let hit_world = ro + rd * t;
                    let mut eng_pos = (hit_world - Z_OFFSET) / SPREAD;
                    let max_r = ENGINE_DRAG_MAX_RADIUS;
                    let len = (eng_pos.x * eng_pos.x + eng_pos.z * eng_pos.z).sqrt();

                    if len > max_r {
                        let scale = max_r / len;
                        eng_pos.x *= scale;
                        eng_pos.z *= scale;
                    }

                    let mut eng = w.engine.borrow_mut();
                    eng.set_voice_position(drag_voice, glam::Vec3::new(eng_pos.x, 0.0, eng_pos.z));
                }
            }
        } else {
            let mut best = None::<(usize, f32)>;
            let z_offset = Z_OFFSET;
            let engine_snapshot = w.engine.borrow();
            for (i, v) in engine_snapshot.voices.iter().enumerate() {
                let center_world = v.position * SPREAD + z_offset;

                if let Some(t) = input::ray_sphere(ro, rd, center_world, PICK_SPHERE_RADIUS) {
                    if t >= 0.0 {
                        match best {
                            Some((_, bt)) if t >= bt => {}
                            _ => best = Some((i, t)),
                        }
                    }
                }
            }

            match best {
                Some((i, _t)) => {
                    *w.hover_index.borrow_mut() = Some(i);
                }
                None => {
                    *w.hover_index.borrow_mut() = None;
                }
            }
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
        if let Some(i) = *w.hover_index.borrow() {
            let plane_z_world = {
                let eng = w.engine.borrow();
                eng.voices[i].position.z * SPREAD.z + Z_OFFSET.z
            };
            let mut ds = w.drag_state.borrow_mut();
            ds.pending = true;
            ds.active = false;
            ds.voice = i;
            ds.plane_z_world = plane_z_world;
            ds.start_x = pos.x;
            ds.start_y = pos.y;
        } else {
            let mut ds = w.drag_state.borrow_mut();
            ds.pending = false;
            ds.active = false;
        }
        {
            let mut ms = w.mouse_state.borrow_mut();
            ms.down = true;
            ms.x = pos.x;
            ms.y = pos.y;
        }
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
        let (was_dragging, pressed_voice) = {
            let mut ds = w.drag_state.borrow_mut();
            let dragging = ds.active;
            let pressed = if ds.pending { Some(ds.voice) } else { None };
            ds.active = false;
            ds.pending = false;
            (dragging, pressed)
        };

        if !was_dragging {
            let hover = *w.hover_index.borrow();
            let target_voice = pressed_voice.or(hover);
            if let Some(i) = target_voice {
                let shift = ev.shift_key();
                let alt = ev.alt_key();
                if alt {
                    w.engine.borrow_mut().toggle_solo(i);
                    log::info!("[click] solo voice {}", i);
                } else if shift {
                    w.engine.borrow_mut().reseed_voice(i, None);
                    log::info!("[click] reseed voice {}", i);
                } else {
                    w.engine.borrow_mut().toggle_mute(i);
                    log::info!("[click] toggle mute voice {}", i);
                }
            } else {
                let [uvx, uvy] = input::pointer_canvas_uv(&ev, &w.canvas);
                if uvx.is_finite() && uvy.is_finite() {
                    let midi = 60.0 + uvx * 24.0;
                    let freq = midi_to_hz(midi as f32);
                    let vel = (0.35 + 0.65 * uvy) as f32;
                    let eng = w.engine.borrow();
                    let norm_xs: Vec<f32> = eng
                        .voices
                        .iter()
                        .map(|v| (v.position.x / 3.0).clamp(-1.0, 1.0) * 0.5 + 0.5)
                        .collect();
                    let best_i = crate::input::nearest_index_by_uvx(&norm_xs, uvx);
                    let dur = 0.35 + 0.25 * (1.0 - uvy as f64);
                    let wf = eng.configs[best_i].waveform;
                    drop(eng);
                    audio::trigger_one_shot(
                        &w.audio_ctx,
                        wf,
                        freq,
                        vel,
                        dur,
                        &w.voice_gains[best_i],
                        &w.delay_sends[best_i],
                        &w.reverb_sends[best_i],
                    );
                    *w.queued_ripple_uv.borrow_mut() = Some([uvx, uvy]);
                }
            }
        }
        w.mouse_state.borrow_mut().down = false;
        ev.prevent_default();
    }) as Box<dyn FnMut(_)>);

    if let Some(wnd) = web::window() {
        _ = wnd.add_event_listener_with_callback("pointerup", closure.as_ref().unchecked_ref());
    }

    closure.forget();
}
