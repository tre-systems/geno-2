// Cloudflare Worker: serves the static app and hosts the performance relay.
//
// Security / cost model — the relay is a PUBLIC WebSocket endpoint, locked down
// in depth (no platform offers a hard spend cap, so pair this with a Cloudflare
// billing alert):
//   * Control + broadcast require the shared secret RELAY_KEY. Unauthenticated
//     sockets are read-only viewers; if RELAY_KEY is unset the relay is LOCKED
//     (fail closed). Set it with `wrangler secret put RELAY_KEY`. The key is
//     compared in constant time, and a socket is dropped after too many wrong
//     guesses (anti-brute-force).
//   * Per-room connection cap, per-socket rate + size limits with strike-based
//     disconnect, a strict parameter whitelist, sanitized gesture events, and
//     throttled storage writes bound per-connection abuse and cost.
//   * Cross-origin browser connections are rejected. The same-origin check is
//     skipped for non-browser clients (no Origin header), so the RELAY_KEY gate,
//     not the Origin header, is the real authority.
//   * In-code limits are per room (each room is an independent Durable Object),
//     so they can't bound someone spawning many rooms/connections. Add an edge
//     Rate Limiting rule on /room/* (see docs/NETWORKED_PERFORMANCE.md) to shed
//     connection floods before they reach a Durable Object.

import {
  ALLOWED_KEYS,
  LIMITS,
  TRANSIENT_LIMITS,
  validParam,
  sanitizeEvent,
  keyMatches,
} from "./scripts/relay-protocol.mjs";

export class Room {
  constructor(state, env) {
    this.state = state;
    this.env = env;
    this.params = null;
    this.lastPersist = 0;
    this.rate = new WeakMap(); // ws -> { times:number[], strikes:number } for {t:"set"}
    this.evRate = new WeakMap(); // ws -> { times:number[], strikes:number } for {t:"ev"}
  }

  async params_() {
    if (this.params === null) this.params = (await this.state.storage.get("params")) || {};
    return this.params;
  }

  async fetch(request) {
    if (request.headers.get("Upgrade") !== "websocket") {
      return new Response("expected websocket", { status: 426 });
    }
    if (this.state.getWebSockets().length >= LIMITS.maxSocketsPerRoom) {
      return new Response("room full", { status: 503 });
    }
    const [client, server] = Object.values(new WebSocketPair());
    this.state.acceptWebSocket(server);
    server.serializeAttachment({ authed: false });
    server.send(JSON.stringify({ t: "state", state: await this.params_() }));
    return new Response(null, { status: 101, webSocket: client });
  }

  // Per-socket sliding-window rate limit. Persistent strikes close a flooding
  // socket. Shared by the param ({t:"set"}) and gesture ({t:"ev"}) budgets.
  rateOkIn(map, ws, limit) {
    const now = Date.now();
    let r = map.get(ws);
    if (!r) {
      r = { times: [], strikes: 0 };
      map.set(ws, r);
    }
    r.times = r.times.filter((t) => now - t < 1000);
    r.times.push(now);
    if (r.times.length > limit) {
      if (++r.strikes > 20) {
        try {
          ws.close(1008, "rate limit");
        } catch {}
      }
      return false;
    }
    if (r.strikes > 0) r.strikes--; // recover on good behaviour; only sustained floods reach 20
    return true;
  }

  rateOk(ws) {
    return this.rateOkIn(this.rate, ws, LIMITS.maxMsgPerSec);
  }

  rateOkTransient(ws) {
    return this.rateOkIn(this.evRate, ws, TRANSIENT_LIMITS.maxEvPerSec);
  }

  // Fan a message out to every peer in the room except the sender.
  broadcast(from, out) {
    for (const peer of this.state.getWebSockets()) {
      if (peer !== from) {
        try {
          peer.send(out);
        } catch {}
      }
    }
  }

