# Project Rules (Agent-Agnostic)

This file is intentionally tool-neutral and should be usable by both GPT Codex and Claude Code.

## Mission

- Keep the current stack: Rust + WebAssembly + WebGPU + WebAudio + Node tooling.
- Make changes that improve clarity, reliability, and creative distinctiveness.
- Preserve Geno-2 as a clearly different audiovisual experience (not a minor reskin).

## Engineering Standards

- Prefer small, reviewable changes over large rewrites.
- Keep code understandable; split functions/modules when complexity grows.
- Remove dead code and stale comments.
- Add comments only where non-obvious logic needs context.
- Do not introduce unrelated refactors during focused fixes.

## Stack and Architecture

- WebGPU via `wgpu` is required; do not add WebGL fallback unless explicitly requested.
- Keep Rust logic host-testable where practical.
- Keep browser-specific behavior gated to WASM/web modules.
- Avoid adding new runtime dependencies without clear benefit.
- Any stack change (language/runtime/framework/deploy platform) requires explicit user approval and a short rationale in the change summary.

## Validation Paths

Use the smallest reliable gate during development, then run the full gate before push:

- Fast path (small/local change): `npm run check:rust`
- Full path (behavior/audio/render/input/deploy changes): `npm run check`

Expected checks include:

- `cargo fmt --check`
- `cargo clippy -D warnings`
- `cargo test`
- Web build + Puppeteer smoke test (`web-test.js`)

## Audio/Visual Direction

- Maintain a coherent visual identity across `shaders/`, post-processing, and overlay styling.
- Keep interaction responsive; avoid effects that make controls feel laggy.
- Validate that audio changes still preserve reliable browser unlock behavior.

## UX Regression Guards

- Do not break existing keyboard/pointer controls unless explicitly requested.
- Keep help panel behavior stable (`H` toggle, close/reopen flows).
- Keep `web-test.js` green before push.

## Documentation

- Update docs only when behavior, controls, architecture, or deployment expectations changed.
- Typical targets:
  - `README.md`
  - `docs/SPEC.md` (when architecture/intent changed)
  - `docs/TODO.md` (when priorities changed)

## Git Workflow

- Use clear commit messages that describe user-visible intent.
- Do not rewrite history unless explicitly requested.
- Keep main branch deploy-safe (green checks before push).
