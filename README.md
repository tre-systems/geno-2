# Geno-2: Prismatic Generative Instrument

Geno-2 is a new branch of the Geno concept with the same technology stack but a distinctly different musical and visual identity.

## Stack

- Rust (core engine)
- WebAssembly (`wasm-pack`)
- WebGPU (`wgpu` + WGSL shaders)
- WebAudio (procedural synthesis + FX graph)
- Cloudflare Worker for static hosting

## What Is Different From Geno-1

- New rhythm engine: pattern-driven gating, contour motion, and accent logic
- New default voice profile: lower tempo, Dorian-first tonal center, different voice spacing
- New FX tone: shorter chamber-style reverb, band-limited delay, lighter saturation blend
- New visual direction: prismatic ribbon fields, orbital voice warps, new color grade/post pipeline
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
