# Backlog

Ordered, honest next work, highest-impact first. No status history — see git for what changed. File hints in parentheses.

## P1 — correctness & confidence

- **Exercise render + audio in CI.** Two gaps: (1) the offline render (`src/offline.rs`) produces a deterministic WAV and `scripts/{render-offline,relay-test,display-test,cf-relay-test}.mjs` cover audio + the networked path, but none run in `npm run check`, so CI never executes them — wire them into the gate (and assert the offline WAV's RMS); (2) the browser smoke test runs `--disable-gpu`, so `GpuState` is `None` and the render + real FPS never run (the "60 fps" is hollow) — a software WebGPU backend (SwiftShader via `--enable-unsafe-swift-shader`, or lavapipe) would test it. Generative logic is covered by `tests/music_tests.rs`. (`.github/workflows/ci.yml`, `package.json`, `web-test.js`)

## P2 — performance & robustness

- **Adaptive render resolution.** Feed the per-frame `dt` into an EMA → render-scale controller: render the scene into a scaled HDR target and let the composite upscale, to hold 60 fps on weak GPUs. It only activates under load, so verify the scaled path with a forced low scale. (`src/render.rs`, `src/frame.rs`)
- **Dependency modernization.** Everything is ~a generation behind. **wgpu 24 → 29** is a real API migration (scoped): `DeviceDescriptor` gains `experimental_features` + `trace`; `request_adapter`/`request_device` return `Result` (and `request_device` drops its trace arg); the surface flow becomes `get_current_texture() -> CurrentSurfaceTexture` (replacing the `SurfaceError` path); `RenderPass`/`Pipeline` `multiview` → `multiview_mask: None`; `RenderPassColorAttachment` gains `depth_slice: None`; `PipelineLayoutDescriptor.bind_group_layouts` entries become `Option`-wrapped and `push_constant_ranges` → `immediate_size`. Then glam, rand, getrandom, and the wasm-bindgen / web-sys family. (`Cargo.toml`, `src/render.rs`, `src/render/*`)
- **Assert the cache headers.** A `Cache-Control` test on the worker's `?v=`-tagged assets vs the `env.js` / HTML entry — the immutable-vs-`no-cache` logic is the riskiest deploy surface and is untested. (`worker.js`)

## P3 — polish & housekeeping

- **GPU timestamp profiling.** A debug-flagged `QuerySet` per pass so the performance work above is measured, not guessed.
- **Post uniforms in one buffer with dynamic offsets** instead of 4 `write_buffer` calls/frame where only `blur_dir` changes. (`src/render.rs`, `src/render/post.rs`)
- **Persistent voice pool (low priority).** A polyphony cap (`MAX_POLYPHONY`) already bounds the worst case. A persistent-voice pool with voice-stealing would also remove per-note oscillator allocation, but at ~3 notes/sec the churn is negligible and a persistent oscillator changes the attack/timbre. (`src/audio.rs`)
- **Drop `rand` / `getrandom` for an inline seeded PRNG.** The engine only uses `StdRng::seed_from_u64` (pure determinism), so a ~10-line PCG removes ~6 transitive crates and the JS-entropy shim — but it changes the generated sequences, so retune by ear. (`src/core/music.rs`)
- **Lift the audio/music magic numbers into named constants** (filter cutoffs, gains, envelope shapes, gate/motif weights) — the *Patterns to Adopt* item in [ARCHITECTURE.md](ARCHITECTURE.md).
- **Configurable scheduling grid** (16th notes, triplets, dotted) instead of the fixed eighth-note grid; **per-voice filtering / configurable ADSR**.
- **Housekeeping.** A `twiggy` size budget after the wgpu upgrade.

## Constraints (intentional)

See [AGENTS.md](../AGENTS.md) for the project's intentional constraints — deliberate design rules, not gaps to fill here.
