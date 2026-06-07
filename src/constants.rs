//! Tuned constants for frame smoothing, interaction, audio sends, and rendering.
//!
//! Values here are hand-tuned; they name the magic numbers used across modules.

// Exponential decay rate for internal pulse energy
pub const PULSE_ENERGY_DECAY_PER_SEC: f32 = 1.28;

// Target smoothing time constants (seconds)
pub const PULSE_RISE_TAU_SEC: f32 = 0.052;
pub const PULSE_FALL_TAU_SEC: f32 = 0.48;

// Pointer speed clamp (normalized units per second)
pub const POINTER_SPEED_MAX: f32 = 10.0;

// Inertial swirl spring parameters
pub const SWIRL_OMEGA: f32 = 1.45; // natural frequency
pub const SWIRL_DAMPING_RATIO: f32 = 0.38; // 0..1 critical at 1
pub const SWIRL_MAX_STEP_PER_SEC: f32 = 0.72; // cap motion per second (in uv units)

// Swirl energy blend weights
pub const SWIRL_TARGET_WEIGHT_POINTER: f32 = 0.32;
pub const SWIRL_TARGET_WEIGHT_VELOCITY: f32 = 0.42;
pub const SWIRL_TARGET_CLICK_BONUS: f32 = 0.35;
pub const SWIRL_ENERGY_BLEND_ALPHA: f32 = 0.18; // new = (1-α)*old + α*target

// Global FX mapping weights
pub const FX_REVERB_BASE: f32 = 0.16;
pub const FX_REVERB_SPAN: f32 = 0.50;

pub const FX_DELAY_WET_BASE: f32 = 0.14;
pub const FX_DELAY_WET_SWIRL: f32 = 0.30;
pub const FX_DELAY_WET_ECHO: f32 = 0.26;

pub const FX_DELAY_FB_BASE: f32 = 0.34;
pub const FX_DELAY_FB_SWIRL: f32 = 0.20;
pub const FX_DELAY_FB_ECHO: f32 = 0.19;

pub const FX_SAT_DRIVE_MIN: f32 = 0.18;
pub const FX_SAT_DRIVE_MAX: f32 = 1.55;
pub const FX_SAT_WET_BASE: f32 = 0.14;
pub const FX_SAT_WET_SPAN: f32 = 0.42;

// Per-voice spatial sends mapping
pub const DIST_NORM_DIVISOR: f32 = 2.7;
pub const D_SEND_BASE: f32 = 0.08;
pub const D_SEND_SPAN: f32 = 0.56;
pub const R_SEND_BASE: f32 = 0.16;
pub const R_SEND_SPAN: f32 = 0.52;
pub const SEND_BOOST_COEFF: f32 = 0.30;
pub const D_SEND_CLAMP_MAX: f32 = 1.2;
pub const R_SEND_CLAMP_MAX: f32 = 1.5;

// Voice level mapping
pub const LEVEL_BASE: f32 = 0.70;
pub const LEVEL_SPAN: f32 = 0.28;

// Camera Z distance, shared by picking and audio-listener alignment.
pub const CAMERA_Z: f32 = 6.0;

// Post-processing defaults
// Bloom blur weights are normalized (sum 1.0), so this is the true composite mix.
pub const BLOOM_STRENGTH: f32 = 0.30;
pub const BLOOM_THRESHOLD: f32 = 0.68;

// Cap the canvas backing scale so high-DPR phones don't render the fullscreen
// shader at 3x+ resolution (the dominant mobile GPU cost).
pub const MAX_DEVICE_PIXEL_RATIO: f64 = 2.0;

// Audio scheduler lookahead (the "two clocks" pattern): generate and schedule
// notes this far ahead on the audio clock so their timing is sample-accurate and
// independent of requestAnimationFrame jitter. ~120 ms absorbs frame drops
// without audible drift.
pub const SCHEDULE_AHEAD_SEC: f64 = 0.12;

// Cap grid steps generated per frame so a resync after a long stall (a
// backgrounded, rAF-throttled tab) can't dump a flood of notes at once.
pub const MAX_SCHEDULE_STEPS_PER_FRAME: u32 = 8;

// Time constant for set_target_at_time smoothing of the per-frame audio params
// (FX wet/feedback, sends, voice gains, panner positions) — removes the zipper
// noise from set_value steps without feeling sluggish.
pub const AUDIO_SMOOTH_SEC: f64 = 0.03;
