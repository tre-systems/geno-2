//! The default Geno-2 instrument definition.
//!
//! Shared by the realtime app (`wasm_app`) and the offline renderer (`offline`)
//! so both produce the same sound from a given seed — one source of truth for the
//! voice layout, tempo, scale, and root.

use crate::core::{Bpm, Cents, EngineParams, VoiceConfig, Waveform, DORIAN};
use glam::Vec3;

/// Default deterministic seed used by the realtime app.
pub const DEFAULT_SEED: u64 = 42;

/// The three-voice default instrument layout (saw bass, triangle mid, sine lead).
pub fn default_voice_configs() -> Vec<VoiceConfig> {
    vec![
        VoiceConfig {
            waveform: Waveform::Saw,
            base_position: Vec3::new(-1.25, 0.0, 0.42),
            trigger_probability: 0.58,
            octave_offset: -2,
            base_duration: 0.96,
        },
        VoiceConfig {
            waveform: Waveform::Triangle,
            base_position: Vec3::new(1.05, 0.0, -0.88),
            trigger_probability: 0.64,
            octave_offset: 0,
            base_duration: 0.62,
        },
        VoiceConfig {
            waveform: Waveform::Sine,
            base_position: Vec3::new(0.10, 0.0, -0.48),
            trigger_probability: 0.48,
            octave_offset: 1,
            base_duration: 0.42,
        },
    ]
}

/// Default engine parameters (tempo, scale, tonal center).
pub fn default_engine_params() -> EngineParams {
    EngineParams {
        bpm: Bpm::new(84.0),
        scale: DORIAN,
        root_midi: 62, // D4
        detune_cents: Cents::default(),
    }
}
