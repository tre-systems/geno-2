use glam::Vec3;
use rand::prelude::*;
use std::time::Duration;

/// Basic oscillator shape used by synths in the web front-end.
#[derive(Clone, Copy, Debug)]
pub enum Waveform {
    Sine,
    //Square,
    Saw,
    Triangle,
}

/// Static configuration for a voice used at engine construction time.
///
/// Fields:
/// - `waveform`: oscillator type to synthesize this voice in the web frontend
/// - `base_position`: initial engine-space position (XZ plane; Y is typically 0)
/// - `trigger_probability`: chance (0.0-1.0) that this voice triggers on each grid step
/// - `octave_offset`: octave adjustment relative to root note (-2 to +2)
/// - `base_duration`: base note duration in seconds
#[derive(Clone, Debug)]
pub struct VoiceConfig {
    pub waveform: Waveform,
    pub base_position: Vec3,
    pub trigger_probability: f32,
    pub octave_offset: i32,
    pub base_duration: f32,
}

/// A scheduled musical event produced by the engine for playback.
///
/// Fields:
/// - `voice_index`: which voice this event belongs to (index into `voices`)
/// - `frequency_hz`: target pitch in Hertz (already converted from MIDI)
/// - `velocity`: normalized loudness 0..1 (mapped to gain envelope)
/// - `start_time_sec`: absolute start time (AudioContext time) in seconds
/// - `duration_sec`: nominal duration in seconds (envelope length)
#[derive(Clone, Debug, Default)]
pub struct NoteEvent {
    pub voice_index: usize,
    pub frequency_hz: f32,
    pub velocity: f32,
    pub duration_sec: f32,
}

/// Mutable runtime state per voice.
#[derive(Clone, Debug)]
pub struct VoiceState {
    pub position: Vec3,
}

/// Global engine parameters controlling tempo and scale.
///
/// - `bpm` controls the tempo of the scheduler (beats per minute)
/// - `scale` is the allowed pitch degree set, expressed as semitone offsets
/// - `root_midi` is the MIDI note number of the tonal center (e.g., 60 for C4)
/// - `detune_cents` is the global detune offset in cents (-200 to +200)
#[derive(Clone, Debug)]
pub struct EngineParams {
    pub bpm: f32,
    pub scale: &'static [f32],
    pub root_midi: i32,
    pub detune_cents: f32,
}

impl Default for EngineParams {
    fn default() -> Self {
        Self {
            bpm: 92.0,
            scale: DORIAN,
            root_midi: 62, // D4
            detune_cents: 0.0,
        }
    }
}

/// Default five-note scale centered around middle C.
pub const C_MAJOR_PENTATONIC: &[f32] = &[0.0, 2.0, 4.0, 7.0, 9.0, 12.0];

/// Diatonic modes (relative semitone degrees)
pub const IONIAN: &[f32] = &[0.0, 2.0, 4.0, 5.0, 7.0, 9.0, 11.0, 12.0]; // major
pub const DORIAN: &[f32] = &[0.0, 2.0, 3.0, 5.0, 7.0, 9.0, 10.0, 12.0];
pub const PHRYGIAN: &[f32] = &[0.0, 1.0, 3.0, 5.0, 7.0, 8.0, 10.0, 12.0];
pub const LYDIAN: &[f32] = &[0.0, 2.0, 4.0, 6.0, 7.0, 9.0, 11.0, 12.0];
pub const MIXOLYDIAN: &[f32] = &[0.0, 2.0, 4.0, 5.0, 7.0, 9.0, 10.0, 12.0];
pub const AEOLIAN: &[f32] = &[0.0, 2.0, 3.0, 5.0, 7.0, 8.0, 10.0, 12.0]; // natural minor
pub const LOCRIAN: &[f32] = &[0.0, 1.0, 3.0, 5.0, 6.0, 8.0, 10.0, 12.0];

