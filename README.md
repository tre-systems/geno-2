# Geno-2: Ambient Generative Instrument

<div align="center">

![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)
![WebAssembly](https://img.shields.io/badge/WebAssembly-654FF0?style=for-the-badge&logo=webassembly&logoColor=white)
![WebGPU](https://img.shields.io/badge/WebGPU-005A9C?style=for-the-badge&logo=gpu&logoColor=white)
[![CI](https://github.com/rgilks/geno-2/actions/workflows/ci.yml/badge.svg)](https://github.com/rgilks/geno-2/actions/workflows/ci.yml)

</div>

<div align="center">
  <img src="/docs/screenshot.png" alt="geno-2 screenshot" width="902" />
</div>

Geno-2 is an original audiovisual instrument built with Rust + WebAssembly + WebGPU + WebAudio.  
It keeps the Geno stack, but shifts into a different artistic direction: shoegaze/ambient tone design and high-energy kaleidoscopic interaction.

## Highlights

- Original visual system: geometric lattice distortion, ripple propagation, and audio-coupled swirl field.
- Original gesture model: click for flare stacks; hold+drag to carve/reseed the field with strong musical and visual impact.
- Ambient-forward mix design with dynamic leveling, compression, and FX mapping.
- Deterministic build/deploy pipeline with CI checks on Rust + browser integration tests.

## Stack

- Rust 2021
- WebAssembly (`wasm-pack`)
- WebGPU (`wgpu`, WGSL shaders)
- WebAudio (procedural synthesis + FX graph)
- Cloudflare Workers static hosting (`wrangler`)

## Controls

- `A..G`: set root note
- `1..7`: set mode
- `8/9/0`: alternate pentatonic tunings
- `P`: reset to C Major pentatonic preset
- `R`: reseed sequence
- `T`: random root + mode
- `Space`: pause/resume
- `,` / `.`: detune (hold `Shift` for fine adjustment)
- `/`: reset detune
- `←/→`: tempo
- `↑/↓`: master volume
- `M`: mute/unmute master
- `Enter` / `Esc`: fullscreen
- `H`: toggle help panel
- `Click`: flare chord stack + shockwave
- `Hold + Drag`: carve/warp field + continuous reseeding
- `Release`: drop carve into new root/mode with accent burst

### Touch (iPad / mobile)

- `2-finger pinch`: adjust BPM (spread = faster, pinch = slower)
- `2-finger rotate`: adjust detune (twist clockwise = sharp, counter-clockwise = flat)
- `3-finger swipe left/right`: cycle root note (circle-of-fifths order)
- `3-finger swipe up/down`: cycle scale mode (Ionian through Locrian)
- `4-finger tap`: randomize root + mode + reseed all voices
- `5-finger tap`: toggle pause/resume

## Documentation

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — how the code is organized and how a frame of audio + video is produced.
- [docs/diagrams/](docs/diagrams/) — system overview, frame loop, and audio graph.
- [docs/BACKLOG.md](docs/BACKLOG.md) — ordered next work and known issues.

## Local Development

Requirements:

- Node.js 20+
- Rust stable
- `wasm-pack`

Commands:

- `npm install`
- `npm run dev`
- `npm run check`

## Quality Gate

`npm run check` runs:

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- production wasm build
- browser integration test (`web-test.js`)

## Deployment

- Build: `npm run build`
- Deploy: `npx --yes wrangler deploy`
- CI deploys on `main` only when `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` are configured.
