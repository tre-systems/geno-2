// Minimal WebSocket relay for networked performance.
//
// One "room" per id. A performer (the control panel) sends {t:"set",k,v}
// messages; the relay records the latest value per key and broadcasts each
// change to every *other* client in the room. New clients receive the current
// accumulated state on connect, so late joiners (an iPad, an audience phone)
// catch up to whatever the performer has already set.
//
// This is the local/dev relay; the same broadcast + state logic ports directly
// to a Cloudflare Durable Object for production.
//
//   node scripts/relay.mjs [port]      # control UI at /, sockets at /room/<id>

import { WebSocketServer } from "ws";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const CONTROL_HTML = join(here, "..", "control.html");

export function startRelay({ port = 8787 } = {}) {
  const rooms = new Map(); // id -> { clients:Set<ws>, state:Object }
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

  const wss = new WebSocketServer({ noServer: true });
  http.on("upgrade", (req, socket, head) => {
    const m = new URL(req.url, "http://x").pathname.match(/^\/room\/([\w-]{1,64})$/);
    if (!m) {
      socket.destroy();
      return;
    }
    const room = getRoom(m[1]);
    wss.handleUpgrade(req, socket, head, (ws) => {
      room.clients.add(ws);
      ws.send(JSON.stringify({ t: "state", state: room.state }));
      ws.on("message", (data) => {
        let msg;
        try {
          msg = JSON.parse(data.toString());
        } catch {
          return;
        }
        if (msg.t === "set" && typeof msg.k === "string") {
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
  const { port } = await startRelay({ port: parseInt(process.argv[2] ?? "8787", 10) });
  console.log(`relay on http://localhost:${port}  (control UI: /, sockets: /room/<id>)`);
}