/// Alternative tuning systems (pentatonic variants)
pub const TET19_PENTATONIC: &[f32] = &[0.0, 2.4, 4.8, 7.2, 9.6, 12.0];
pub const TET24_PENTATONIC: &[f32] = &[0.0, 2.5, 5.0, 7.5, 10.0, 12.0];
pub const TET31_PENTATONIC: &[f32] = &[0.0, 2.4, 4.8, 7.2, 9.6, 12.0];

/// Random generative scheduler producing `NoteEvent`s on an eighth-note grid.
///
/// The engine maintains per-voice state and RNGs. On each tick, it advances an
/// internal accumulator based on the configured tempo (`params.bpm`) and emits
/// events aligned to an eighth-note grid. Voices have distinct trigger
/// probabilities, octave ranges, and base durations to create a simple texture.
///
/// Typical usage:
/// - Construct with `MusicEngine::new(configs, params, seed)`
/// - Call `tick(dt, &mut out_events)` regularly to schedule audio
/// - Use `reseed_voice` and `set_voice_position` to interact with engine state
pub struct MusicEngine {
    pub voices: Vec<VoiceState>,
    pub configs: Vec<VoiceConfig>,
    pub params: EngineParams,
    rngs: Vec<StdRng>,
    beat_accum: f64,
    step_counter: u64,
}

impl MusicEngine {
    /// Construct a new engine with voices derived from the provided configs.
    pub fn new(configs: Vec<VoiceConfig>, params: EngineParams, seed: u64) -> Self {
        let voices = configs
            .iter()
            .map(|c| VoiceState {
                position: c.base_position,
            })
            .collect::<Vec<_>>();

        // Derive per-voice RNGs from base seed so we can reseed voices independently
        let rngs = (0..voices.len())
            .map(|i| {
                let mix = seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                StdRng::seed_from_u64(mix)
            })
            .collect::<Vec<_>>();

        Self {
            voices,
            configs,
            params,
            rngs,
            beat_accum: 0.0,
            step_counter: 0,
        }
    }

    /// Set beats-per-minute for the internal scheduler.
    pub fn set_bpm(&mut self, bpm: f32) {
        if !bpm.is_finite() {
            return;
        }
        self.params.bpm = bpm.clamp(1.0, 400.0);
    }

    /// Set the global detune offset in cents.
    /// Range: -200 to +200 cents (±2 semitones)
    pub fn set_detune_cents(&mut self, detune_cents: f32) {
        self.params.detune_cents = detune_cents.clamp(-200.0, 200.0);
    }

    /// Adjust the global detune offset by the specified amount in cents.
    /// The result is clamped to the valid range of -200 to +200 cents.
    pub fn adjust_detune_cents(&mut self, delta_cents: f32) {
        let new_detune = self.params.detune_cents + delta_cents;
        self.set_detune_cents(new_detune);
    }

    /// Reset the global detune offset to 0 cents (no detune).
    pub fn reset_detune(&mut self) {
        self.params.detune_cents = 0.0;
    }

    /// Update the engine-space position of a voice.
    pub fn set_voice_position(&mut self, voice_index: usize, pos: Vec3) {
        if let Some(v) = self.voices.get_mut(voice_index) {
            v.position = pos;
        }
    }

    /// Reseed the per-voice RNG. If `seed` is None, a new random seed is chosen.
    pub fn reseed_voice(&mut self, voice_index: usize, seed: Option<u64>) {
        if let Some(r) = self.rngs.get_mut(voice_index) {
            let new_seed = seed.unwrap_or_else(|| r.gen());
            *r = StdRng::seed_from_u64(new_seed);
        }
    }

    /// Advance the scheduler by `dt`, pushing any newly scheduled `NoteEvent`s into `out_events`.
    pub fn tick(&mut self, dt: Duration, out_events: &mut Vec<NoteEvent>) {
        let bpm = self.params.bpm as f64;
        if !bpm.is_finite() || bpm <= 0.0 {
            return;
        }
        let step = (60.0 / bpm) / 2.0;
        if !step.is_finite() || step <= 0.0 {
            return;
        }
        self.beat_accum += dt.as_secs_f64();
        while self.beat_accum >= step {
            // eighth notes grid
            self.beat_accum -= step;
            self.schedule_step(out_events);
        }
    }

