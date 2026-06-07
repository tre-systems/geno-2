//! Offline, deterministic render of the default instrument to a stereo WAV.
//!
//! This builds the *same* Web Audio graph the realtime app uses, but on an
//! `OfflineAudioContext`, so a given seed renders the same piece every time,
//! faster than realtime, with no canvas, audio device, or user gesture. (The
//! browser's convolution reverb/HRTF leaves sub-perceptual, ~-120 dBFS
//! floating-point variance between runs; the music is identical.) The realtime
//! app and this renderer share one instrument definition (`crate::instrument`)
//! so the bounce matches what you hear and play.

use crate::audio;
use crate::constants::{
    DIST_NORM_DIVISOR, D_SEND_BASE, D_SEND_CLAMP_MAX, D_SEND_SPAN, LEVEL_BASE, LEVEL_SPAN,
    R_SEND_BASE, R_SEND_CLAMP_MAX, R_SEND_SPAN,
};
use crate::core::{MusicEngine, NoteEvent, Waveform};
use crate::instrument;
use glam::Vec3;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys as web;

/// Render the default instrument to a reproducible stereo 32-bit-float WAV for
/// the given `seed` and `duration_sec`. Returns the complete WAV file bytes.
#[wasm_bindgen]
pub async fn render_audio_wav(
    seed: u32,
    duration_sec: f64,
    sample_rate: f32,
) -> Result<Vec<u8>, JsValue> {
    let sr = if sample_rate >= 8000.0 {
        sample_rate
    } else {
        48000.0
    };
    let duration = duration_sec.clamp(0.1, 1800.0);
    let length = ((sr as f64) * duration).ceil() as u32;

    let ctx = web::OfflineAudioContext::new_with_number_of_channels_and_length_and_sample_rate(
        2, length, sr,
    )
    .map_err(|e| JsValue::from_str(&format!("OfflineAudioContext: {e:?}")))?;
    let base: &web::BaseAudioContext = &ctx;

    // A previous (realtime or offline) run may have left realtime-clock end
    // times in the shared voice pool; clear it so the polyphony cap is correct.
    audio::reset_voice_pool();

    // Match the realtime listener so HRTF panning is consistent.
    base.listener().set_position(0.0, 0.0, 1.5);

    // Build the same FX + per-voice graph as the realtime app.
    let fx = audio::build_fx_buses(base).map_err(|e| JsValue::from_str(&format!("fx: {e:?}")))?;
    let configs = instrument::default_voice_configs();
    let positions: Vec<Vec3> = configs.iter().map(|c| c.base_position).collect();
    let routing = audio::wire_voices(
        base,
        &positions,
        &fx.master_gain,
        &fx.delay_in,
        &fx.reverb_in,
    )
    .map_err(|e| JsValue::from_str(&format!("voices: {e:?}")))?;

    // The realtime frame loop ramps voice levels / sends every frame; with no
    // interaction they settle to constants, so set those steady values once.
    for (i, pos) in positions.iter().enumerate() {
        let dist = (pos.x * pos.x + pos.z * pos.z).sqrt();
        let norm = (dist / DIST_NORM_DIVISOR).clamp(0.0, 1.0);
        let lvl = LEVEL_BASE + LEVEL_SPAN * (1.0 - norm);
        let d_amt = (D_SEND_BASE + D_SEND_SPAN * pos.x.abs().min(1.0)).clamp(0.0, D_SEND_CLAMP_MAX);
        let r_amt = (R_SEND_BASE + R_SEND_SPAN * norm).clamp(0.0, R_SEND_CLAMP_MAX);
        routing.voice_gains[i].gain().set_value(lvl);
        routing.delay_sends[i].gain().set_value(d_amt);
        routing.reverb_sends[i].gain().set_value(r_amt);
    }

    // Enumerate every deterministic note for the whole duration up front.
    let waveforms: Vec<Waveform> = configs.iter().map(|c| c.waveform).collect();
    let mut engine = MusicEngine::new(configs, instrument::default_engine_params(), seed as u64);
    let step = engine.step_duration();
    let mut events: Vec<NoteEvent> = Vec::new();
    let mut t = 0.0_f64;
    while t < duration {
        engine.generate_step(t, &mut events);
        t += step;
    }

    // Schedule every note on the offline graph. `now` = the note's onset so the
    // polyphony cap prunes voices that ended before it (overlap-based, exact).
    for ev in &events {
        audio::trigger_one_shot(
            base,
            ev.start_time,
            waveforms[ev.voice_index],
            ev.frequency_hz,
            ev.velocity,
            ev.duration_sec as f64,
            ev.start_time,
            &routing.voice_gains[ev.voice_index],
            &routing.delay_sends[ev.voice_index],
            &routing.reverb_sends[ev.voice_index],
        );
    }

    // Render offline and pull the PCM back out.
    let promise = ctx
        .start_rendering()
        .map_err(|e| JsValue::from_str(&format!("start_rendering: {e:?}")))?;
    let rendered = JsFuture::from(promise).await?;
    let buffer: web::AudioBuffer = rendered
        .dyn_into()
        .map_err(|_| JsValue::from_str("rendered value was not an AudioBuffer"))?;
    let left = buffer.get_channel_data(0)?;
    let right = if buffer.number_of_channels() > 1 {
        buffer.get_channel_data(1)?
    } else {
        left.clone()
    };

    Ok(encode_wav_f32(&left, &right, sr))
}

/// Encode interleaved stereo f32 PCM as a canonical 32-bit IEEE-float WAV.
fn encode_wav_f32(left: &[f32], right: &[f32], sample_rate: f32) -> Vec<u8> {
    let n = left.len().min(right.len());
    let channels: u16 = 2;
    let bits: u16 = 32;
    let sr = sample_rate as u32;
    let block_align: u16 = channels * bits / 8;
    let byte_rate: u32 = sr * block_align as u32;
    let data_len: u32 = (n * channels as usize * (bits as usize / 8)) as u32;

    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes()); // WAVE_FORMAT_IEEE_FLOAT
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sr.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..n {
        out.extend_from_slice(&left[i].to_le_bytes());
        out.extend_from_slice(&right[i].to_le_bytes());
    }
    out
}
