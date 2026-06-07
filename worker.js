// Cloudflare Worker: serves the static app and hosts the performance relay.
//
// Static assets get cache headers (versioned wasm/js immutable; HTML/env
// revalidated). WebSocket upgrades to /room/<id> are routed to a Room Durable
// Object that broadcasts performer parameter changes to every connected display
// client and replays accumulated state to late joiners — the same protocol as
// scripts/relay.mjs, so control.html and ?mode=display work same-origin once
// deployed.

export class Room {
  constructor(state) {
    this.state = state;
    this.params = null; // accumulated control state, lazily restored from storage
  }

  async params_() {
    if (this.params === null) {
      this.params = (await this.state.storage.get("params")) || {};
    }
    return this.params;
  }

  async fetch(request) {
    if (request.headers.get("Upgrade") !== "websocket") {
      return new Response("expected websocket", { status: 426 });
    }
    const [client, server] = Object.values(new WebSocketPair());
    this.state.acceptWebSocket(server); // hibernatable
    server.send(JSON.stringify({ t: "state", state: await this.params_() }));
    return new Response(null, { status: 101, webSocket: client });
  }

  async webSocketMessage(ws, message) {
    let msg;
    try {
      msg = JSON.parse(typeof message === "string" ? message : new TextDecoder().decode(message));
    } catch {
      return;
    }
    if (msg.t === "set" && typeof msg.k === "string") {
      const params = await this.params_();
      params[msg.k] = msg.v;
      await this.state.storage.put("params", params);
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
      return env.ROOM.get(env.ROOM.idFromName(room[1])).fetch(request);
    }

    const response = await env.ASSETS.fetch(request);
    if (!response.ok) return response;
    const res = new Response(response.body, response);
    const path = url.pathname;
    const versioned = url.searchParams.has("v");

    if (versioned && (path.endsWith(".wasm") || path.endsWith(".js"))) {
      // Versioned (?v=<git-sha>) glue/wasm are immutable per deploy.
      res.headers.set("Cache-Control", "public, max-age=31536000, immutable");
    } else if (path.endsWith("env.js") || path === "/" || path.endsWith(".html")) {
      // The version pointer and HTML must revalidate so deploys are picked up.
      res.headers.set("Cache-Control", "no-cache");
    }

    return res;
  },
};
