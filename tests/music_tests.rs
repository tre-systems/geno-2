use app_web::core::*;
use std::time::Duration;

fn test_configs() -> Vec<VoiceConfig> {
    vec![
        VoiceConfig {
            waveform: Waveform::Sine,
            base_position: glam::Vec3::new(-1.0, 0.0, 0.0),
            trigger_probability: 0.4,
            octave_offset: -1,
            base_duration: 0.4,
        },
        VoiceConfig {
            waveform: Waveform::Saw,
            base_position: glam::Vec3::new(1.0, 0.0, 0.0),
            trigger_probability: 0.6,
            octave_offset: 0,
            base_duration: 0.25,
        },
        VoiceConfig {
            waveform: Waveform::Triangle,
            base_position: glam::Vec3::new(0.0, 0.0, -1.0),
            trigger_probability: 0.3,
            octave_offset: 1,
            base_duration: 0.6,
        },
    ]
}

fn engine_with_seed(seed: u64) -> MusicEngine {
    MusicEngine::new(test_configs(), EngineParams::default(), seed)
}

fn make_engine() -> MusicEngine {
    engine_with_seed(42)
}

#[test]
fn midi_to_hz_matches_a4_and_octave() {
    let a4 = midi_to_hz(69.0);
    assert!((a4 - 440.0).abs() < 1e-4);
    let a5 = midi_to_hz(81.0);
    assert!((a5 - 880.0).abs() < 1e-3);
    assert!((a5 / a4 - 2.0).abs() < 1e-4);
}

#[test]
fn midi_to_hz_is_monotonic_over_range() {
    let mut prev = midi_to_hz(20.0);
    for m in 21..=100 {
        let f = midi_to_hz(m as f32);
        assert!(f > prev, "frequency not increasing at midi {m}");
        prev = f;
    }
}

#[test]
fn engine_tick_emits_some_events_over_time() {
    let mut engine = make_engine();
    let mut events = Vec::new();
    let seconds_per_beat = 60.0 / engine.params.bpm.get() as f64;
    for _ in 0..200 {
        engine.tick(Duration::from_secs_f64(seconds_per_beat / 2.0), &mut events);
    }
    assert!(!events.is_empty(), "expected some scheduled events");
    for ev in &events {
        assert!(ev.voice_index < engine.voices.len());
        assert!(ev.frequency_hz.hz() > 0.0);
        assert!(ev.velocity >= 0.0 && ev.velocity <= 1.0);
        assert!(ev.duration_sec > 0.0);
    }
}

#[test]
fn tick_caps_catch_up_after_a_long_stall() {
    let mut engine = make_engine();
    let mut events = Vec::new();
    // A 10s stall is hundreds of grid steps; the accumulator clamp must bound it
    // to at most MAX_CATCHUP_STEPS (4) grid steps across the 3 voices.
    engine.tick(Duration::from_secs(10), &mut events);
    assert!(
        events.len() <= 12,
        "catch-up not bounded after a long stall: {} events",
        events.len()
    );
}

#[test]
fn midi_to_hz_octave_doubling_property() {
    // Adding 12 semitones (one octave) doubles the frequency.
    for midi in 20..100 {
        let freq1 = midi_to_hz(midi as f32);
        let freq2 = midi_to_hz((midi + 12) as f32);
        let ratio = freq2 / freq1;
        assert!(
            (ratio - 2.0).abs() < 1e-6,
            "Octave doubling failed for MIDI {midi}: {freq1} -> {freq2} (ratio: {ratio})"
        );
    }
}

#[test]
fn midi_to_hz_semitone_ratio_property() {
    // Each semitone multiplies frequency by 2^(1/12) ≈ 1.059463.
    let semitone_ratio = 2.0_f32.powf(1.0 / 12.0);

    for midi in 30..90 {
        let freq1 = midi_to_hz(midi as f32);
        let freq2 = midi_to_hz((midi + 1) as f32);
        let actual_ratio = freq2 / freq1;
        assert!(
            (actual_ratio - semitone_ratio).abs() < 1e-6,
            "Semitone ratio failed for MIDI {midi} -> {}: expected {semitone_ratio}, got {actual_ratio}",
            midi + 1
        );
    }
}

