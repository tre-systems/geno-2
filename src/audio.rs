use crate::core::Waveform;
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

fn create_gain(
    audio_ctx: &web::AudioContext,
    value: f32,
    label: &str,
) -> Result<web::GainNode, ()> {
    match web::GainNode::new(audio_ctx) {
        Ok(g) => {
            g.gain().set_value(value);
            Ok(g)
        }
        Err(e) => {
            log::error!("{} GainNode error: {:?}", label, e);
            Err(())
        }
    }
}

pub fn build_fx_buses(audio_ctx: &web::AudioContext) -> Result<FxBuses, ()> {
    // Master gain
    let master_gain = create_gain(audio_ctx, 0.46, "Master")?;

    // Subtle master saturation (arctan) with wet/dry mix
    let sat_pre = create_gain(audio_ctx, 0.54, "sat pre")?;
    #[allow(deprecated)]
    let saturator = web::WaveShaperNode::new(audio_ctx)
        .map_err(|e| {
            log::error!("WaveShaperNode error: {:?}", e);
        })
        .map_err(|_| ())?;
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
    let master_hp = web::BiquadFilterNode::new(audio_ctx)
        .map_err(|e| {
            log::error!("Master highpass filter error: {:?}", e);
        })
        .map_err(|_| ())?;
    master_hp.set_type(web::BiquadFilterType::Highpass);
    master_hp.frequency().set_value(95.0);
    master_hp.q().set_value(0.70);

    let master_lp = web::BiquadFilterNode::new(audio_ctx)
        .map_err(|e| {
            log::error!("Master lowpass filter error: {:?}", e);
        })
        .map_err(|_| ())?;
    master_lp.set_type(web::BiquadFilterType::Lowpass);
    master_lp.frequency().set_value(4600.0);
    master_lp.q().set_value(0.45);

    // Gentle master compression + makeup keeps baseline louder while taming peaks.
    let master_comp = web::DynamicsCompressorNode::new(audio_ctx)
        .map_err(|e| {
            log::error!("DynamicsCompressorNode error: {:?}", e);
        })
        .map_err(|_| ())?;
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
    let reverb = web::ConvolverNode::new(audio_ctx)
        .map_err(|e| {
            log::error!("ConvolverNode error: {:?}", e);
        })
        .map_err(|_| ())?;
    reverb.set_normalize(true);
    // Create a short bright impulse response procedurally
    {
        let sr = audio_ctx.sample_rate();
        let seconds = 3.8_f32;
        let len = (sr as f32 * seconds) as u32;
        if let Ok(ir) = audio_ctx.create_buffer(2, len, sr) {
            // simple xorshift32 for deterministic noise
            let mut seed_l: u32 = 0x1234ABCD;
            let mut seed_r: u32 = 0x7890FEDC;
            for ch in 0..2 {
                let mut buf: Vec<f32> = vec![0.0; len as usize];
                let mut t = 0.0_f32;
                let dt = 1.0_f32 / sr as f32;
                for i in 0..len as usize {
                    let s = if ch == 0 { &mut seed_l } else { &mut seed_r };
                    let mut x = *s;
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    *s = x;
                    let n = ((x as f32 / std::u32::MAX as f32) * 2.0 - 1.0) as f32;
                    // Faster decay with soft early emphasis
                    let decay = (-t / 1.60).exp();
                    let early = (1.0 - (t / 0.36)).clamp(0.0, 1.0);
                    let v = n * decay * (0.38 + 0.62 * early);
                    buf[i] = v;
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
    let delay = audio_ctx
        .create_delay_with_max_delay_time(3.0)
        .map_err(|e| {
            log::error!("DelayNode error: {:?}", e);
        })
        .map_err(|_| ())?;
    delay.delay_time().set_value(0.38);
    let delay_tone = web::BiquadFilterNode::new(audio_ctx)
        .map_err(|e| {
            log::error!("BiquadFilterNode error: {:?}", e);
        })
        .map_err(|_| ())?;
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

// Fire a simple one-shot oscillator routed through a voice's gain and sends
pub fn trigger_one_shot(
    audio_ctx: &web::AudioContext,
    waveform: Waveform,
    frequency_hz: f32,
    velocity: f32,
    duration_sec: f64,
    voice_gain: &web::GainNode,
    delay_send: &web::GainNode,
    reverb_send: &web::GainNode,
) {
    let src_main = match web::OscillatorNode::new(audio_ctx) {
        Ok(s) => s,
        Err(_) => return,
    };
    let src_chorus = web::OscillatorNode::new(audio_ctx).ok();

    match waveform {
        Waveform::Sine => src_main.set_type(web::OscillatorType::Sine),
        Waveform::Saw => src_main.set_type(web::OscillatorType::Sawtooth),
        Waveform::Triangle => src_main.set_type(web::OscillatorType::Triangle),
    }
    if let Some(chorus) = &src_chorus {
        match waveform {
            Waveform::Sine => chorus.set_type(web::OscillatorType::Sine),
            Waveform::Saw => chorus.set_type(web::OscillatorType::Sawtooth),
            Waveform::Triangle => chorus.set_type(web::OscillatorType::Triangle),
        }
    }

    if let Ok(g) = web::GainNode::new(audio_ctx) {
        g.gain().set_value(0.0);
        let now = audio_ctx.current_time();
        let t0 = now + 0.005;
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
    if let Some(a) = &analyser {
        a.set_fft_size(256);
    }
    let buf: Rc<RefCell<Vec<f32>>> = Rc::new(RefCell::new(Vec::new()));
    if let Some(a) = &analyser {
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
) -> Result<VoiceRouting, ()> {
    let mut voice_gains: Vec<web::GainNode> = Vec::new();
    let mut voice_panners: Vec<web::PannerNode> = Vec::new();
    let mut delay_sends_vec: Vec<web::GainNode> = Vec::new();
    let mut reverb_sends_vec: Vec<web::GainNode> = Vec::new();

    for pos in initial_positions.iter() {
        let panner = web::PannerNode::new(audio_ctx)
            .map_err(|e| {
                log::error!("PannerNode error: {:?}", e);
            })
            .map_err(|_| ())?;
        panner.set_panning_model(web::PanningModelType::Hrtf);
        panner.set_distance_model(web::DistanceModelType::Inverse);
        panner.set_ref_distance(0.5);
        panner.set_max_distance(50.0);
        panner.position_x().set_value(pos.x as f32);
        panner.position_y().set_value(pos.y as f32);
        panner.position_z().set_value(pos.z as f32);

        let gain = create_gain(audio_ctx, 0.0, "Voice gain").map_err(|_| ())?;
        _ = gain.connect_with_audio_node(&panner);
        _ = panner.connect_with_audio_node(master_gain);

        let d_send = create_gain(audio_ctx, 0.22, "Delay send").map_err(|_| ())?;
        _ = d_send.connect_with_audio_node(delay_in);
        delay_sends_vec.push(d_send);

        let r_send = create_gain(audio_ctx, 0.30, "Reverb send").map_err(|_| ())?;
        _ = r_send.connect_with_audio_node(reverb_in);
        reverb_sends_vec.push(r_send);

        voice_gains.push(gain);
        voice_panners.push(panner);
    }

    Ok(VoiceRouting {
        voice_gains,
        voice_panners,
        delay_sends: delay_sends_vec,
        reverb_sends: reverb_sends_vec,
    })
}

// Public create_gain used across modules
// (no-op) use the Result-returning `create_gain` defined above for internal wiring
