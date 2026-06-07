use crate::audio;
use crate::constants::*;
use crate::core::{MusicEngine, NoteEvent, Waveform};
use crate::input;
use crate::render;
use glam::Vec3;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys as web;
use web_time::Instant;

pub struct FrameContext<'a> {
    pub engine: Rc<RefCell<MusicEngine>>,
    pub paused: Rc<RefCell<bool>>,
    pub pulses: Rc<RefCell<Vec<f32>>>,

    pub canvas: web::HtmlCanvasElement,
    pub mouse: Rc<RefCell<input::MouseState>>,

    pub audio_ctx: web::AudioContext,
    pub listener: web::AudioListener,
    pub voice_gains: Rc<Vec<web::GainNode>>,
    pub delay_sends: Rc<Vec<web::GainNode>>,
    pub reverb_sends: Rc<Vec<web::GainNode>>,
    pub voice_panners: Vec<web::PannerNode>,

    pub reverb_wet: web::GainNode,
    pub delay_wet: web::GainNode,
    pub delay_feedback: web::GainNode,
    pub sat_pre: web::GainNode,
    pub sat_wet: web::GainNode,
    pub sat_dry: web::GainNode,

    pub analyser: Option<web::AnalyserNode>,
    pub analyser_buf: Rc<RefCell<Vec<f32>>>,

    pub gpu: Option<render::GpuState<'a>>,
    pub queued_ripple_uv: Rc<RefCell<Option<input::RippleEvent>>>,

    pub last_instant: Instant,
    pub prev_uv: [f32; 2],
    pub swirl_energy: f32,
    pub swirl_pos: [f32; 2],
    pub swirl_vel: [f32; 2],
    pub swirl_initialized: bool,
    pub pulse_energy: [f32; 3],
    pub visual_voice_positions: Vec<Vec3>,
    pub visual_swirl_strength: f32,
    pub audio_visual_energy: f32,
    pub last_audio_ripple_time: f64,

    /// Next grid-step time on the audio clock — the lookahead scheduler's cursor.
    pub next_note_time: f64,
    /// Notes scheduled ahead but not yet sounded; drained as their time arrives
    /// so the visuals couple to the audio (see `drain_played_visuals`).
    pub pending_visuals: Vec<PendingVisual>,
}

/// A scheduled note's visual onset, applied when its audio-clock time arrives.
#[derive(Clone, Copy)]
pub struct PendingVisual {
    pub time: f64,
    pub voice: usize,
    pub velocity: f32,
}

