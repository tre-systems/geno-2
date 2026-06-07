# Claude Code Notes

Read `AGENTS.md` first — it is the source of truth for workflow, verification commands, and architecture rules in this repo.

Key reflexes for Geno-2:

- Work on `main` and push there. A push to `main` deploys to Cloudflare, so after a code change confirm CI is green and smoke-test <https://geno-2.tre.systems>. Docs-only changes just need commit + push.
- Fast local gate: `npm run check:rust`. Full gate before push: `npm run check` (Rust + diagrams + web build + Puppeteer smoke test). `npm run setup` installs the git hooks.
- Keep `src/core` host-testable — no `web-sys` there. The generative engine (`core::music`) and gesture geometry (`input::MultiTouchState`) must stay unit-testable with `cargo test`.
- Diagrams: edit the `.dot` sources in `docs/diagrams/`, then `npm run diagrams` to re-render the PNGs (needs Graphviz — `brew install graphviz`).