#[test]
fn midi_to_hz_fractional_values() {
    // Fractional MIDI (microtonal): 60.5 sits halfway between C4 and C#4
    // in log-frequency space.
    let midi_60 = midi_to_hz(60.0);
    let midi_60_5 = midi_to_hz(60.5);
    let midi_61 = midi_to_hz(61.0);

    let log_60 = midi_60.ln();
    let log_60_5 = midi_60_5.ln();
    let log_61 = midi_61.ln();

    let expected_log_60_5 = (log_60 + log_61) / 2.0;
    assert!(
        (log_60_5 - expected_log_60_5).abs() < 1e-6,
        "Fractional MIDI value 60.5 should be logarithmic midpoint between 60 and 61"
    );
}

#[test]
fn midi_to_hz_extreme_values() {
    let very_low = midi_to_hz(0.0); // C-1, ~8.18 Hz
    let very_high = midi_to_hz(127.0); // G9, ~12543 Hz

    assert!(
        very_low > 0.0 && very_low < 20.0,
        "MIDI 0 should be a sub-bass frequency"
    );
    assert!(
        very_high > 10000.0 && very_high < 15000.0,
        "MIDI 127 should be a very high frequency"
    );
    assert!(very_low.is_finite() && very_high.is_finite());
}

#[test]
fn midi_to_hz_negative_values() {
    // Negative MIDI yields sub-audio frequencies; -12 is one octave below 0.
    let neg_midi = midi_to_hz(-12.0);
    let zero_midi = midi_to_hz(0.0);

    let ratio = zero_midi / neg_midi;
    assert!(
        (ratio - 2.0).abs() < 1e-6,
        "MIDI -12 should be exactly one octave below MIDI 0"
    );
}

#[test]
fn midi_to_hz_with_detune_accuracy() {
    // 50¢ sits halfway between C4 and C#4, i.e. their geometric-mean frequency.
    let midi_60 = midi_to_hz(60.0);
    let midi_61 = midi_to_hz(61.0);
    let midi_60_50cents = midi_to_hz_with_detune(60.0, 50.0);

    let expected_ratio = (midi_61 / midi_60).sqrt();
    let actual_ratio = midi_60_50cents / midi_60;
    assert!(
        (actual_ratio - expected_ratio).abs() < 1e-6,
        "50¢ detune should produce geometric mean frequency ratio"
    );
}

#[test]
fn midi_to_hz_with_detune_bounds() {
    // Detune clamps to ±200¢ (±2 semitones) around C4.
    let extreme_high = midi_to_hz_with_detune(60.0, 500.0);
    let extreme_low = midi_to_hz_with_detune(60.0, -500.0);

    assert!(
        (extreme_high - midi_to_hz(62.0)).abs() < 1e-6,
        "high detune should clamp to +200¢ (2 semitones up)"
    );
    assert!(
        (extreme_low - midi_to_hz(58.0)).abs() < 1e-6,
        "low detune should clamp to -200¢ (2 semitones down)"
    );
}

#[test]
fn engine_params_detune_default() {
    assert_eq!(EngineParams::default().detune_cents.get(), 0.0);
}

#[test]
fn engine_detune_methods() {
    let mut engine = make_engine();

    engine.set_detune_cents(Cents::new(50.0));
    assert_eq!(engine.params.detune_cents.get(), 50.0);

    // Cents::new clamps at construction, so set/adjust inherit the ±200¢ bound.
    engine.set_detune_cents(Cents::new(300.0));
    assert_eq!(engine.params.detune_cents.get(), 200.0, "clamps to +200¢");

    engine.set_detune_cents(Cents::new(-300.0));
    assert_eq!(engine.params.detune_cents.get(), -200.0, "clamps to -200¢");

    engine.adjust_detune_cents(Cents::new(25.0));
    assert_eq!(
        engine.params.detune_cents.get(),
        -175.0,
        "adjust is additive"
    );

    engine.reset_detune();
    assert_eq!(engine.params.detune_cents.get(), 0.0);
}

