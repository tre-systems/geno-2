use crate::core::{Frequency, Waveform};
use glam::Vec3;
use std::cell::RefCell;
use std::rc::Rc;
use web_sys as web;

pub struct FxBuses {
    pub master_gain: web::GainNode,
    pub sat_pre: web::GainNode,
    pub sat_wet: web::GainNode,
    pub sat_dry: web::GainNode,
    pub reverb_in: web::GainNode,
    pub reverb_wet: web::GainNode,
    pub delay_in: web::GainNode,
    pub delay_feedback: web::GainNode,
    pub delay_wet: web::GainNode,
}

pub struct VoiceRouting {
    pub voice_gains: Vec<web::GainNode>,
    pub voice_panners: Vec<web::PannerNode>,
    pub delay_sends: Vec<web::GainNode>,
    pub reverb_sends: Vec<web::GainNode>,
}

// Tag a Web Audio constructor result with a human-readable label for error reporting.
fn named<T>(result: Result<T, wasm_bindgen::JsValue>, label: &str) -> anyhow::Result<T> {
    result.map_err(|e| anyhow::anyhow!("{label}: {e:?}"))
}

fn osc_type(waveform: Waveform) -> web::OscillatorType {
    match waveform {
        Waveform::Sine => web::OscillatorType::Sine,
        Waveform::Saw => web::OscillatorType::Sawtooth,
        Waveform::Triangle => web::OscillatorType::Triangle,
    }
}

fn create_gain(
    audio_ctx: &web::AudioContext,
    value: f32,
    label: &str,
) -> anyhow::Result<web::GainNode> {
    let g =
        web::GainNode::new(audio_ctx).map_err(|e| anyhow::anyhow!("{label} GainNode: {e:?}"))?;
    g.gain().set_value(value);
    Ok(g)
}

pub fn build_fx_buses(audio_ctx: &web::AudioContext) -> anyhow::Result<FxBuses> {
    // Master gain
    let master_gain = create_gain(audio_ctx, 0.46, "Master")?;

    // Subtle master saturation (arctan) with wet/dry mix
    let sat_pre = create_gain(audio_ctx, 0.54, "sat pre")?;
    #[allow(deprecated)]
    let saturator = named(web::WaveShaperNode::new(audio_ctx), "WaveShaperNode")?;
    // Build arctan curve
    let curve_len: u32 = 2048;
    let drive: f32 = 0.92;
    let mut curve: Vec<f32> = Vec::with_capacity(curve_len as usize);
    for i in 0..curve_len {
        let x = (i as f32 / (curve_len - 1) as f32) * 2.0 - 1.0;
        curve.push((2.0 / std::f32::consts::PI) * (drive * x).atan());
    }
    #[allow(deprecated)]
    saturator.set_curve(Some(curve.as_mut_slice()));
    let sat_wet = create_gain(audio_ctx, 0.16, "sat wet")?;
    let sat_dry = create_gain(audio_ctx, 0.84, "sat dry")?;

    // Global tone shaping before saturation blend.
    let master_hp = named(
        web::BiquadFilterNode::new(audio_ctx),
        "Master highpass filter",
    )?;
    master_hp.set_type(web::BiquadFilterType::Highpass);
    master_hp.frequency().set_value(95.0);
    master_hp.q().set_value(0.70);

    let master_lp = named(
        web::BiquadFilterNode::new(audio_ctx),
        "Master lowpass filter",
    )?;
    master_lp.set_type(web::BiquadFilterType::Lowpass);
    master_lp.frequency().set_value(4600.0);
    master_lp.q().set_value(0.45);

    // Gentle master compression + makeup keeps baseline louder while taming peaks.
    let master_comp = named(
        web::DynamicsCompressorNode::new(audio_ctx),
        "DynamicsCompressorNode",
    )?;
    master_comp.threshold().set_value(-22.0);
    master_comp.knee().set_value(18.0);
    master_comp.ratio().set_value(2.9);
    master_comp.attack().set_value(0.003);
    master_comp.release().set_value(0.30);
    let comp_makeup = create_gain(audio_ctx, 1.26, "comp makeup")?;

    // Route master -> tone shaping -> [dry,wet] -> comp -> makeup -> destination.
    _ = master_gain.connect_with_audio_node(&master_hp);
    _ = master_hp.connect_with_audio_node(&master_lp);
    _ = master_lp.connect_with_audio_node(&sat_pre);
    _ = sat_pre.connect_with_audio_node(&saturator);
    _ = saturator.connect_with_audio_node(&sat_wet);
    _ = sat_wet.connect_with_audio_node(&master_comp);
    _ = master_lp.connect_with_audio_node(&sat_dry);
    _ = sat_dry.connect_with_audio_node(&master_comp);
    _ = master_comp.connect_with_audio_node(&comp_makeup);
    _ = comp_makeup.connect_with_audio_node(&audio_ctx.destination());

    // Reverb bus (short glass chamber IR)
    let reverb_in = create_gain(audio_ctx, 1.0, "Reverb in")?;
    let reverb = named(web::ConvolverNode::new(audio_ctx), "ConvolverNode")?;
    reverb.set_normalize(true);
    // Create a short bright impulse response procedurally
    {
        let sr = audio_ctx.sample_rate();
        let seconds = 3.8_f32;
        let len = (sr as f32 * seconds) as u32;
        if let Ok(ir) = audio_ctx.create_buffer(2, len, sr) {
            let dt = 1.0_f32 / sr as f32;
            for ch in 0..2 {
                let mut buf: Vec<f32> = vec![0.0; len as usize];
                let mut t = 0.0_f32;
                // Per-channel xorshift32 state for deterministic noise.
                let mut seed: u32 = if ch == 0 { 0x1234ABCD } else { 0x7890FEDC };
                for sample in buf.iter_mut() {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    let n = (seed as f32 / std::u32::MAX as f32) * 2.0 - 1.0;
                    // Faster decay with soft early emphasis
                    let decay = (-t / 1.60).exp();
                    let early = (1.0 - (t / 0.36)).clamp(0.0, 1.0);
                    *sample = n * decay * (0.38 + 0.62 * early);
                    t += dt;
                }
                _ = ir.copy_to_channel(&mut buf, ch as i32);
            }
            reverb.set_buffer(Some(&ir));
        }
    }
    let reverb_wet = create_gain(audio_ctx, 0.20, "Reverb wet")?;
    _ = reverb_in.connect_with_audio_node(&reverb);
    _ = reverb.connect_with_audio_node(&reverb_wet);
    _ = reverb_wet.connect_with_audio_node(&master_gain);

    // Delay bus with feedback loop and band-limited tone
    let delay_in = create_gain(audio_ctx, 1.0, "Delay in")?;
    let delay = named(audio_ctx.create_delay_with_max_delay_time(3.0), "DelayNode")?;
    delay.delay_time().set_value(0.38);
    let delay_tone = named(web::BiquadFilterNode::new(audio_ctx), "Delay tone filter")?;
    delay_tone.set_type(web::BiquadFilterType::Lowpass);
    delay_tone.frequency().set_value(2450.0);
    delay_tone.q().set_value(0.72);
    let delay_feedback = create_gain(audio_ctx, 0.50, "Delay feedback")?;
    let delay_wet = create_gain(audio_ctx, 0.26, "Delay wet")?;
    _ = delay_in.connect_with_audio_node(&delay);
    _ = delay.connect_with_audio_node(&delay_tone);
    _ = delay_tone.connect_with_audio_node(&delay_feedback);
    _ = delay_feedback.connect_with_audio_node(&delay);
    _ = delay_tone.connect_with_audio_node(&delay_wet);
    _ = delay_wet.connect_with_audio_node(&master_gain);

    Ok(FxBuses {
        master_gain,
        sat_pre,
        sat_wet,
        sat_dry,
        reverb_in,
        reverb_wet,
        delay_in,
        delay_feedback,
        delay_wet,
    })
}

