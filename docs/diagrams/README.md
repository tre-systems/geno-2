# Diagrams

Graphviz / DOT sources plus rendered PNGs. The `.dot` files are the source of truth; the PNGs are committed for in-browser viewing on GitHub. Graphviz is for standalone architecture and flow diagrams; Mermaid is reserved for small inline diagrams inside Markdown.

## Files

| Diagram                          | Source                | Rendered              |
| -------------------------------- | --------------------- | --------------------- |
| System overview                  | `system-overview.dot` | `system-overview.png` |
| Frame loop (update → render)     | `frame-loop.dot`      | `frame-loop.png`      |
| Audio graph (WebAudio topology)  | `audio-graph.dot`     | `audio-graph.png`     |

## Reading order

1. **System overview** for the whole shape: Cloudflare Worker → the browser page → the Rust/WASM app core → the two outputs (the WebAudio graph to the speakers, the WebGPU passes to the canvas).
2. **Frame loop** for what one `requestAnimationFrame` does: schedule notes, couple state to audio + visuals, render, then trigger the frame's voices.
3. **Audio graph** for the WebAudio node topology: per-note synthesis → per-voice panner + sends → reverb/delay buses → master tone/saturation/compression → destination.

## Conventions

Color coding by domain:

- Teal — the host (Cloudflare Worker serving `dist/`).
- Blue — the browser / client surface (`index.html` bootstrap, input).
- Green — the Rust WASM app core (`wasm_app`, `FrameContext`, `MusicEngine`).
- Amber — the WebAudio graph (oscillators, buses, master chain).
- Purple — the WebGPU rendering boundary (waves pass, bloom, composite).
- Bold green outline — terminal outputs (`<canvas>`, speakers).
- Diamonds — decisions (white fill, dark border).

Fonts: Avenir. Rendered at 220 DPI.

## Render

```
npm run diagrams          # render all .dot files to PNG next to the source
npm run check:diagrams    # verify each .dot renders cleanly and the PNG exists
```

Both scripts assume Graphviz is on PATH (`brew install graphviz`). CI installs Graphviz before running `check:diagrams` (see `.github/workflows/diagrams.yml`). On a machine without `dot`, `check:diagrams` skips with a clear message; refresh the PNGs with `npm run diagrams` before committing diagram changes.

To render one manually:

```
dot -Tpng:cairo docs/diagrams/<name>.dot -Gdpi=220 -o docs/diagrams/<name>.png
```