    /// Schedule a single grid step for all voices.
    fn schedule_step(&mut self, out_events: &mut Vec<NoteEvent>) {
        let step = self.step_counter;
        self.step_counter = self.step_counter.wrapping_add(1);
        let phrase_idx = ((step / 6) as usize) % PHRASE_ROOT_SHIFTS.len();
        let phrase_shift = PHRASE_ROOT_SHIFTS[phrase_idx];

        for (i, voice) in self.voices.iter().enumerate() {
            let rng = &mut self.rngs[i];
            let scale = self.params.scale;
            if scale.is_empty() {
                continue;
            }
            let scale_len = scale.len();
            let single_tone_scale = scale_len == 1;

            let gate = if single_tone_scale {
                1.0
            } else {
                let (steps, hits, rotate) = polymeter_for_voice(i);
                let hard_gate = euclidean_gate(step, steps, hits, rotate);
                let swing = 0.5 + 0.5 * (step as f32 * (0.09 + i as f32 * 0.03)).sin();
                let travel = 0.5
                    + 0.5
                        * (step as f32 * (0.07 + i as f32 * 0.04)
                            + voice.position.x * 1.3
                            + voice.position.z * 0.9)
                            .cos();
                (0.62 * hard_gate + 0.24 * swing + 0.14 * travel).clamp(0.0, 1.0)
            };
            let accent = if single_tone_scale {
                0.0
            } else {
                accent_gate(step, i)
            };
            let prob = if single_tone_scale {
                self.configs[i].trigger_probability.clamp(0.0, 1.0)
            } else {
                (self.configs[i].trigger_probability * (0.16 + 0.76 * gate)
                    + 0.10 * accent
                    + 0.08 * (phrase_idx as f32 / PHRASE_ROOT_SHIFTS.len() as f32))
                    .clamp(0.02, 0.98)
            };
            if rng.gen::<f32>() >= prob {
                continue;
            }

            let degree = if single_tone_scale {
                scale[0]
            } else {
                let motif = motif_for_voice(i, step);
                let stride = (i as i32 * 2) + 1;
                let phase = (step as i32 * stride) + motif + phrase_idx as i32;
                let mut scale_pos = phase % scale_len as i32;
                if scale_pos < 0 {
                    scale_pos += scale_len as i32;
                }
                let mut deg = scale[scale_pos as usize];
                if accent > 0.84 && rng.gen::<f32>() < 0.24 {
                    deg += 12.0;
                }
                deg
            };

            let root_shift = if single_tone_scale { 0.0 } else { phrase_shift };
            let contour = if single_tone_scale {
                0.0
            } else {
                0.42 * (step as f32 * (0.14 + i as f32 * 0.04)).sin()
            };
            let register = if single_tone_scale {
                0.0
            } else {
                match i {
                    0 => {
                        if gate > 0.78 {
                            -12.0
                        } else {
                            -24.0
                        }
                    }
                    1 => 0.0,
                    _ => {
                        if gate > 0.72 {
                            12.0
                        } else {
                            24.0
                        }
                    }
                }
            };

            let octave = self.configs[i].octave_offset;
            let micro_drift = if single_tone_scale {
                0.0
            } else {
                (rng.gen::<f32>() - 0.5) * 0.14
            };
            let midi = self.params.root_midi as f32
                + root_shift
                + degree
                + contour
                + register
                + (octave * 12) as f32
                + micro_drift;
            let freq = midi_to_hz_with_detune(midi, self.params.detune_cents);

            let (vel_base, vel_span, dur_scale, staccato) = match i {
                0 => (0.48, 0.34, 1.45, 0.28),
                1 => (0.30, 0.40, 0.88, 0.48),
                _ => (0.24, 0.52, 0.54, 0.70),
            };
            let vel = (vel_base
                + vel_span * (0.58 * gate + 0.22 * accent + 0.20 * rng.gen::<f32>()))
            .clamp(0.0, 1.0);
            let sustain = (1.0 - staccato * gate).clamp(0.18, 1.35);
            let jitter = 0.72 + 0.48 * rng.gen::<f32>();
            let dur = (self.configs[i].base_duration * dur_scale * sustain * jitter).max(0.04);
            out_events.push(NoteEvent {
                voice_index: i,
                frequency_hz: freq,
                velocity: vel,
                duration_sec: dur,
            });
        }
    }
}

