// Cloudflare Worker: serves the static app and hosts the performance relay.
//
// Security / cost model (pairs with a Cloudflare billing alert + edge rate-limit
// rule on /room/* — no platform offers a hard spend cap):
//   * Control requires the shared secret RELAY_KEY. Sockets that don't
//     authenticate are read-only viewers; if RELAY_KEY is unset the relay is
//     LOCKED (no control) — fail closed. Set it with `wrangler secret put
//     RELAY_KEY`.
//   * Per-room connection cap, per-socket message rate + size limits, a strict
//     parameter whitelist, and throttled storage writes bound abuse and cost.
//   * Cross-origin browser connections are rejected.

import { ALLOWED_KEYS, LIMITS, validParam } from "./scripts/relay-protocol.mjs";

export class Room {
  constructor(state, env) {
    this.state = state;
    this.env = env;
    this.params = null;
    this.lastPersist = 0;
    this.rate = new WeakMap(); // ws -> { times:number[], strikes:number }
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

  rateOk(ws) {
    const now = Date.now();
    let r = this.rate.get(ws);
    if (!r) {
      r = { times: [], strikes: 0 };
      this.rate.set(ws, r);
    }
    r.times = r.times.filter((t) => now - t < 1000);
    r.times.push(now);
    if (r.times.length > LIMITS.maxMsgPerSec) {
      if (++r.strikes > 20) {
        try {
          ws.close(1008, "rate limit");
        } catch {}
      }
      return false;
    }
    return true;
  }

  async webSocketMessage(ws, message) {
    const raw = typeof message === "string" ? message : new TextDecoder().decode(message);
    if (raw.length > LIMITS.maxMsgBytes) return;
    if (!this.rateOk(ws)) return;

    let msg;
    try {
      msg = JSON.parse(raw);
    } catch {
      return;
    }

    if (msg.t === "auth") {
      const key = this.env.RELAY_KEY;
      const ok = typeof key === "string" && key.length > 0 && msg.key === key;
      ws.serializeAttachment({ authed: ok });
      ws.send(JSON.stringify({ t: "auth", ok }));
      return;
    }

    if (msg.t === "set" && typeof msg.k === "string") {
      const att = ws.deserializeAttachment();
      if (!att || att.authed !== true) {
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
      const out = JSON.stringify({ t: "set", k: msg.k, v: msg.v });
      for (const peer of this.state.getWebSockets()) {
        if (peer !== ws) {
          try {
            peer.send(out);
          } catch {}
        }
      }
    }
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

    const response = await env.ASSETS.fetch(request);
    if (!response.ok) return response;
    const res = new Response(response.body, response);
    const path = url.pathname;
    const versioned = url.searchParams.has("v");

    if (versioned && (path.endsWith(".wasm") || path.endsWith(".js"))) {
      res.headers.set("Cache-Control", "public, max-age=31536000, immutable");
    } else if (path.endsWith("env.js") || path === "/" || path.endsWith(".html")) {
      res.headers.set("Cache-Control", "no-cache");
    }

    return res;
  },
};
