//! Realtime control surface for the separate browser control panel.
//!
//! The `/control` page sends same-origin browser messages to the surface page,
//! which calls these exported setters to mutate the live engine / audio graph
//! that `wasm_app` installed. Everything is a safe no-op until `install` runs.

use crate::core::{scale_for_name, scale_name, Bpm, Cents, MusicEngine};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys as web;

struct Control {
    engine: Rc<RefCell<MusicEngine>>,
    paused: Rc<RefCell<bool>>,
    master_gain: web::GainNode,
    audio_ctx: web::AudioContext,
}

thread_local! {
    static CONTROL: RefCell<Option<Control>> = const { RefCell::new(None) };
}

/// Stash the live handles so the exported setters can drive them. Called once
/// from `wasm_app` after the engine and audio graph are built.
pub fn install(
    engine: Rc<RefCell<MusicEngine>>,
    paused: Rc<RefCell<bool>>,
    master_gain: web::GainNode,
    audio_ctx: web::AudioContext,
) {
    CONTROL.with(|c| {
        *c.borrow_mut() = Some(Control {
            engine,
            paused,
            master_gain,
            audio_ctx,
        });
    });
}

fn with_control<F: FnOnce(&Control)>(f: F) {
    CONTROL.with(|c| {
        if let Some(ctl) = c.borrow().as_ref() {
            f(ctl);
        }
    });
}

/// Whether the control surface is installed and ready to receive parameters.
#[wasm_bindgen]
pub fn control_ready() -> bool {
    CONTROL.with(|c| c.borrow().is_some())
}

/// Resume audio and start playback — the one user gesture iOS requires.
#[wasm_bindgen]
pub fn control_start() {
    with_control(|c| {
        _ = c.audio_ctx.resume();
        *c.paused.borrow_mut() = false;
    });
}

#[wasm_bindgen]
pub fn control_set_bpm(value: f32) {
    with_control(|c| c.engine.borrow_mut().set_bpm(Bpm::new(value)));
}

#[wasm_bindgen]
pub fn control_set_detune(cents: f32) {
    with_control(|c| c.engine.borrow_mut().set_detune_cents(Cents::new(cents)));
}

#[wasm_bindgen]
pub fn control_set_root(midi: i32) {
    with_control(|c| c.engine.borrow_mut().params.root_midi = midi.clamp(0, 127));
}

#[wasm_bindgen]
pub fn control_set_scale(name: &str) {
    if let Some(scale) = scale_for_name(name) {
        with_control(|c| c.engine.borrow_mut().params.scale = scale);
    }
}

#[wasm_bindgen]
pub fn control_set_seed(seed: u32) {
    with_control(|c| c.engine.borrow_mut().reseed_all(seed as u64));
}

#[wasm_bindgen]
pub fn control_set_paused(paused: bool) {
    with_control(|c| *c.paused.borrow_mut() = paused);
}

#[wasm_bindgen]
pub fn control_set_volume(value: f32) {
    with_control(|c| {
        _ = c.master_gain.gain().set_value(value.clamp(0.0, 1.0));
    });
}

/// Current engine parameters as JSON — for the control panel to reflect, and for tests.
#[wasm_bindgen]
pub fn control_get_state() -> String {
    let mut out = String::from("{}");
    with_control(|c| {
        let e = c.engine.borrow();
        out = format!(
            "{{\"bpm\":{},\"detune\":{},\"root\":{},\"scale\":\"{}\",\"paused\":{},\"volume\":{}}}",
            e.params.bpm.get(),
            e.params.detune_cents.get(),
            e.params.root_midi,
            scale_name(e.params.scale),
            *c.paused.borrow(),
            c.master_gain.gain().value(),
        );
    });
    out
}
