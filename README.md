# Geno-2: Prismatic Generative Instrument

Geno-2 is a new branch of the Geno concept with the same technology stack but a distinctly different musical and visual identity.

## Stack

- Rust (core engine)
- WebAssembly (`wasm-pack`)
- WebGPU (`wgpu` + WGSL shaders)
- WebAudio (procedural synthesis + FX graph)
- Cloudflare Worker for static hosting

## What Is Different From Geno-1

- New rhythm engine: polymetric/euclidean phrase logic with motif-led register motion
- New interaction model: click creates burst chords, drag performs gestural tempo/detune/space sweeps
- New FX tone: louder output, tighter room tail, quicker delay, waveform-shaped envelopes
- New visual direction: geometric kaleidoscope lattice + print-like post grade (not wave sheets)
- Restyled overlay/theme for project identity

## Controls

- `A..G`: set root note
- `1..7`: set mode
- `8/9/0`: set alternate pentatonic tuning
- `P`: reset to C Major Pentatonic preset
- `R`: new sequence seeds
- `T`: random root + mode
- `Space`: pause/resume
- `,` / `.`: detune (hold Shift for fine adjustment)
- `/`: reset detune
- `←/→`: tempo
- `↑/↓`: master volume
- `M`: mute/unmute
- `Enter` / `Esc`: fullscreen
- `H`: toggle help panel
- `Click`: trigger burst chord at cursor
- `Drag`: morph tempo/detune and rotate voice geometry
- `Release after drag`: commit new root + mode + phrase reseed

## Local Development

Requirements:

- Node 20+
- Rust stable
- `wasm-pack`

Commands:

- `npm install`
- `npm run dev`
- `npm run check`

## Deployment Notes

- `wrangler.toml` is set to `workers_dev = true` and `ENVIRONMENT = "private"` by default.
- CI deploy runs on `main` only when Cloudflare secrets exist.
- Keep the GitHub repo private until you are ready to expose/demo.