#[test]
fn engine_bpm_methods_guard_invalid_values() {
    let mut engine = make_engine();

    engine.set_bpm(Bpm::new(180.0));
    assert_eq!(engine.params.bpm.get(), 180.0);

    engine.set_bpm(Bpm::new(-20.0));
    assert_eq!(engine.params.bpm.get(), 1.0, "clamps to lower bound");

    engine.set_bpm(Bpm::new(1000.0));
    assert_eq!(engine.params.bpm.get(), 400.0, "clamps to upper bound");

    engine.set_bpm(Bpm::new(f32::NAN));
    assert_eq!(
        engine.params.bpm.get(),
        1.0,
        "non-finite collapses to minimum"
    );
}

#[test]
fn newtypes_clamp_out_of_range_values() {
    // The Bpm/Cents newtypes make an invalid engine tempo/detune unrepresentable.
    assert_eq!(Bpm::new(-10.0).get(), Bpm::MIN);
    assert_eq!(Bpm::new(f32::NAN).get(), Bpm::MIN);
    assert_eq!(Bpm::new(10_000.0).get(), Bpm::MAX);
    assert_eq!(Cents::new(500.0).get(), Cents::LIMIT);
    assert_eq!(Cents::new(-500.0).get(), -Cents::LIMIT);
}

#[test]
fn engine_schedule_with_detune() {
    // Deterministic: 1 voice, prob=1.0, scale=[0], root=C4
    let configs = vec![VoiceConfig {
        waveform: Waveform::Sine,
        base_position: glam::Vec3::new(0.0, 0.0, 0.0),
        trigger_probability: 1.0,
        octave_offset: 0,
        base_duration: 0.25,
    }];
    let params = EngineParams {
        scale: &[0.0],
        root_midi: 60,
        ..EngineParams::default()
    };
    let mut engine = MusicEngine::new(configs, params, 12345);

    engine.set_detune_cents(Cents::new(50.0));
    let mut events = Vec::new();
    let seconds_per_beat = 60.0 / engine.params.bpm.get() as f64;
    engine.tick(Duration::from_secs_f64(seconds_per_beat / 2.0), &mut events);

    assert!(
        !events.is_empty(),
        "expected at least one event with probability=1.0"
    );

    let expected = midi_to_hz_with_detune(60.0, engine.params.detune_cents.get());
    for ev in &events {
        assert!(
            (ev.frequency_hz.hz() - expected).abs() < 1e-6,
            "scheduled freq does not include detune: got {:.6}, expected {:.6}",
            ev.frequency_hz.hz(),
            expected
        );
    }
}

#[test]
fn detune_round_trip_accuracy() {
    // Detune is added to the MIDI note before conversion, so N¢ shifts the
    // note by N/100 semitones (e.g. -100¢ → MIDI 59, +100¢ → MIDI 61).
    let midi_60 = 60.0; // C4
    for detune in [-100.0, -50.0, -25.0, 0.0, 25.0, 50.0, 100.0] {
        let detuned_freq = midi_to_hz_with_detune(midi_60, detune);
        let adjusted_midi = midi_60 + detune / 100.0;
        let expected_freq = midi_to_hz(adjusted_midi);
        assert!(
            (detuned_freq - expected_freq).abs() < 1e-6,
            "detune of {detune}¢ should produce frequency for MIDI {adjusted_midi:.1}"
        );
    }
}

#[test]
fn step_duration_matches_tempo() {
    let mut e = make_engine();
    e.set_bpm(Bpm::new(120.0));
    // Eighth-note at 120 bpm = (60/120)/2 = 0.25 s.
    assert!((e.step_duration() - 0.25).abs() < 1e-9);
    e.set_bpm(Bpm::new(84.0));
    assert!((e.step_duration() - (60.0 / 84.0 / 2.0)).abs() < 1e-9);
}

