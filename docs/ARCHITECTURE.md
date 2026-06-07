# Geno-2 — Architecture

> Scope: how the code is organized and how one frame of audio + video is produced. Geno-2 is a single Rust crate (`app-web`) compiled to WebAssembly: a generative music engine, a WebAudio FX graph, and a fullscreen WebGPU shader, wired together by one `requestAnimationFrame` loop.

![System overview](diagrams/system-overview.png)

## Stack

| Layer        | Choice                                  | Notes                                                            |
| ------------ | --------------------------------------- | --------------------------------------------------------------- |
| Language     | Rust (edition 2021)                     | 19 modules + 2 WGSL shaders, one crate (`app-web`)       |
| GPU          | `wgpu` 24 (WebGPU)                      | Fullscreen waves pass + bloom/composite; no WebGL fallback      |
| Shaders      | WGSL                                    | `waves.wgsl` (scene), `post.wgsl` (bright-pass, blur, composite) |
| Audio        | WebAudio via `web-sys`                  | Procedural synthesis + FX graph; no audio samples shipped       |
| Math / POD   | `glam`, `bytemuck`                      | Vector math; `#[repr(C)]` uniforms cast straight into buffers   |
| RNG          | `rand` (`StdRng`) + `getrandom` (`js`)  | Per-voice seeded generators                                     |
| WASM         | `wasm-bindgen` + `web-sys`              | Canvas, pointer/keyboard events, `requestAnimationFrame`        |
| Build        | `wasm-pack` (`--target web`)            | Emits `pkg/app_web.js` + `app_web_bg.wasm`, copied into `dist/` |
| Host         | Cloudflare Workers (`wrangler`)         | `worker.js` serves `dist/` with cache headers                   |

The toolchain is plain `stable` (`rust-toolchain.toml`) — no nightly, no threads.

## Repo Layout

```
src/
├── lib.rs            # Crate root: module wiring; re-exports `start` (the only WASM export)
├── wasm_app.rs       # Composition root: build AudioContext + engine + FX + voices + GPU, wire events, start the loop
├── frame.rs          # FrameContext + per-frame update (schedule → swirl → FX → spatialize → render); the rAF driver
├── audio.rs          # WebAudio graph: master FX buses (tone, saturation, compressor, reverb, delay), per-voice strips, note trigger
├── input.rs          # Input state (mouse, drag, multitouch) + pointer→pixel/uv helpers + multitouch geometry
├── overlay.rs        # Start overlay + hint overlay (BPM · detune · scale)
├── dom.rs            # DOM helpers: window/document, click listeners, DPR-aware canvas sizing
├── constants.rs      # Tuning constants: swirl spring, FX mapping, per-voice sends, camera Z, bloom
├── render.rs         # GpuState: WebGPU init + the per-frame render (waves → bloom → composite)
├── render/
│   ├── targets.rs    # Offscreen HDR scene target + two half-res bloom buffers
│   ├── waves.rs      # Waves fullscreen pass: pipeline, bind group, WavesUniforms (3 voices, swirl, ripple)
│   ├── post.rs       # Post pipelines (bright-pass, blur, composite), uniforms, blit helper, bind groups
│   └── helpers.rs    # Texture-creation helpers
├── core/
│   ├── mod.rs        # Re-exports `music`; embeds WAVES_WGSL / POST_WGSL via include_str!
│   └── music.rs      # MusicEngine: the generative scheduler; scales/modes/tunings; midi_to_hz(+detune)
└── events/
    ├── mod.rs        # Event-wiring re-exports
    ├── keyboard.rs   # keydown: root/mode/preset/tempo/detune/volume/mute/fullscreen
    ├── keymap.rs     # key→root MIDI and digit→mode/tuning tables (host-testable)
    └── pointer.rs    # Pointer + multitouch gestures: flare / carve / carve-drop + 2–5-finger
shaders/
├── waves.wgsl        # Fullscreen scene: swirl displacement, per-voice influence, ripple propagation
└── post.wgsl         # Bright-pass, separable blur, ACES tonemap composite, vignette, grain
index.html            # Canvas + start/hint overlays + WebGPU/Audio error UI
worker.js             # Cloudflare asset worker: immutable cache for versioned assets, no-cache for the entry
scripts/gen-env.js    # Stamps pkg/env.js with the build's git short SHA
web-test.js           # Puppeteer smoke test (boot, WebGPU, keyboard, FPS)
```