impl<'a> FrameContext<'a> {
    pub fn frame(&mut self) {
        let now = Instant::now();
        let dt = now - self.last_instant;
        self.last_instant = now;
        let dt_sec = dt.as_secs_f32();

        let audio_time = self.audio_ctx.current_time();
        // Schedule notes ahead on the audio clock (sample-accurate timing,
        // decoupled from rAF jitter), then surface the notes that have just
        // sounded so the visuals pulse in sync with what's audible.
        if !*self.paused.borrow() {
            self.schedule_ahead(audio_time);
        }
        let played = self.drain_played_visuals(audio_time);

        let pulses_copy: Vec<f32> = {
            let mut pulses_ref = self.pulses.borrow_mut();
            let n = pulses_ref.len().min(3);
            for ev in &played {
                if ev.voice < n {
                    self.pulse_energy[ev.voice] =
                        (self.pulse_energy[ev.voice] + ev.velocity).min(1.8);
                }
            }
            smooth_pulses(&mut pulses_ref, &mut self.pulse_energy, dt_sec);
            pulses_ref.clone()
        };

        let note_transient = if played.is_empty() {
            0.0
        } else {
            let sum: f32 = played.iter().map(|ev| ev.velocity).sum();
            (sum / played.len() as f32).clamp(0.0, 1.0)
        };
        let pulse_n = pulses_copy.len().min(3).max(1);
        let pulse_mean = pulses_copy.iter().take(pulse_n).copied().sum::<f32>() / pulse_n as f32;

        // Swirl input and energy (no RefCell borrow active)
        let (uv, mouse_down, gesture_energy, gesture_flash, gesture_spin) = {
            let mut ms = self.mouse.borrow_mut();
            ms.gesture_energy *= (-dt_sec * 1.55).exp();
            ms.gesture_flash = (ms.gesture_flash - dt_sec * 0.95).max(0.0);
            ms.gesture_spin *= (-dt_sec * 2.20).exp();
            (
                input::mouse_uv(&self.canvas, &ms),
                ms.down,
                ms.gesture_energy,
                ms.gesture_flash,
                ms.gesture_spin.abs(),
            )
        };
        self.update_swirl(uv, dt_sec, mouse_down, gesture_energy, gesture_spin);

        // Global FX modulation
        apply_global_fx_swirl(
            &self.reverb_wet,
            &self.delay_wet,
            &self.delay_feedback,
            &self.sat_pre,
            &self.sat_wet,
            &self.sat_dry,
            self.swirl_energy,
            gesture_flash,
            uv,
        );

        // Per-voice audio positioning and sends
        let voice_positions_snapshot: Vec<Vec3> = {
            let eng = self.engine.borrow();
            eng.voices.iter().map(|v| v.position).collect()
        };

        if self.visual_voice_positions.len() != voice_positions_snapshot.len() {
            self.visual_voice_positions = voice_positions_snapshot.clone();
        }
        for i in 0..self.voice_panners.len() {
            let pos = voice_positions_snapshot[i];
            self.voice_panners[i].position_x().set_value(pos.x as f32);
            self.voice_panners[i].position_y().set_value(pos.y as f32);
            self.voice_panners[i].position_z().set_value(pos.z as f32);
            let dist = (pos.x * pos.x + pos.z * pos.z).sqrt();
            let mut d_amt = (D_SEND_BASE + D_SEND_SPAN * pos.x.abs().min(1.0)).clamp(0.0, 1.0);
            let mut r_amt = (R_SEND_BASE
                + R_SEND_SPAN * (dist / DIST_NORM_DIVISOR).clamp(0.0, 1.0))
            .clamp(0.0, R_SEND_CLAMP_MAX);
            let boost = 1.0 + SEND_BOOST_COEFF * self.swirl_energy;
            d_amt = (d_amt * boost).clamp(0.0, D_SEND_CLAMP_MAX);
            r_amt = (r_amt * boost).clamp(0.0, R_SEND_CLAMP_MAX);
            self.delay_sends[i].gain().set_value(d_amt);
            self.reverb_sends[i].gain().set_value(r_amt);
            let lvl = (LEVEL_BASE + LEVEL_SPAN * (1.0 - (dist / DIST_NORM_DIVISOR).clamp(0.0, 1.0)))
                as f32;
            self.voice_gains[i].gain().set_value(lvl);
        }

        let mut analyser_avg = 0.0_f32;
        let mut ambient_hint =
            (0.22 * self.swirl_energy + 0.48 * gesture_flash + 0.30 * gesture_energy)
                .clamp(0.0, 1.0);

        // Optional analyser-driven ambient energy
        if let Some(a) = &self.analyser {
            let bins = a.frequency_bin_count() as usize;
            {
                let mut buf = self.analyser_buf.borrow_mut();
                if buf.len() != bins {
                    buf.resize(bins, 0.0);
                }
                a.get_float_frequency_data(&mut buf);
            }
            let mut sum = 0.0f32;
            let take = (bins.min(16)) as u32;
            for i in 0..take {
                let v = self.analyser_buf.borrow()[i as usize];
                let lin = ((v + 100.0) / 100.0).clamp(0.0, 1.0);
                sum += lin;
            }
            let avg = sum / take as f32;
            let n = pulses_copy.len().min(3);
            {
                // Feed the analyser energy back into the shared pulse buffer.
                let mut pulses_ref = self.pulses.borrow_mut();
                for i in 0..n {
                    pulses_ref[i] = (pulses_ref[i] + avg * 0.05).min(1.5);
                }
            }
            analyser_avg = avg;
            ambient_hint = (avg * 0.74 + 0.26 * ambient_hint).clamp(0.0, 1.0);
        }

        let audio_visual_target =
            (0.44 * pulse_mean + 0.34 * note_transient + 0.22 * analyser_avg).clamp(0.0, 1.2);
        self.audio_visual_energy =
            smooth_towards(self.audio_visual_energy, audio_visual_target, dt_sec, 0.15);

        ambient_hint =
            (0.44 * ambient_hint + 0.34 * self.audio_visual_energy + 0.22 * note_transient)
                .clamp(0.0, 1.0);

        if note_transient > 0.48
            && self.audio_visual_energy > 0.35
            && (audio_time - self.last_audio_ripple_time) > 0.13
        {
            let queue_empty = self.queued_ripple_uv.borrow().is_none();
            if queue_empty {
                self.last_audio_ripple_time = audio_time;
                *self.queued_ripple_uv.borrow_mut() = Some(input::RippleEvent {
                    uv: self.swirl_pos,
                    amp: (0.48 + 0.78 * note_transient + 0.36 * self.audio_visual_energy)
                        .clamp(0.4, 1.5),
                });
            }
        }

        let voice_step_limit =
            ((0.70 + 0.95 * self.audio_visual_energy + 0.40 * self.swirl_energy) * dt_sec)
                .max(0.010);
        for i in 0..self.visual_voice_positions.len() {
            self.visual_voice_positions[i] = slew_towards_vec3(
                self.visual_voice_positions[i],
                voice_positions_snapshot[i],
                voice_step_limit,
            );
        }

        // Camera + listener
        let cam_eye = Vec3::new(0.0, 0.0, CAMERA_Z);
        let cam_target = Vec3::ZERO;
        update_listener_to_camera(&self.listener, cam_eye, cam_target);

        if let Some(g) = &mut self.gpu {
            g.set_camera(cam_eye, cam_target);
            g.set_ambient_clear(ambient_hint);
            if let Some(ripple) = self.queued_ripple_uv.borrow_mut().take() {
                g.set_ripple(ripple.uv, ripple.amp);
            }
            let speed_norm = ((self.swirl_vel[0] * self.swirl_vel[0]
                + self.swirl_vel[1] * self.swirl_vel[1])
                .sqrt()
                / 1.0)
                .clamp(0.0, 1.0);
            let target_strength = (0.16
                + 0.62 * self.swirl_energy
                + 0.26 * speed_norm
                + 0.60 * gesture_energy
                + 0.34 * gesture_flash
                + 0.42 * self.audio_visual_energy
                + 0.18 * note_transient)
                .clamp(0.0, 2.4);
            let smoothed_strength =
                smooth_towards(self.visual_swirl_strength, target_strength, dt_sec, 0.17);
            self.visual_swirl_strength = slew_towards_scalar(
                self.visual_swirl_strength,
                smoothed_strength,
                (1.05 + 1.25 * self.audio_visual_energy) * dt_sec,
            )
            .clamp(0.0, 2.4);
            g.set_swirl(self.swirl_pos, self.visual_swirl_strength, true);
            let w = self.canvas.width();
            let h = self.canvas.height();
            g.resize_if_needed(w, h);
            let pulse_energy_snapshot: Vec<f32> = {
                let pulses_ref = self.pulses.borrow();
                pulses_ref.clone()
            };

            if let Err(e) = g.render(dt_sec, &self.visual_voice_positions, &pulse_energy_snapshot) {
                log::error!("render error: {:?}", e);
            }
        }
    }