#[test]
fn generate_step_stamps_start_time() {
    let mut e = make_engine();
    let step = e.step_duration();
    // The default voices fire probabilistically, so advance until a step emits.
    let mut t = 7.5;
    let mut events = Vec::new();
    loop {
        events.clear();
        e.generate_step(t, &mut events);
        if !events.is_empty() {
            break;
        }
        t += step;
    }
    for ev in &events {
        assert_eq!(ev.start_time, t, "event not stamped with the step time");
    }
}

#[test]
fn lookahead_generates_ordered_on_grid_events() {
    // Mirror the frame loop's lookahead: walk the grid and assert events land in
    // time order, on the step grid.
    let mut e = make_engine();
    let step = e.step_duration();
    let mut all = Vec::new();
    let mut t = 0.0;
    for _ in 0..96 {
        e.generate_step(t, &mut all);
        t += step;
    }
    assert!(!all.is_empty(), "engine produced no notes over 96 steps");
    for pair in all.windows(2) {
        assert!(
            pair[1].start_time >= pair[0].start_time,
            "events not in non-decreasing time order"
        );
    }
    for ev in &all {
        let k = (ev.start_time / step).round();
        assert!(
            (ev.start_time - k * step).abs() < 1e-6,
            "start_time {} is off the step grid",
            ev.start_time
        );
    }
}

#[test]
fn engine_stream_is_deterministic_per_seed() {
    // Same seed -> identical note stream. This determinism is what makes the
    // generative engine unit-testable, so CI verifies the music, not just boot.
    let render = |seed: u64| {
        let mut e = engine_with_seed(seed);
        let step = e.step_duration();
        let mut evs = Vec::new();
        let mut t = 0.0;
        for _ in 0..240 {
            e.generate_step(t, &mut evs);
            t += step;
        }
        evs
    };
    let a = render(42);
    let b = render(42);
    assert_eq!(a.len(), b.len(), "same seed changed the note count");
    for (x, y) in a.iter().zip(b.iter()) {
        assert_eq!(x.voice_index, y.voice_index);
        assert_eq!(x.frequency_hz.hz().to_bits(), y.frequency_hz.hz().to_bits());
        assert_eq!(x.velocity.to_bits(), y.velocity.to_bits());
        assert_eq!(x.duration_sec.to_bits(), y.duration_sec.to_bits());
    }
    let c = render(7);
    let differs = a.len() != c.len()
        || a.iter()
            .zip(c.iter())
            .any(|(x, y)| x.frequency_hz.hz().to_bits() != y.frequency_hz.hz().to_bits());
    assert!(differs, "different seeds produced an identical stream");
}

#[test]
fn engine_produces_music_at_a_sane_density() {
    // Over ~10 s of musical time the engine should emit a healthy note stream —
    // neither silence nor a runaway flood. This is the liveness assertion the
    // headless browser test cannot make.
    let mut e = make_engine();
    let step = e.step_duration();
    let steps = (10.0 / step).ceil() as usize;
    let mut evs = Vec::new();
    let mut t = 0.0;
    for _ in 0..steps {
        e.generate_step(t, &mut evs);
        t += step;
    }
    let per_sec = evs.len() as f64 / 10.0;
    assert!(
        (1.0..60.0).contains(&per_sec),
        "unexpected note density: {per_sec}/s"
    );
}

/// Pin the exact generated sequence (seed 42) so accidental engine changes are
/// caught. Update the pinned pair only when intentionally changing the output.
#[test]
fn music_fingerprint_is_stable() {
    let mut e = engine_with_seed(42);
    let step = e.step_duration();
    let mut evs = Vec::new();
    let mut t = 0.0;
    for _ in 0..1000 {
        e.generate_step(t, &mut evs);
        t += step;
    }
    let mut h: u64 = 0xcbf29ce484222325;
    for ev in &evs {
        for x in [
            ev.voice_index as u32,
            ev.frequency_hz.hz().to_bits(),
            ev.velocity.to_bits(),
            ev.duration_sec.to_bits(),
        ] {
            h ^= x as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    assert_eq!(
        (evs.len(), h),
        (935, 10142961523132047772),
        "music fingerprint"
    );
}
