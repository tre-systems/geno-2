// Verifies the Cloudflare relay (Room Durable Object) under `wrangler dev`.
//
// Spawns wrangler dev locally, then checks that (a) the static app still serves
// and (b) the Durable Object broadcasts params + replays state to late joiners.
// Requires a prior `npm run build`. Run: `node scripts/cf-relay-test.mjs`.

import { spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";
import { WebSocket } from "ws";

const PORT = 8799;
const wrangler = spawn(
  "npx",
  [
    "wrangler",
    "dev",
    "--port",
    String(PORT),
    "--var",
    "RELAY_ENABLED:true",
    "--var",
    "RELAY_KEY:test-secret",
  ],
  {
    stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, WRANGLER_SEND_METRICS: "false", CI: "1" },
  },
);
const log = (d) => process.stderr.write("[wrangler] " + d);
wrangler.stdout.on("data", log);
wrangler.stderr.on("data", log);

let failed = false;
const check = (n, ok) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${n}`);
  if (!ok) failed = true;
};
const open = (ws) =>
  new Promise((res, rej) => {
    ws.once("open", res);
    ws.once("error", () => rej(new Error("ws error")));
  });
const waitMsg = (ws, pred) =>
  new Promise((res, rej) => {
    const to = setTimeout(() => rej(new Error("timeout")), 5000);
    const on = (data) => {
      const m = JSON.parse(data.toString());
      if (pred(m)) {
        clearTimeout(to);
        ws.off("message", on);
        res(m);
      }
    };
    ws.on("message", on);
  });
async function waitHttp(url, ms) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) {
    try {
      const r = await fetch(url);
      if (r.ok || r.status === 404 || r.status === 426) return true;
    } catch {}
    await sleep(500);
  }
  return false;
}

try {
  const up = await waitHttp(`http://localhost:${PORT}/`, 90000);
  check("wrangler dev serving", up);
  if (up) {
    const home = await fetch(`http://localhost:${PORT}/`);
    check("static app still served (GET /)", home.ok && (await home.text()).includes("app-canvas"));

    const base = `ws://localhost:${PORT}/room/test`;
    const control = new WebSocket(base);
    const display = new WebSocket(base);
    const displayState = waitMsg(display, (m) => m.t === "state");
    await Promise.all([open(control), open(display)]);
    await displayState;

    const authed = waitMsg(control, (m) => m.t === "auth");
    control.send(JSON.stringify({ t: "auth", key: "test-secret" }));
    check("control authenticates with key", (await authed).ok === true);

    const got = waitMsg(display, (m) => m.t === "set" && m.k === "bpm");
    control.send(JSON.stringify({ t: "set", k: "bpm", v: 140 }));
    check("DO broadcasts param to display", (await got).v === 140);

    const late = new WebSocket(base);
    const lateState = waitMsg(late, (m) => m.t === "state");
    await open(late);
    const st = (await lateState).state;
    check("DO replays state to late joiner", st.bpm === 140);

    control.close();
    display.close();
    late.close();
  }
} catch (e) {
  console.error(e);
  failed = true;
} finally {
  wrangler.kill("SIGTERM");
  await sleep(800);
  try {
    wrangler.kill("SIGKILL");
  } catch {}
}

console.log(failed ? "\nFAILED" : "\nall Cloudflare relay checks passed");
process.exit(failed ? 1 : 0);