const PHRASE_ROOT_SHIFTS: [f32; 16] = [
    0.0, 0.0, 5.0, 5.0, 3.0, 3.0, 7.0, 7.0, 2.0, 2.0, 8.0, 8.0, 10.0, 10.0, 5.0, 5.0,
];

fn euclidean_gate(step: u64, steps: u32, hits: u32, rotate: u32) -> f32 {
    if steps == 0 || hits == 0 {
        return 0.0;
    }
    let p = ((step as u32) + rotate) % steps;
    let a = (p * hits) / steps;
    let b = (((p + 1) % steps) * hits) / steps;
    if a != b {
        1.0
    } else {
        0.0
    }
}

fn polymeter_for_voice(voice_index: usize) -> (u32, u32, u32) {
    match voice_index % 3 {
        0 => (13, 5, 0),
        1 => (11, 7, 2),
        _ => (17, 4, 5),
    }
}

fn motif_for_voice(voice_index: usize, step: u64) -> i32 {
    const M0: [i32; 12] = [0, 2, -1, 4, 0, 3, -2, 5, 1, -1, 3, 0];
    const M1: [i32; 9] = [0, 1, 3, 2, -1, 4, 1, 2, 0];
    const M2: [i32; 10] = [0, 4, 2, 5, 1, 6, 3, 5, 2, 4];

    match voice_index % 3 {
        0 => M0[step as usize % M0.len()],
        1 => M1[step as usize % M1.len()],
        _ => M2[step as usize % M2.len()],
    }
}

fn accent_gate(step: u64, voice_index: usize) -> f32 {
    let cycle = 16 + voice_index as u64 * 3;
    let phase = (step + 3 * voice_index as u64) % cycle;
    let hard = if phase == 0 || phase == cycle / 2 {
        1.0
    } else if phase.is_multiple_of(4) {
        0.58
    } else {
        0.24
    };
    let swing = 0.5 + 0.5 * (step as f32 * (0.11 + voice_index as f32 * 0.04)).cos();
    (0.65 * hard + 0.35 * swing).clamp(0.0, 1.0)
}

/// Convert a MIDI note number to Hertz (A4=440 Hz).
///
/// Monotonic and exhibits octave symmetry: +12 semitones doubles the frequency.
/// Supports fractional MIDI values for microtonal precision.
pub fn midi_to_hz(midi: f32) -> f32 {
    440.0 * (2.0_f32).powf((midi - 69.0) / 12.0)
}

/// Convert a MIDI note number to Hertz with detune offset in cents.
///
/// The detune_cents parameter allows for microtonal adjustments:
/// - Positive values raise the pitch (e.g., +50¢ = quarter tone sharp)
/// - Negative values lower the pitch (e.g., -50¢ = quarter tone flat)
/// - Range: -200 to +200 cents (±2 semitones)
pub fn midi_to_hz_with_detune(midi: f32, detune_cents: f32) -> f32 {
    let clamped_detune = detune_cents.clamp(-200.0, 200.0);
    let detune_semitones = clamped_detune / 100.0;
    let adjusted_midi = midi + detune_semitones;
    midi_to_hz(adjusted_midi)
}
