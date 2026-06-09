# Geno-2 // Lattice

<div align="center">

![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)
![WebAssembly](https://img.shields.io/badge/WebAssembly-654FF0?style=for-the-badge&logo=webassembly&logoColor=white)
![WebGPU](https://img.shields.io/badge/WebGPU-005A9C?style=for-the-badge&logo=gpu&logoColor=white)
[![CI](https://github.com/tre-systems/geno-2/actions/workflows/ci.yml/badge.svg)](https://github.com/tre-systems/geno-2/actions/workflows/ci.yml)

</div>

<div align="center">
  <img src="/docs/screenshot.png" alt="geno-2 screenshot" width="902" />
</div>

Geno-2 is an original audiovisual instrument built with Rust + WebAssembly + WebGPU + WebAudio.  
It plays as **Lattice**: a geometric pulse instrument — sharp, polymetric rhythms and faceted, kaleidoscopic light over an atmospheric, reverb-and-delay mix. It starts in D Dorian.

## Highlights

- Original visual system: geometric lattice distortion, ripple propagation, and audio-coupled swirl field.
- Original gesture model: click for flare stacks; hold+drag to carve/reseed the field with strong musical and visual impact.
- Atmospheric, reverb-and-delay-forward mix with dynamic leveling, compression, and gesture-driven FX mapping.
- Deterministic build/deploy pipeline with CI checks on Rust + browser integration tests.

## Stack

Rust + WebAssembly + WebGPU (`wgpu`) + WebAudio, built with `wasm-pack` and served from Cloudflare Workers. Full breakdown in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#stack).

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

- `1-finger tap`: flare chord stack + visible shockwave
- `1-finger hold + drag`: carve the field, bend tempo/detune, move voices, and drop into a new root/mode on release
- `2+ fingers touch + drag`: continuous performance surface; every active finger is drawn into the shader, the centroid steers the swirl, spread nudges tempo, rotation bends detune, and finger count/depth open the visual/audio energy
- `2+ fingers release`: ends the surface cleanly with a soft ripple, without randomizing, pausing, or reseeding

## Control panel & offline render

The same engine also supports a separate panel and offline rendering (details in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#control-panel)):

- **Separate control panel** — open `/` for the instrument and `/control` in another same-origin tab/window. The panel shows a random code; press `H` on the instrument and enter that code in the bottom-right control panel area to link it. With no linked panel, all controls stay local.
- **Offline render** — `node scripts/render-offline.mjs` renders a seed to a deterministic 32-bit-float WAV under an `OfflineAudioContext`, faster than realtime.

## Documentation

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — how the code is organized and how a frame of audio + video is produced.
- [docs/diagrams/](docs/diagrams/) — system overview, frame loop, and audio graph.
- [docs/BACKLOG.md](docs/BACKLOG.md) — ordered next work and known issues.

## Develop

Requires Node.js 22 and Rust stable; `npm install` pulls in `wasm-pack`.

- `npm install`
- `npm run dev` — build and serve locally (needs a WebGPU-capable browser).
- `npm run check` — the full gate: format, clippy, tests, diagram render, wasm build, and a Puppeteer smoke test. `npm run check:rust` is the fast inner loop.

## Deploy

`npm run deploy` builds and ships to Cloudflare Workers. CI also deploys on every push to `main` when `CLOUDFLARE_API_TOKEN` / `CLOUDFLARE_ACCOUNT_ID` are configured.
