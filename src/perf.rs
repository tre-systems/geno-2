//! Networked-performance gesture bridge.
//!
//! In **broadcast** mode the main app records the performer's transient gesture
//! output — flares, carve arpeggios, drag ripples, and the pointer swirl — and
//! the page streams it over the relay's `{t:"ev"}` channel. In **display** mode
//! the receiving page calls the `perf_*` appliers to reproduce each event on its
//! own engine + renderer, so every screen looks and sounds like the performer's.
//!
//! Multi-finger gestures resolve to engine *parameters*, which already ride the
//! `{t:"set"}` channel; this module carries only the visual/audible transient
//! layer. Everything is a safe no-op until `install` runs, and recording is
//! gated on `perf_enable_broadcast`, so a plain (non-broadcasting) app pays
//! nothing but a flag check.

use crate::audio;
use crate::core::MusicEngine;
use crate::input::{MouseState, RippleEvent};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys as web;

/// Live handles the perf surface drives — the same subsystems `wasm_app` built.
struct Perf {
    canvas: web::HtmlCanvasElement,
    engine: Rc<RefCell<MusicEngine>>,
    mouse_state: Rc<RefCell<MouseState>>,
    voice_gains: Rc<Vec<web::GainNode>>,
    delay_sends: Rc<Vec<web::GainNode>>,
    reverb_sends: Rc<Vec<web::GainNode>>,
    audio_ctx: web::AudioContext,
    queued_ripple_uv: Rc<RefCell<Option<RippleEvent>>>,
}

/// A transient gesture event captured on the performer, awaiting broadcast.
enum EvOut {
    Flare { u: f32, v: f32 },
    Carve { u: f32, v: f32, m: f32, g: f32 },
    Ripple { u: f32, v: f32, p: f32 },
}

thread_local! {
    static PERF: RefCell<Option<Perf>> = const { RefCell::new(None) };
    static BROADCASTING: Cell<bool> = const { Cell::new(false) };
    static OUTBOX: RefCell<Vec<EvOut>> = const { RefCell::new(Vec::new()) };
}

/// Cap buffered events between JS drains so a stalled page can't grow it without
/// bound (a fast drag emits a ripple every ~88 px of travel).
const OUTBOX_CAP: usize = 96;

/// Stash the live handles. Called once from `wasm_app` after the engine, audio
/// graph, and canvas exist (the same clones handed to `wire_input_handlers`).
#[allow(clippy::too_many_arguments)]
pub fn install(
    canvas: web::HtmlCanvasElement,
    engine: Rc<RefCell<MusicEngine>>,
    mouse_state: Rc<RefCell<MouseState>>,
    voice_gains: Rc<Vec<web::GainNode>>,
    delay_sends: Rc<Vec<web::GainNode>>,
    reverb_sends: Rc<Vec<web::GainNode>>,
    audio_ctx: web::AudioContext,
    queued_ripple_uv: Rc<RefCell<Option<RippleEvent>>>,
) {
    PERF.with(|p| {
        *p.borrow_mut() = Some(Perf {
            canvas,
            engine,
            mouse_state,
            voice_gains,
            delay_sends,
            reverb_sends,
            audio_ctx,
            queued_ripple_uv,
        });
    });
}

fn with_perf<F: FnOnce(&Perf)>(f: F) {
    PERF.with(|p| {
        if let Some(perf) = p.borrow().as_ref() {
            f(perf);
        }
    });
}

// ── Performer side: record gesture output (no-op unless broadcasting) ──

fn push(ev: EvOut) {
    if !BROADCASTING.with(Cell::get) {
        return;
    }
    OUTBOX.with(|o| {
        let mut o = o.borrow_mut();
        if o.len() < OUTBOX_CAP {
            o.push(ev);
        }
    });
}

/// Record a tap flare for broadcast. Called from the pointer-up handler.
pub fn record_flare(u: f32, v: f32) {
    push(EvOut::Flare { u, v });
}

/// Record a carve drag-release for broadcast.
pub fn record_carve(u: f32, v: f32, motion_n: f32, angle01: f32) {
    push(EvOut::Carve {
        u,
        v,
        m: motion_n,
        g: angle01,
    });
}

/// Record a surface ripple for broadcast. Hooked into `InputWiring::queue_ripple`
/// so every ripple (tap, drag, flare, carve) is captured from one place.
pub fn record_ripple(uv: [f32; 2], amp: f32) {
    push(EvOut::Ripple {
        u: uv[0],
        v: uv[1],
        p: amp,
    });
}

// ── Performer exports (driven by the broadcast loop in index.html) ──

/// Arm gesture recording. Called once the broadcast socket has authenticated.
#[wasm_bindgen]
pub fn perf_enable_broadcast() {
    BROADCASTING.with(|b| b.set(true));
}