thread_local! {
    /// End times (audio clock) of in-flight one-shot voices, for the polyphony cap.
    static ACTIVE_VOICE_ENDS: RefCell<Vec<f64>> = const { RefCell::new(Vec::new()) };
}

// Fire a simple one-shot oscillator routed through a voice's gain and sends
pub fn trigger_one_shot(
    audio_ctx: &web::AudioContext,
    waveform: Waveform,
    frequency: Frequency,
    velocity: f32,
    duration_sec: f64,
    start_time: f64,
    voice_gain: &web::GainNode,
    delay_send: &web::GainNode,
    reverb_send: &web::GainNode,
) {
    // Polyphony cap: prune voices that have finished, then drop this note if
    // we're maxed out — a frantic gesture burst can't spawn unbounded oscillators.
    let now = audio_ctx.current_time();
    let at_cap = ACTIVE_VOICE_ENDS.with(|ends| {
        let mut ends = ends.borrow_mut();
        ends.retain(|&end| end > now);
        ends.len() >= crate::constants::MAX_POLYPHONY
    });
    if at_cap {
        return;
    }
    let frequency_hz = frequency.hz();
    let src_main = match web::OscillatorNode::new(audio_ctx) {
        Ok(s) => s,
        Err(_) => return,
    };
    let src_chorus = web::OscillatorNode::new(audio_ctx).ok();

    src_main.set_type(osc_type(waveform));
    if let Some(chorus) = &src_chorus {
        chorus.set_type(osc_type(waveform));
    }

    if let Ok(g) = web::GainNode::new(audio_ctx) {
        g.gain().set_value(0.0);
        // Honour the scheduled start time; never schedule in the past.
        let t0 = start_time.max(audio_ctx.current_time() + 0.001);
        let (glide_mul, glide_time, chorus_detune) = match waveform {
            Waveform::Saw => (1.05, 0.14, 9.0),
            Waveform::Triangle => (0.97, 0.18, -7.0),
            Waveform::Sine => (1.00, 0.20, 4.0),
        };
        _ = src_main
            .frequency()
            .set_value_at_time(frequency_hz * glide_mul, t0);
        _ = src_main
            .frequency()
            .linear_ramp_to_value_at_time(frequency_hz, t0 + glide_time);

        if let Some(chorus) = &src_chorus {
            chorus.detune().set_value(chorus_detune);
            _ = chorus
                .frequency()
                .set_value_at_time(frequency_hz * (2.0 - glide_mul), t0);
            _ = chorus
                .frequency()
                .linear_ramp_to_value_at_time(frequency_hz, t0 + glide_time * 1.1);
        }

        let attack = match waveform {
            Waveform::Saw => 0.016,
            Waveform::Triangle => 0.026,
            Waveform::Sine => 0.038,
        };
        let sustain_k = match waveform {
            Waveform::Saw => 0.56,
            Waveform::Triangle => 0.64,
            Waveform::Sine => 0.74,
        };
        let release_tail = match waveform {
            Waveform::Saw => 0.62,
            Waveform::Triangle => 0.78,
            Waveform::Sine => 0.96,
        };
        let sustain_t = (t0 + duration_sec * 0.68).min(t0 + duration_sec - 0.03);
        let end_t = t0 + duration_sec + release_tail;
        ACTIVE_VOICE_ENDS.with(|ends| ends.borrow_mut().push(end_t + 0.06));

        _ = g.gain().set_value_at_time(0.0001, t0);
        _ = g
            .gain()
            .linear_ramp_to_value_at_time(velocity * 0.90, t0 + attack);
        _ = g
            .gain()
            .linear_ramp_to_value_at_time(velocity * sustain_k, sustain_t);
        _ = g.gain().exponential_ramp_to_value_at_time(0.0008, end_t);

        _ = src_main.connect_with_audio_node(&g);
        if let Some(chorus) = &src_chorus {
            _ = chorus.connect_with_audio_node(&g);
        }
        _ = g.connect_with_audio_node(voice_gain);
        _ = g.connect_with_audio_node(delay_send);
        _ = g.connect_with_audio_node(reverb_send);

        _ = src_main.start_with_when(t0);
        if let Some(chorus) = &src_chorus {
            _ = chorus.start_with_when(t0 + 0.0015);
        }
        _ = src_main.stop_with_when(end_t + 0.06);
        if let Some(chorus) = &src_chorus {
            _ = chorus.stop_with_when(end_t + 0.06);
        }
    }
}

