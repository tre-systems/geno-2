# Backlog

Ordered, honest next work. No status history — see git for what changed.

## Audio

- Optional `AudioWorklet` path for sample-accurate scheduling (currently main-thread + WebAudio lookahead).
- Cap polyphony / pool oscillators — `trigger_one_shot` creates a fresh oscillator pair per note; audit node lifetimes under sustained play.
- Per-voice filtering and configurable ADSR.
- Lift the FX design's inline constants (filter cutoffs, gains, envelope shapes) into named constants — see *Patterns to Adopt* in [ARCHITECTURE.md](ARCHITECTURE.md).

## Engine

- Strong newtypes (`MidiNote`, `Frequency`, `Cents`, `BPM`) to prevent unit mix-ups at call sites.
- Configurable scheduling grid (16th notes, triplets, dotted) instead of the fixed eighth-note grid.

## Visuals

- Profile for steady 60 FPS on mid-range GPUs; reuse GPU buffers where the per-frame uniform writes allow.

## Tooling

- Modernize dependencies: `getrandom` 0.2 → 0.3 (entangled with `rand` 0.8's transitive 0.2; needs the `wasm_js` backend); periodic `npm run deps` review.
- Extend `web-test.js` to change tempo and assert the hint overlay reflects the new BPM.

## Constraints (intentional)

- WebGPU only — no WebGL fallback.
- Keep `src/core` host-testable: no `web-sys` there, so the generative engine and gesture geometry stay unit-testable with `cargo test`.