/// Drain the recorded gesture events as a JSON array of ready-to-send
/// `{t:"ev",...}` messages, clearing the buffer.
#[wasm_bindgen]
pub fn perf_drain_events() -> String {
    OUTBOX.with(|o| {
        let mut o = o.borrow_mut();
        if o.is_empty() {
            return "[]".to_string();
        }
        let mut s = String::from("[");
        for (i, ev) in o.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            match ev {
                EvOut::Flare { u, v } => s.push_str(&format!(
                    "{{\"t\":\"ev\",\"e\":\"flare\",\"u\":{u:.4},\"v\":{v:.4}}}"
                )),
                EvOut::Carve { u, v, m, g } => s.push_str(&format!(
                    "{{\"t\":\"ev\",\"e\":\"carve\",\"u\":{u:.4},\"v\":{v:.4},\"m\":{m:.4},\"g\":{g:.4}}}"
                )),
                EvOut::Ripple { u, v, p } => s.push_str(&format!(
                    "{{\"t\":\"ev\",\"e\":\"ripple\",\"u\":{u:.4},\"v\":{v:.4},\"p\":{p:.4}}}"
                )),
            }
        }
        s.push(']');
        o.clear();
        s
    })
}

/// The performer's current pointer swirl as a `{t:"ev",e:"swirl",...}` message,
/// or an empty string when idle — so the swirl streams only during a gesture.
#[wasm_bindgen]
pub fn perf_get_swirl() -> String {
    let mut out = String::new();
    with_perf(|perf| {
        let ms = perf.mouse_state.borrow();
        if !ms.down && ms.gesture_energy <= 0.02 {
            return;
        }
        let w = perf.canvas.width().max(1) as f32;
        let h = perf.canvas.height().max(1) as f32;
        let u = (ms.x / w).clamp(0.0, 1.0);
        let v = (ms.y / h).clamp(0.0, 1.0);
        let s = ms.gesture_energy.clamp(0.0, 2.4);
        let a = if ms.down { 1 } else { 0 };
        out = format!(
            "{{\"t\":\"ev\",\"e\":\"swirl\",\"u\":{u:.4},\"v\":{v:.4},\"s\":{s:.4},\"a\":{a}}}"
        );
    });
    out
}

// ── Display side: apply incoming events (called from index.html) ──

/// Whether the display appliers are wired and ready.
#[wasm_bindgen]
pub fn perf_ready() -> bool {
    PERF.with(|p| p.borrow().is_some())
}

/// Drive the local swirl from the performer's pointer by feeding the shared
/// mouse state, so the existing per-frame swirl pipeline follows it.
#[wasm_bindgen]
pub fn perf_apply_swirl(u: f32, v: f32, s: f32, a: bool) {
    with_perf(|perf| {
        let w = perf.canvas.width().max(1) as f32;
        let h = perf.canvas.height().max(1) as f32;
        let mut ms = perf.mouse_state.borrow_mut();
        ms.x = u.clamp(0.0, 1.0) * w;
        ms.y = v.clamp(0.0, 1.0) * h;
        ms.down = a;
        ms.gesture_energy = ms.gesture_energy.max(s.clamp(0.0, 2.4));
    });
}

/// Queue a surface ripple at the given uv (visual only).
#[wasm_bindgen]
pub fn perf_ripple(u: f32, v: f32, amp: f32) {
    with_perf(|perf| {
        *perf.queued_ripple_uv.borrow_mut() = Some(RippleEvent {
            uv: [u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)],
            amp: amp.clamp(0.0, 2.8),
        });
    });
}

/// Fire the performer's tap-flare chord locally (audio only; the ripple arrives
/// as its own `ripple` event).
#[wasm_bindgen]
pub fn perf_flare(u: f32, v: f32) {
    with_perf(|perf| {
        let eng = perf.engine.borrow();
        audio::emit_flare_chord(
            &perf.audio_ctx,
            &eng,
            &perf.voice_gains,
            &perf.delay_sends,
            &perf.reverb_sends,
            u.clamp(0.0, 1.0),
            v.clamp(0.0, 1.0),
        );
    });
}

/// Fire the performer's carve drag-release arpeggio locally (audio only).
#[wasm_bindgen]
pub fn perf_carve(u: f32, v: f32, motion_n: f32, angle01: f32) {
    with_perf(|perf| {
        let eng = perf.engine.borrow();
        audio::emit_carve_chord(
            &perf.audio_ctx,
            &eng,
            &perf.voice_gains,
            &perf.delay_sends,
            &perf.reverb_sends,
            u.clamp(0.0, 1.0),
            v.clamp(0.0, 1.0),
            motion_n.clamp(0.0, 1.0),
            angle01.clamp(0.0, 1.0),
        );
    });
}
