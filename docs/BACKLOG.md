# Backlog

Ordered, honest next work, highest-impact first. No status history — see git for what changed. File hints in parentheses.

## P1 — correctness & confidence

- **Sample-accurate audio scheduling.** Give `NoteEvent` an absolute start time on the audio clock and schedule a ~100–150 ms window ahead, instead of firing every note in a frame at `current_time() + 5 ms`. Today a slow frame collapses several eighth-notes onto one instant, and tempo rides the `requestAnimationFrame` jitter. This is the canonical "two clocks" lookahead pattern; an `AudioWorklet` is the heavier alternative if sample-accuracy is still short. (`src/audio.rs`, `src/frame.rs`, `src/core/music.rs`)
- **Make CI actually exercise the engine.** `web-test.js` runs headless with `--disable-gpu`, so WebGPU is absent, the engine never starts, and every interaction/audio/perf assertion is silently skipped — "CI green" currently means "the overlay renders." Run a software WebGPU backend (SwiftShader / lavapipe) in CI, or keep moving deterministic logic into the host-testable `core` and unit-test it there.

## P2 — performance & robustness

- **Voice pooling + polyphony cap.** `trigger_one_shot` allocates two oscillators + a gain *per note* with no cap (a gesture fires 5 notes = 10 oscillators); pooled persistent voices with voice-stealing remove the per-note GC churn. (`src/audio.rs`)
- **Smooth audio params.** The frame loop `set_value()`s FX/voice params every frame (e.g. `delay_feedback` jumps straight into a recursive loop); use `set_target_at_time` (~30 ms) to stop zipper noise — the visual values are already smoothed, the audio ones aren't. (`src/frame.rs`)
- **Adaptive render resolution.** Feed the existing per-frame `dt` into an EMA → render-scale controller (the resize plumbing already exists) to hold 60 fps on weak GPUs. (`src/render.rs`, `src/frame.rs`)
- **Integer-hash shaders.** The waves Voronoi uses `fract(sin(dot) * 43758)` (~18 `sin`/pixel) — it bands and is slow on Mali/Adreno/Apple GPUs; a PCG-style integer hash is the same look without transcendentals. (`shaders/waves.wgsl`, `shaders/post.wgsl`)
- **Surface-loss recovery + texture-dimension clamp.** `render()` returns `SurfaceError` and the caller only logs it; handle `Lost`/`Outdated` by reconfiguring, and clamp the canvas backing size to the device `max_texture_dimension_2d` (the DPR cap covers the common case, not 5K+ displays). (`src/frame.rs`, `src/render.rs`, `src/dom.rs`)
- **Dependency modernization.** Everything is ~a generation behind: **wgpu 24 → 29** (real API churn in `render.rs` — do on a branch), plus glam 0.27→0.33, rand 0.8→0.10, getrandom 0.2→0.4, and the wasm-bindgen / web-sys 0.3.77→0.3.99 family.
- **Audio + cache-header test assertions.** An `AnalyserNode` RMS check (audio is currently verified only by the *absence* of a thrown error) and a `Cache-Control` assertion on the worker's `?v=` vs `env.js` responses (the cache logic is the riskiest deploy surface and is untested).

## P3 — polish & housekeeping

- **GPU timestamp profiling.** A debug-flagged `QuerySet` per pass so the performance work above is measured, not guessed.
- **Post uniforms in one buffer with dynamic offsets** instead of 4 `write_buffer` calls/frame where only `blur_dir` changes. (`src/render.rs`, `src/render/post.rs`)
- **Proper bloom bright-pass downsample.** `fs_bright` point-samples full-res HDR into the half-res buffer; a 2×2 box tap removes shimmer on thin bright features. (`shaders/post.wgsl`)
- **Drop `rand` / `getrandom` for an inline seeded PRNG.** The engine only uses `StdRng::seed_from_u64` (pure determinism), so a ~10-line PCG removes ~6 transitive crates and the dead JS-entropy shim — but it changes the generated sequences, so retune by ear. (`src/core/music.rs`)
- **Lift the audio/music magic numbers into named constants** (filter cutoffs, gains, envelope shapes, gate/motif weights) — the *Patterns to Adopt* item in [ARCHITECTURE.md](ARCHITECTURE.md).
- **Configurable scheduling grid** (16th notes, triplets, dotted) instead of the fixed eighth-note grid; **per-voice filtering / configurable ADSR**.
- **Housekeeping.** `power_preference: LowPower` for a long-running battery toy; one source of truth for wasm-pack (the npm devDep vs the CI `curl | sh`); a `twiggy` size budget after the wgpu upgrade.

## Constraints (intentional)

See [AGENTS.md](../AGENTS.md) for the project's intentional constraints — deliberate design rules, not gaps to fill here.