    /// Generate and schedule every grid step within the lookahead window, firing
    /// each note's oscillator at its precise audio-clock time and queueing its
    /// visual onset. A resync when the cursor falls behind `audio_now` (first
    /// frame, a stall, or resume after pause) plus a per-frame cap bound any
    /// catch-up burst.
    fn schedule_ahead(&mut self, audio_now: f64) {
        if self.next_note_time < audio_now {
            self.next_note_time = audio_now + 0.02;
        }
        let horizon = audio_now + SCHEDULE_AHEAD_SEC;
        let waveforms: Vec<Waveform> = {
            let eng = self.engine.borrow();
            eng.configs.iter().map(|cfg| cfg.waveform).collect()
        };
        let mut scheduled: Vec<NoteEvent> = Vec::new();
        let mut steps = 0u32;
        while self.next_note_time < horizon && steps < MAX_SCHEDULE_STEPS_PER_FRAME {
            let t = self.next_note_time;
            scheduled.clear();
            let step_dur = {
                let mut eng = self.engine.borrow_mut();
                eng.generate_step(t, &mut scheduled);
                eng.step_duration()
            };
            self.next_note_time = t + step_dur;
            for ev in &scheduled {
                audio::trigger_one_shot(
                    &self.audio_ctx,
                    waveforms[ev.voice_index],
                    ev.frequency_hz,
                    ev.velocity,
                    ev.duration_sec as f64,
                    ev.start_time,
                    &self.voice_gains[ev.voice_index],
                    &self.delay_sends[ev.voice_index],
                    &self.reverb_sends[ev.voice_index],
                );
                self.pending_visuals.push(PendingVisual {
                    time: ev.start_time,
                    voice: ev.voice_index,
                    velocity: ev.velocity,
                });
            }
            steps += 1;
        }
    }

    /// Remove and return the scheduled notes whose audio-clock time has arrived,
    /// so the frame couples visuals to notes as they actually sound.
    fn drain_played_visuals(&mut self, audio_now: f64) -> Vec<PendingVisual> {
        let played: Vec<PendingVisual> = self
            .pending_visuals
            .iter()
            .copied()
            .filter(|p| p.time <= audio_now)
            .collect();
        self.pending_visuals.retain(|p| p.time > audio_now);
        played
    }