  async webSocketMessage(ws, message) {
    // Reject oversized frames before decoding/parsing them.
    const size = typeof message === "string" ? message.length : message.byteLength;
    if (size > LIMITS.maxMsgBytes) return;
    const raw = typeof message === "string" ? message : new TextDecoder().decode(message);

    let msg;
    try {
      msg = JSON.parse(raw);
    } catch {
      return;
    }

    // Per-type rate budgets: gesture events stream faster than params.
    if (!(msg.t === "ev" ? this.rateOkTransient(ws) : this.rateOk(ws))) return;

    if (msg.t === "auth") {
      const att = ws.deserializeAttachment() || {};
      const ok = await keyMatches(msg.key, this.env.RELAY_KEY);
      const fails = ok ? 0 : (att.fails || 0) + 1;
      ws.serializeAttachment({ authed: ok, fails });
      ws.send(JSON.stringify({ t: "auth", ok }));
      // Anti-brute-force: drop the socket after too many wrong keys so an
      // attacker must reconnect (cheap to detect / shed at the edge).
      if (!ok && fails >= LIMITS.maxAuthFails) {
        try {
          ws.close(1008, "too many auth attempts");
        } catch {}
      }
      return;
    }

    if (msg.t === "set" && typeof msg.k === "string") {
      if (!this.isAuthed(ws)) {
        ws.send(JSON.stringify({ t: "error", e: "unauthorized" }));
        return;
      }
      if (!ALLOWED_KEYS.has(msg.k) || !validParam(msg.k, msg.v)) return;

      const params = await this.params_();
      params[msg.k] = msg.v;
      const now = Date.now();
      if (now - this.lastPersist >= LIMITS.persistThrottleMs) {
        this.lastPersist = now;
        await this.state.storage.put("params", params);
      }
      this.broadcast(ws, JSON.stringify({ t: "set", k: msg.k, v: msg.v }));
      return;
    }

    // Transient performance events: authed-only, broadcast to peers, never
    // persisted and never replayed to late joiners (so a flare doesn't fire on
    // someone who joins minutes later). Re-broadcast a sanitized copy so no
    // attacker-added fields ride along.
    if (msg.t === "ev") {
      if (!this.isAuthed(ws)) {
        ws.send(JSON.stringify({ t: "error", e: "unauthorized" }));
        return;
      }
      const clean = sanitizeEvent(msg);
      if (!clean) return;
      this.broadcast(ws, JSON.stringify(clean));
    }
  }

  isAuthed(ws) {
    const att = ws.deserializeAttachment();
    return !!att && att.authed === true;
  }

  async webSocketClose(ws, code) {
    try {
      ws.close(code, "closing");
    } catch {}
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const room = url.pathname.match(/^\/room\/([\w-]{1,64})$/);
    if (room) {
      if (request.headers.get("Upgrade") !== "websocket") {
        return new Response("expected websocket", { status: 426 });
      }
      // Reject cross-origin browser connections (Origin is absent for non-browser clients).
      const origin = request.headers.get("Origin");
      if (origin) {
        try {
          if (new URL(origin).host !== url.host) {
            return new Response("forbidden origin", { status: 403 });
          }
        } catch {
          return new Response("forbidden origin", { status: 403 });
        }
      }
      return env.ROOM.get(env.ROOM.idFromName(room[1])).fetch(request);
    }

    const assetUrl = new URL(request.url);
    if (assetUrl.pathname === "/control") assetUrl.pathname = "/control.html";
    const assetRequest =
      assetUrl.toString() === request.url ? request : new Request(assetUrl, request);

    const response = await env.ASSETS.fetch(assetRequest);
    if (!response.ok) return response;
    const res = new Response(response.body, response);
    const path = assetUrl.pathname;
    const versioned = url.searchParams.has("v");

    if (versioned && (path.endsWith(".wasm") || path.endsWith(".js"))) {
      res.headers.set("Cache-Control", "public, max-age=31536000, immutable");
    } else if (path.endsWith("env.js") || path === "/" || path.endsWith(".html")) {
      res.headers.set("Cache-Control", "no-cache");
    }

    return res;
  },
};
