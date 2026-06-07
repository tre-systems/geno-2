# Backlog

Ordered, honest next work, highest-impact first. No status history — see git for what changed. File hints in parentheses.

## P1 — correctness & confidence

- **Exercise the render path and audio output in CI.** The browser smoke test runs with `--disable-gpu`, so `navigator.gpu` exposes no adapter, `GpuState` falls back to `None`, and neither the WebGPU render nor real frame timing ever run — the reported "60 fps" is hollow (empty rAF callbacks). Bring up a software WebGPU backend (SwiftShader via `--enable-unsafe-swift-shader`, or lavapipe) so render and true FPS are tested, and add an `AnalyserNode` RMS assertion so CI verifies the graph actually *sounds* (audio is checked today only by the absence of a thrown error). The engine's generative logic is already covered by host tests in `tests/music_tests.rs`. (`web-test.js`, `.github/workflows/ci.yml`)

## P2 — performance & robustness

- **Voice pooling + polyphony cap.** `trigger_one_shot` allocates two oscillators + a gain *per note* with no cap (a gesture fires 5 notes = 10 oscillators); pooled persistent voices with voice-stealing remove the per-note GC churn. (`src/audio.rs`)
- **Smooth audio params.** The frame loop `set_value()`s FX/voice params every frame (e.g. `delay_feedback` jumps straight into a recursive loop); use `set_target_at_time` (~30 ms) to stop zipper noise — the visual values are already smoothed, the audio ones aren't. (`src/frame.rs`)
- **Adaptive render resolution.** Feed the existing per-frame `dt` into an EMA → render-scale controller (the resize plumbing already exists) to hold 60 fps on weak GPUs. (`src/render.rs`, `src/frame.rs`)
- **Integer-hash shaders.** The waves Voronoi uses `fract(sin(dot) * 43758)` (~18 `sin`/pixel) — it bands and is slow on Mali/Adreno/Apple GPUs; a PCG-style integer hash is the same look without transcendentals. (`shaders/waves.wgsl`, `shaders/post.wgsl`)
- **Surface-loss recovery + texture-dimension clamp.** `render()` returns `SurfaceError` and the caller only logs it; handle `Lost`/`Outdated` by reconfiguring, and clamp the canvas backing size to the device `max_texture_dimension_2d` (the DPR cap covers the common case, not 5K+ displays). (`src/frame.rs`, `src/render.rs`, `src/dom.rs`)
- **Dependency modernization.** Everything is ~a generation behind: **wgpu 24 → 29** (real API churn in `render.rs` — do on a branch), plus glam 0.27→0.33, rand 0.8→0.10, getrandom 0.2→0.4, and the wasm-bindgen / web-sys 0.3.77→0.3.99 family.
- **Assert the cache headers.** A `Cache-Control` test on the worker's `?v=`-tagged assets vs the `env.js` / HTML entry — the immutable-vs-`no-cache` logic is the riskiest deploy surface and is untested. (`worker.js`)

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