// Create analyser and an appropriately sized buffer
pub fn create_analyser(
    audio_ctx: &web::AudioContext,
) -> (Option<web::AnalyserNode>, Rc<RefCell<Vec<f32>>>) {
    let analyser: Option<web::AnalyserNode> = web::AnalyserNode::new(audio_ctx).ok();
    let buf: Rc<RefCell<Vec<f32>>> = Rc::new(RefCell::new(Vec::new()));
    if let Some(a) = &analyser {
        a.set_fft_size(256);
        let bins = a.frequency_bin_count() as usize;
        buf.borrow_mut().resize(bins, 0.0);
    }
    (analyser, buf)
}

// Wire per-voice panners, gains and effect sends
pub fn wire_voices(
    audio_ctx: &web::AudioContext,
    initial_positions: &[Vec3],
    master_gain: &web::GainNode,
    delay_in: &web::GainNode,
    reverb_in: &web::GainNode,
) -> anyhow::Result<VoiceRouting> {
    let mut voice_gains: Vec<web::GainNode> = Vec::new();
    let mut voice_panners: Vec<web::PannerNode> = Vec::new();
    let mut delay_sends: Vec<web::GainNode> = Vec::new();
    let mut reverb_sends: Vec<web::GainNode> = Vec::new();

    for pos in initial_positions.iter() {
        let panner = named(web::PannerNode::new(audio_ctx), "PannerNode")?;
        panner.set_panning_model(web::PanningModelType::Hrtf);
        panner.set_distance_model(web::DistanceModelType::Inverse);
        panner.set_ref_distance(0.5);
        panner.set_max_distance(50.0);
        panner.position_x().set_value(pos.x as f32);
        panner.position_y().set_value(pos.y as f32);
        panner.position_z().set_value(pos.z as f32);

        let gain = create_gain(audio_ctx, 0.0, "Voice gain")?;
        _ = gain.connect_with_audio_node(&panner);
        _ = panner.connect_with_audio_node(master_gain);

        let d_send = create_gain(audio_ctx, 0.22, "Delay send")?;
        _ = d_send.connect_with_audio_node(delay_in);
        delay_sends.push(d_send);

        let r_send = create_gain(audio_ctx, 0.30, "Reverb send")?;
        _ = r_send.connect_with_audio_node(reverb_in);
        reverb_sends.push(r_send);

        voice_gains.push(gain);
        voice_panners.push(panner);
    }

    Ok(VoiceRouting {
        voice_gains,
        voice_panners,
        delay_sends,
        reverb_sends,
    })
}