`core`, `input`, and `events::keymap` compile on the host (not `#[cfg(target_arch = "wasm32")]`), so the generative engine, multitouch geometry, and key tables are unit-tested with plain `cargo test`. Everything that touches `web-sys` is gated to the wasm target.

## Patterns

Most files are an instance of one of a handful of recurring idioms; naming them once makes the rest predictable.

**Host-testable core, browser-gated shell.** The musically and geometrically interesting code (`core::music`, `input`'s `MultiTouchState` geometry, `events::keymap`) is pure Rust with no `web-sys`, so it runs under `cargo test` with no browser. `MusicEngine` never imports a web type — it emits `NoteEvent`s that the wasm layer renders to WebAudio.

**Single composition root + self-scheduling loop.** `wasm_app::init` (one `#[wasm_bindgen(start)]` export, via `lib.rs`) builds every subsystem — AudioContext, `MusicEngine`, the FX graph, voice routing, `GpuState`, and the event handlers — then hands a `FrameContext` to `frame::start_loop`, which arms a `requestAnimationFrame` callback that re-arms itself each frame. Shared mutable state is `Rc<RefCell<…>>` (the single-threaded-WASM idiom; no `static mut`).

**Deferred input: accumulate, then drain.** DOM pointer/keyboard handlers write into shared `MouseState` / `DragState` / `MultiTouchState`. The frame loop reads that state once per frame, decaying gesture energy/flash/spin exponentially. Edge events (taps, carve ripples) are pushed onto a one-slot `queued_ripple_uv` and drained by the renderer, decoupling bursty event delivery from the synchronous frame.

**Procedural everything (no assets).** The reverb impulse response is generated at runtime (seeded xorshift noise × an exponential decay envelope), the saturation curve is an arctan lookup table, and each voice's timbre is an oscillator plus a slightly detuned chorus oscillator. Nothing but code and shaders ships.

**POD uniforms mirrored Rust ↔ WGSL, guarded at compile time.** `WavesUniforms`, `VoicePacked`, and `PostUniforms` are `#[repr(C)]` + `bytemuck::Pod`, byte-compatible with their WGSL `struct` counterparts, so they `bytes_of` straight into uniform buffers with no serialization. The Rust and WGSL definitions are one contract and change together — a `const _: () = assert!(size_of::<…>() == N)` next to each struct fails the build if a field is added or reordered without updating the matching shader.

**Typed domain values.** Tempo, detune, MIDI pitch, and frequency are newtypes (`Bpm`, `Cents`, `MidiNote`, `Frequency` in `core/music.rs`), not bare `f32`s. `Bpm` and `Cents` validate at construction (`Bpm::new` clamps to `[1, 400]` and sanitizes non-finite; `Cents::new` clamps to ±200), so an out-of-range tempo or detune is unrepresentable and the engine setters carry no runtime guard. `MidiNote::to_freq` is the single typed path from a pitch to the `Frequency` that flows through `NoteEvent` into the audio layer, so a MIDI number can't be passed where Hz is expected.

**Fullscreen-triangle passes.** The waves pass and every post step are a single oversized triangle (`draw(0..3, 0..1)`, no vertex buffer) — the standard cheaper-than-a-quad fullscreen idiom.

**Compile-time-embedded shaders.** WGSL is pulled in with `include_str!` (`core::WAVES_WGSL` / `POST_WGSL`), so the shaders are compiled into the WASM — no runtime fetch, no separate asset to deploy.

**Deliberate `'static` at the browser boundary.** Objects the browser holds past setup are given a `'static` lifetime three ways, by intent: event-listener closures are `.forget()`-ed (dropping one would silently unregister the listener); the `requestAnimationFrame` callback is held in an `Rc<RefCell<Option<Closure>>>` that the loop re-references each frame, so it stays alive without leaking a fresh closure per frame; and the canvas handed to the WebGPU surface is `Box::leak`-ed once (`frame::init_gpu`) to satisfy the surface's `'static` bound. Each is a conscious one-time leak at the JS↔WASM seam, not an accident.

**Display-synced canvas sizing.** A `resize` listener keeps the canvas backing buffer at its displayed size × `devicePixelRatio` (capped at 2×, `dom::sync_canvas_backing_size`); `GpuState::resize_if_needed` reconfigures the surface and rebuilds the offscreen targets to match.

**Labeled GPU resources.** Every buffer, pipeline, bind group, pass, and texture carries a `label: Some(...)` (the `render/` modules and `render.rs`), so each is identifiable in browser GPU debuggers and validation messages.

**FX graph built once, parameters written per frame.** `audio::build_fx_buses` and `wire_voices` construct the entire WebAudio node graph a single time at startup; the frame loop never adds or removes nodes, it only writes `AudioParam` values (wet levels, sends, panner positions, saturation drive). Per-note oscillators are the one exception — created and stopped per `NoteEvent` in `trigger_one_shot`.

**Errors bubble as `anyhow::Result`; `JsValue` only at the boundary.** The engine and setup code (`audio`, `render`, `wasm_app::init`) return `anyhow::Result`, attaching context as failures propagate; `JsValue` is confined to the `#[wasm_bindgen]` `start` surface and the DOM error overlay. A failed node-graph build surfaces its real cause in the console rather than a bare unit error.

**Tuning constants for the visual/interaction layer.** The smoothing time-constants, the swirl spring, the FX-mapping weights, and the per-voice send curves live as named constants in `constants.rs` rather than scattered literals. This holds for the visual/interaction layer; the audio FX design (`audio.rs`) and the generative engine (`core/music.rs`) still tune with inline literals — see *Patterns to adopt*.

## Patterns to Adopt

Patterns the codebase would benefit from but does not yet apply consistently:

- **Extend the constants pattern to audio.** `audio.rs` and `core/music.rs` carry the bulk of the project's magic numbers (filter cutoffs, gains, envelope shapes, gate/motif weights) inline. Lifting the audio FX design and the generative tuning into named constants — the way `constants.rs` already does for the visuals — would make the sound design legible and tweakable in one place.

## How a Frame Is Produced

![Frame loop: update → render → trigger](diagrams/frame-loop.png)

A single `requestAnimationFrame` callback (`FrameContext::frame`) runs three phases on the shared state:

1. **Schedule** — unless paused, `MusicEngine::tick(dt)` advances the eighth-note grid and emits this frame's `NoteEvent`s. Each event bumps its voice's pulse energy.
2. **Couple state to audio + visuals** —
   - smooth the per-voice pulse energies; decay gesture energy/flash/spin;
   - update the inertial **swirl** from the pointer (or multitouch centroid) — a damped spring in UV space;
   - modulate the **global FX** (reverb wet, delay wet/feedback, saturation drive/mix) from swirl energy, gesture flash, and pointer position;
   - push each voice's engine position into its `PannerNode`, and set its delay/reverb sends and level from distance;
   - align the `AudioListener` to the fixed camera.
3. **Render + sound** — feed the ambient clear color, any queued ripple, and the smoothed swirl strength into `GpuState`, then `render()` (waves → bloom → composite). Finally, fire a one-shot oscillator voice for each scheduled `NoteEvent`.

Loud note onsets also queue a visual ripple, so the picture pulses with the music. State lives in the engine and the GPU between frames; the loop is a tail chain of rAF calls, not a timer.

## Audio Engine

![Audio graph](diagrams/audio-graph.png)

`audio.rs` builds the WebAudio graph once (`build_fx_buses`, `wire_voices`) and fires notes through it (`trigger_one_shot`).

**Per note.** A `NoteEvent` becomes an `OscillatorNode` (the voice's waveform) plus a slightly detuned **chorus** oscillator, both through one envelope `GainNode` (attack → sustain → exponential release, shaped per waveform with a short pitch glide). The envelope feeds three places: the voice gain, the delay send, and the reverb send.

**Per voice.** `voice gain → PannerNode (HRTF, inverse-distance) → master`. Each voice also has a delay send and a reverb send. Per frame, the voice's engine-space position drives the panner and scales its sends and level by distance, so the carve gesture's moving voices sweep through space.

**Master chain.** Everything sums into `master_gain`, then: a high-pass + low-pass tone shaping, an arctan **WaveShaper** saturation (wet/dry blended), a **DynamicsCompressor** with makeup gain, and out to `destination`. The reverb bus is a procedurally-generated convolution IR; the delay bus is a `DelayNode` with a low-passed feedback loop. Swirl/gesture energy modulates the wet levels and saturation drive each frame (see the frame loop), so motion audibly opens up the space.

> An `AnalyserNode` taps the master bus, so the frame loop's ambient energy responds to the overall output level alongside the per-note pulses.

## Generative Music Engine

`core/music.rs` is the headless heart. `MusicEngine` holds three voices (default: saw bass, triangle mid, sine high), each with its own seeded `StdRng`, and advances an eighth-note grid in `tick`. Per step, for each voice:

- a **Euclidean gate** (per-voice polymeter, e.g. 5-in-13, 7-in-11, 4-in-17) blended with a swing term and a position-driven travel term sets the trigger probability;
- an **accent gate** and the voice's base probability gate whether a note fires;
- a **motif table** plus rotating **phrase root-shifts** pick the scale degree, with register, contour, octave offset, and a little micro-drift shaping the final MIDI pitch;
- per-voice velocity/duration curves shape the envelope.

Pitch is `midi_to_hz` (A4 = 440) with a global **detune in cents** (±200) applied before conversion. Scales are the seven diatonic modes plus a C-major pentatonic preset and three alt-tuning pentatonics — 19- and 31-TET (n-EDO-derived) and a quarter-tone 24-TET (`8`/`9`/`0`). Reseeding a voice (`R`, gesture release, etc.) swaps its RNG for a fresh sequence. The engine is deterministic given a seed, which is what makes it unit-testable.

## Visual Engine

`render.rs` (`GpuState`) renders entirely in screen space — there is no 3D scene. The "camera" is fixed at `(0, 0, 6)` and exists only to anchor the `AudioListener`.

Resources: one offscreen **HDR** scene target (`Rgba16Float`) plus two half-resolution **bloom** buffers. Each frame:

1. **scene pass** — clear the HDR target to a dark slate that lifts toward a teal/amber haze with ambient energy, then draw the **waves** fullscreen pass (`waves.wgsl`): layered ribbons displaced by the pointer-driven swirl, per-voice influence and pulses, and propagating click/tap ripples;
2. **bloom** — bright-pass (HDR → bloom A), separable blur (A → B horizontal, B → A vertical);
3. **composite** — `post.wgsl` adds the bloom back, applies an ACES tonemap, vignette, and film grain, and writes the swapchain.

No depth buffer; `Fifo` present (vsync). On resize, the surface and both offscreen targets are rebuilt and the dependent bind groups regenerated.

## Interaction

Pointer and keyboard handlers live in `events/`; the full control list is in the [README § Controls](../README.md#controls). The pointer model (`events/pointer.rs`):

- **Tap (no drag) → flare** — a chord stack of one-shot notes plus a ripple at the cursor.
- **Hold + drag → carve** — continuously rewrites BPM (from travel), detune (from position + rotation), and the voices' lattice positions, periodically reseeding and emitting ripples.
- **Release after a carve → drop** — locks in a new root (from drag angle) and mode (from travel/spin), reseeds all voices, and fires an accent burst.
- **Multitouch** (up to 5 pointers, tracked in `MultiTouchState`): 2-finger pinch→BPM / rotate→detune, 3-finger swipe→root/mode, 4-finger tap→randomize, 5-finger tap→pause.

## Build & Deploy

- `npm run build` → `wasm-pack build --target web --release`, then `scripts/gen-env.js` stamps `pkg/env.js` with the git short SHA, and the JS + wasm + `index.html` + `favicon.svg` are copied into `dist/`.
- `worker.js` runs before asset serving (`run_worker_first`) and sets `Cache-Control`: the JS glue and wasm — both loaded with a `?v=<git-sha>` tag (`index.html` versions the wasm URL too) — are `immutable`, while the `env.js` version pointer and the HTML entry are `no-cache`, so a deploy is picked up immediately while the heavy assets cache forever.
- `npm run dev` builds and serves locally; `npm run deploy` builds and ships it. CI (`.github/workflows/ci.yml`) runs the full gate on every push/PR and deploys to Cloudflare on `main` when the Cloudflare secrets are present.

## What This Architecture Deliberately Does Not Include

- **No WebGL fallback.** The renderer targets WebGPU; `index.html` checks for it and shows a message rather than degrading.
- **No AudioWorklet.** Note scheduling runs on the main thread via the rAF loop and WebAudio's own lookahead scheduling; sample-accurate timing via an AudioWorklet is a [backlog](BACKLOG.md) item.
- **No 3D scene / object picking.** Audio is spatialized through per-voice panners, but the voices are not interactive on-screen objects — the visuals are a screen-space shader.
- **No server or persistence.** Everything runs client-side; there is no backend or saved state.
- **No threads.** The WASM is single-threaded — no `SharedArrayBuffer`, so no cross-origin-isolation headers are needed.
