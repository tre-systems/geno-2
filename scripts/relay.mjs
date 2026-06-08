// Minimal WebSocket relay for networked performance (local/dev mirror of the
// Cloudflare Durable Object in worker.js — same protocol, auth, and limits).
//
// One "room" per id. A performer authenticates with the shared RELAY_KEY, then
// sends {t:"set",k,v} messages; the relay records the latest value per key and
// broadcasts each change to every *other* client. Unauthenticated sockets are
// read-only viewers. New clients receive the accumulated state on connect.
//
//   RELAY_KEY=secret node scripts/relay.mjs [port]

import { WebSocketServer } from "ws";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const CONTROL_HTML = join(here, "..", "control.html");

const ALLOWED_KEYS = new Set(["bpm", "detune", "root", "scale", "seed", "paused", "volume"]);
const LIMITS = { maxSocketsPerRoom: 200, maxMsgPerSec: 16, maxMsgBytes: 1024 };

function validParam(k, v) {
  switch (k) {
    case "bpm":
      return typeof v === "number" && v >= 1 && v <= 400;
    case "detune":
      return typeof v === "number" && v >= -200 && v <= 200;
    case "root":
      return Number.isInteger(v) && v >= 0 && v <= 127;
    case "seed":
      return Number.isInteger(v) && v >= 0 && v <= 0xffffffff;
    case "volume":
      return typeof v === "number" && v >= 0 && v <= 1;
    case "paused":
      return typeof v === "boolean";
    case "scale":
      return typeof v === "string" && v.length <= 48;
    default:
      return false;
  }
}

export function startRelay({ port = 8787, key = process.env.RELAY_KEY } = {}) {
  const rooms = new Map();
  const getRoom = (id) => {
    let r = rooms.get(id);
    if (!r) {
      r = { clients: new Set(), state: {} };
      rooms.set(id, r);
    }
    return r;
  };

  const http = createServer(async (req, res) => {
    const path = new URL(req.url, "http://x").pathname;
    if (path === "/health") {
      res.writeHead(200);
      res.end("ok");
      return;
    }
    if (path === "/" || path === "/control") {
      try {
        res.writeHead(200, { "content-type": "text/html" });
        res.end(await readFile(CONTROL_HTML));
      } catch {
        res.writeHead(404);
        res.end("control.html not found");
      }
      return;
    }
    res.writeHead(404);
    res.end();
  });

  const wss = new WebSocketServer({ noServer: true, maxPayload: LIMITS.maxMsgBytes });
  http.on("upgrade", (req, socket, head) => {
    const m = new URL(req.url, "http://x").pathname.match(/^\/room\/([\w-]{1,64})$/);
    if (!m) {
      socket.destroy();
      return;
    }
    const room = getRoom(m[1]);
    if (room.clients.size >= LIMITS.maxSocketsPerRoom) {
      socket.destroy();
      return;
    }
    wss.handleUpgrade(req, socket, head, (ws) => {
      ws._authed = false;
      ws._times = [];
      room.clients.add(ws);
      ws.send(JSON.stringify({ t: "state", state: room.state }));
      ws.on("message", (data) => {
        const now = Date.now();
        ws._times = ws._times.filter((t) => now - t < 1000);
        ws._times.push(now);
        if (ws._times.length > LIMITS.maxMsgPerSec) return;

        let msg;
        try {
          msg = JSON.parse(data.toString());
        } catch {
          return;
        }
        if (msg.t === "auth") {
          ws._authed = typeof key === "string" && key.length > 0 && msg.key === key;
          ws.send(JSON.stringify({ t: "auth", ok: ws._authed }));
          return;
        }
        if (msg.t === "set" && typeof msg.k === "string") {
          if (!ws._authed) {
            ws.send(JSON.stringify({ t: "error", e: "unauthorized" }));
            return;
          }
          if (!ALLOWED_KEYS.has(msg.k) || !validParam(msg.k, msg.v)) return;
          room.state[msg.k] = msg.v;
          const out = JSON.stringify({ t: "set", k: msg.k, v: msg.v });
          for (const c of room.clients) {
            if (c !== ws && c.readyState === 1) c.send(out);
          }
        }
      });
      const drop = () => room.clients.delete(ws);
      ws.on("close", drop);
      ws.on("error", drop);
    });
  });

  return new Promise((resolve) => {
    http.listen(port, () =>
      resolve({
        port: http.address().port,
        rooms,
        close: () =>
          new Promise((r) => {
            wss.close();
            http.close(() => r());
          }),
      }),
    );
  });
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  if (!process.env.RELAY_KEY) {
    console.warn("warning: RELAY_KEY not set — control is locked. Set RELAY_KEY to enable performers.");
  }
  const { port } = await startRelay({ port: parseInt(process.argv[2] ?? "8787", 10) });
  console.log(`relay on http://localhost:${port}  (control UI: /, sockets: /room/<id>)`);
}