    fn update_swirl(
        &mut self,
        uv: [f32; 2],
        dt_sec: f32,
        mouse_down: bool,
        gesture_energy: f32,
        gesture_spin: f32,
    ) {
        step_inertial_swirl(
            &mut self.swirl_initialized,
            &mut self.swirl_pos,
            &mut self.swirl_vel,
            uv,
            dt_sec,
        );
        let du = uv[0] - self.prev_uv[0];
        let dv = uv[1] - self.prev_uv[1];
        let pointer_speed = ((du * du + dv * dv).sqrt() / (dt_sec + 1e-5)).min(POINTER_SPEED_MAX);
        let swirl_speed =
            (self.swirl_vel[0] * self.swirl_vel[0] + self.swirl_vel[1] * self.swirl_vel[1]).sqrt();
        let target = ((pointer_speed * SWIRL_TARGET_WEIGHT_POINTER)
            + (swirl_speed * SWIRL_TARGET_WEIGHT_VELOCITY)
            + 0.58 * gesture_energy
            + 0.24 * gesture_spin
            + if mouse_down {
                SWIRL_TARGET_CLICK_BONUS
            } else {
                0.0
            })
        .clamp(0.0, 1.0);
        self.swirl_energy = (1.0 - SWIRL_ENERGY_BLEND_ALPHA) * self.swirl_energy
            + SWIRL_ENERGY_BLEND_ALPHA * target;
        self.prev_uv = uv;
    }
}

#[inline]
fn smooth_pulses(pulses: &mut [f32], pulse_energy: &mut [f32; 3], dt_sec: f32) {
    let n = pulses.len().min(3);
    let energy_decay = (-dt_sec * PULSE_ENERGY_DECAY_PER_SEC).exp();
    for i in 0..n {
        pulse_energy[i] *= energy_decay;
    }
    let tau_up = PULSE_RISE_TAU_SEC;
    let tau_down = PULSE_FALL_TAU_SEC;
    let alpha_up = 1.0 - (-dt_sec / tau_up).exp();
    let alpha_down = 1.0 - (-dt_sec / tau_down).exp();
    for i in 0..n {
        let target = pulse_energy[i].clamp(0.0, 1.5);
        let alpha = if target > pulses[i] {
            alpha_up
        } else {
            alpha_down
        };
        pulses[i] += (target - pulses[i]) * alpha;
    }
}

#[inline]
fn smooth_towards(current: f32, target: f32, dt_sec: f32, tau_sec: f32) -> f32 {
    let alpha = 1.0 - (-dt_sec / tau_sec.max(1e-4)).exp();
    current + (target - current) * alpha.clamp(0.0, 1.0)
}

#[inline]
fn slew_towards_scalar(current: f32, target: f32, max_delta: f32) -> f32 {
    current + (target - current).clamp(-max_delta.abs(), max_delta.abs())
}

#[inline]
fn slew_towards_vec3(current: Vec3, target: Vec3, max_step: f32) -> Vec3 {
    let delta = target - current;
    let len = delta.length();
    if len <= max_step.max(1e-6) {
        target
    } else {
        current + delta * (max_step / len)
    }
}

pub async fn init_gpu(canvas: &web::HtmlCanvasElement) -> Option<render::GpuState<'static>> {
    // leak a canvas clone to satisfy 'static lifetime for surface
    let leaked_canvas = Box::leak(Box::new(canvas.clone()));
    match render::GpuState::new(leaked_canvas, CAMERA_Z).await {
        Ok(g) => {
            log::info!("WebGPU initialized successfully");
            Some(g)
        }
        Err(e) => {
            log::error!("WebGPU init error: {:?}", e);

            // Try to show user-friendly message in DOM
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(error_div) = document.get_element_by_id("no-webgpu") {
                        _ = error_div.set_attribute("style", "display: block");
                    }
                }
            }
            None
        }
    }
}

pub fn start_loop(frame_ctx: Rc<RefCell<FrameContext<'static>>>) {
    let tick: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let tick_clone = tick.clone();
    let frame_ctx_tick = frame_ctx.clone();
    *tick.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        frame_ctx_tick.borrow_mut().frame();
        if let Some(w) = web::window() {
            _ = w.request_animation_frame(
                tick_clone
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unchecked_ref(),
            );
        }
    }) as Box<dyn FnMut()>));
    if let Some(w) = web::window() {
        _ = w.request_animation_frame(tick.borrow().as_ref().unwrap().as_ref().unchecked_ref());
    }
}

