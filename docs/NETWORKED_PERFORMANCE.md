# Networked Performance

Drive the instrument on remote displays (an iPad, an audience's phones) from a
laptop or the iPad itself, over the internet. Only **control** crosses the
network — shared parameters, and in broadcast mode the performer's live gestures
— never audio or video. Each device renders locally, so bandwidth is tiny and
every device runs at full quality.

## Run it

- **Display (iPad / audience):**
  `https://geno-2.tre.systems/?mode=display&room=<room>` — tap once to start
  audio. The UI is hidden; it just follows.
- **Control (laptop):** `https://geno-2.tre.systems/control` — enter the room and
  the control key, then Connect. Sliders for tempo, detune, root, scale, seed,
  master, and pause.
- **Broadcast (perform from the iPad):**
  `https://geno-2.tre.systems/?mode=broadcast&room=<room>` — play the full
  instrument. A panel (top-right) takes the control key; once connected, your
  taps, drags, and multi-finger gestures (flares, carves, ripples, and the
  pointer swirl) stream to every display in the room, which reproduce them
  locally — sound and picture. Parameter changes (from gestures or the keyboard)
  broadcast too, so displays stay in the same key and tempo.

Both default to room `main`. Any number of displays can join a room; new joiners
catch up to the current parameter state immediately (gestures are live-only).

## Security model

The relay is a public WebSocket endpoint (a Cloudflare Durable Object), so it is
locked down by default:

- **Control and broadcast require the shared secret `RELAY_KEY`.** Sockets that
  do not authenticate are read-only viewers. **If `RELAY_KEY` is unset, control
  is disabled entirely (fail closed)** — displays still render, but nobody can
  drive the instrument.
- **Gestures ride a separate transient channel (`{t:"ev"}`):** authed-only like
  control, broadcast to the room but **never persisted or replayed** to late
  joiners, with its own higher per-socket rate budget (a gesture stream is faster
  than slider changes; the broadcaster coalesces its output to fit). Payloads are
  tiny uv-anchored events.
- Per-room connection cap (200), per-socket message rate + size (1 KB) limits, a
  strict parameter whitelist with range checks, and throttled storage writes —
  together these bound abuse and cost.
- Cross-origin browser connections are rejected.

The local dev relay (`scripts/relay.mjs`) mirrors the same protocol, auth, and
limits, gated on the `RELAY_KEY` environment variable.

## Required setup

Set the control secret (pick a strong value — this is what enables performers):

```sh
npx wrangler secret put RELAY_KEY
```

For local development, provide it via the environment:

```sh
RELAY_KEY=your-secret npm run dev
```

## Cost protection

Cloudflare offers **no hard spend cap**, so the safety net is layered:

1. The in-code limits above bound worst-case Durable Object compute, storage,
   and broadcast fan-out. Idle WebSocket connections hibernate (no duration
   billing); only active message processing and (throttled) storage writes bill.
2. Set a **billing usage alert** in the Cloudflare dashboard (Notifications → add
   a Workers/usage billing alert) so you are emailed before costs climb.
3. Optional: add a **Rate Limiting rule** on `/room/*` (Security → WAF) to shed
   connection/request floods at the edge, before they reach the Durable Object.

## Tests

- `npm run relay:test` — auth, abuse guards, broadcast, state replay (node relay).
- `npm run display:test` — a broadcast parameter reaches the live engine (headless browser).
- `npm run cf-relay:test` — the Durable Object under `wrangler dev`.
