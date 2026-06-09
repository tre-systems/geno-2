# Legacy Relay Tooling

The relay scripts and Durable Object protocol remain in the repository for
testing and future experiments, but they are no longer the recommended
performance setup.

For current iPad/live use:

- Open `/` for the clean instrument surface.
- Open `/control` in a separate same-origin tab or window for settings, then
  enter the panel's code from the instrument help screen (`H`, bottom right).
- Use touch directly on the surface; active fingers are visible in the shader and
  affect the instrument locally.
- With no linked panel, the instrument remains fully local.

The deployed Worker returns `404` for `/room/*` unless `RELAY_ENABLED=true` is
set. Leave it disabled in production unless there is a specific reason to
restore server-relayed display clients.

The legacy relay still has tests and protocol guards:

- `npm run relay:test` — auth, abuse guards, broadcast, state replay.
- `npm run cf-relay:test` — the Durable Object under `wrangler dev`, with
  `RELAY_ENABLED:true` and a test `RELAY_KEY`.

If the relay is reintroduced publicly, keep `RELAY_ENABLED` opt-in, require
`RELAY_KEY`, preserve origin checks and per-room limits, and add an edge rate
limiting rule on `/room/*` so room-spawning floods are shed before they create
Durable Object/WebSocket work.