// --- helpers private to frame ---
fn step_inertial_swirl(
    initialized: &mut bool,
    swirl_pos: &mut [f32; 2],
    swirl_vel: &mut [f32; 2],
    target_uv: [f32; 2],
    dt_sec: f32,
) {
    if !*initialized {
        *swirl_pos = target_uv;
        swirl_vel[0] = 0.0;
        swirl_vel[1] = 0.0;
        *initialized = true;
        return;
    }
    let omega = SWIRL_OMEGA;
    let k = omega * omega;
    let c = 2.0 * omega * SWIRL_DAMPING_RATIO;
    let dx = target_uv[0] - swirl_pos[0];
    let dy = target_uv[1] - swirl_pos[1];
    let ax = k * dx - c * swirl_vel[0];
    let ay = k * dy - c * swirl_vel[1];
    swirl_vel[0] += ax * dt_sec;
    swirl_vel[1] += ay * dt_sec;
    let mut nx = swirl_pos[0] + swirl_vel[0] * dt_sec;
    let mut ny = swirl_pos[1] + swirl_vel[1] * dt_sec;
    let sdx = nx - swirl_pos[0];
    let sdy = ny - swirl_pos[1];
    let step = (sdx * sdx + sdy * sdy).sqrt();
    let max_step = SWIRL_MAX_STEP_PER_SEC * dt_sec;
    if step > max_step {
        let inv = 1.0 / (step + 1e-6);
        nx = swirl_pos[0] + sdx * inv * max_step;
        ny = swirl_pos[1] + sdy * inv * max_step;
    }
    swirl_pos[0] = nx.clamp(0.0, 1.0);
    swirl_pos[1] = ny.clamp(0.0, 1.0);
}

fn apply_global_fx_swirl(
    reverb_wet: &web::GainNode,
    delay_wet: &web::GainNode,
    delay_feedback: &web::GainNode,
    sat_pre: &web::GainNode,
    sat_wet: &web::GainNode,
    sat_dry: &web::GainNode,
    swirl_energy: f32,
    gesture_flash: f32,
    uv: [f32; 2],
) {
    _ = reverb_wet.gain().set_value(
        (FX_REVERB_BASE + FX_REVERB_SPAN * swirl_energy + 0.10 * gesture_flash).clamp(0.0, 1.0),
    );
    let echo = ((uv[0] - uv[1]).abs() * 0.85 + (uv[0] * uv[1]).sqrt() * 0.15).clamp(0.0, 1.0);
    let delay_wet_val = (FX_DELAY_WET_BASE
        + FX_DELAY_WET_SWIRL * swirl_energy
        + FX_DELAY_WET_ECHO * echo
        + 0.12 * gesture_flash)
        .clamp(0.0, 1.0);
    let delay_fb_val = (FX_DELAY_FB_BASE
        + FX_DELAY_FB_SWIRL * swirl_energy
        + FX_DELAY_FB_ECHO * echo
        + 0.06 * gesture_flash)
        .clamp(0.0, 0.95);
    _ = delay_wet.gain().set_value(delay_wet_val);
    _ = delay_feedback.gain().set_value(delay_fb_val);
    let fizz = (0.62 * swirl_energy + 0.25 * (uv[0] * (1.0 - uv[1])) + 0.18 * gesture_flash)
        .clamp(0.0, 1.0);
    let drive = (FX_SAT_DRIVE_MIN + (FX_SAT_DRIVE_MAX - FX_SAT_DRIVE_MIN) * fizz)
        .clamp(FX_SAT_DRIVE_MIN, FX_SAT_DRIVE_MAX);
    _ = sat_pre.gain().set_value(drive);
    let wet = (FX_SAT_WET_BASE
        + FX_SAT_WET_SPAN * (0.55 * fizz + 0.35 * swirl_energy + 0.22 * gesture_flash))
        .clamp(0.0, 1.0);
    _ = sat_wet.gain().set_value(wet);
    _ = sat_dry.gain().set_value(1.0 - wet);
}

fn update_listener_to_camera(listener: &web::AudioListener, cam_eye: Vec3, cam_target: Vec3) {
    let fwd = (cam_target - cam_eye).normalize();
    listener.set_position(cam_eye.x as f64, cam_eye.y as f64, cam_eye.z as f64);
    _ = listener.set_orientation(fwd.x as f64, fwd.y as f64, fwd.z as f64, 0.0, 1.0, 0.0);
}
