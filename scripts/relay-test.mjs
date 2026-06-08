// Headless security + transport test for the performance relay.
//
// Verifies auth (control requires the key), abuse guards (param whitelist +
// ranges), broadcast, late-joiner state replay, and the transient gesture
// channel ({t:"ev"}: authed-only, broadcast, not persisted). Exits non-zero on
// failure. Run: `node scripts/relay-test.mjs`.

import { startRelay } from "./relay.mjs";

const KEY = "test-secret";
const open = (ws) => new Promise((r) => ws.addEventListener("open", r, { once: true }));
const wait = (ws, pred) =>
  new Promise((resolve, reject) => {
    const to = setTimeout(() => reject(new Error("timeout")), 4000);
    ws.addEventListener("message", (ev) => {
      const msg = JSON.parse(ev.data);
      if (pred(msg)) {
        clearTimeout(to);
        resolve(msg);
      }
    });
  });
const never = (ws, pred, ms = 400) =>
  new Promise((resolve) => {
    let hit = false;
    const on = (ev) => {
      if (pred(JSON.parse(ev.data))) hit = true;
    };
    ws.addEventListener("message", on);
    setTimeout(() => {
      ws.removeEventListener("message", on);
      resolve(!hit);
    }, ms);
  });

let failed = false;
const check = (name, ok) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${name}`);
  if (!ok) failed = true;
};

const relay = await startRelay({ port: 0, key: KEY });
const base = `ws://localhost:${relay.port}/room/test`;

try {
  const control = new WebSocket(base);
  const display = new WebSocket(base);
  await Promise.all([open(control), open(display)]);
  await wait(display, (m) => m.t === "state");

  // 1) Unauthenticated control cannot drive the room.
  const noBroadcast = never(display, (m) => m.t === "set");
  const gotErr = wait(control, (m) => m.t === "error");
  control.send(JSON.stringify({ t: "set", k: "bpm", v: 120 }));
  check("unauthenticated set is rejected", (await gotErr).e === "unauthorized");
  check("unauthenticated set is not broadcast", await noBroadcast);

  // 2) Wrong key rejected, correct key accepted.
  const wrong = wait(control, (m) => m.t === "auth");
  control.send(JSON.stringify({ t: "auth", key: "nope" }));
  check("wrong key is rejected", (await wrong).ok === false);

  const right = wait(control, (m) => m.t === "auth");
  control.send(JSON.stringify({ t: "auth", key: KEY }));
  check("correct key authenticates", (await right).ok === true);

  // 3) Authenticated, valid set is broadcast.
  const gotBpm = wait(display, (m) => m.t === "set" && m.k === "bpm");
  control.send(JSON.stringify({ t: "set", k: "bpm", v: 120 }));
  check("authenticated set is broadcast", (await gotBpm).v === 120);

  // 4) Invalid params (out of range / not whitelisted) are dropped.
  const dropped = never(display, (m) => m.t === "set" && (m.k === "evil" || m.v === 9999));
  control.send(JSON.stringify({ t: "set", k: "bpm", v: 9999 }));
  control.send(JSON.stringify({ t: "set", k: "evil", v: 1 }));
  check("invalid params are dropped", await dropped);

  // 5) Authenticated gesture events broadcast to peers.
  const gotEv = wait(display, (m) => m.t === "ev" && m.e === "flare");
  control.send(JSON.stringify({ t: "ev", e: "flare", u: 0.5, v: 0.4 }));
  const ev = await gotEv;
  check("authenticated ev is broadcast", ev.e === "flare" && ev.u === 0.5 && ev.v === 0.4);

  // 6) Invalid gesture events (bad kind / out-of-range uv) are dropped.
  const evDropped = never(display, (m) => m.t === "ev" && (m.e === "evil" || m.u === 5));
  control.send(JSON.stringify({ t: "ev", e: "evil", u: 0.5, v: 0.5 }));
  control.send(JSON.stringify({ t: "ev", e: "flare", u: 5, v: 0.5 }));
  check("invalid ev is dropped", await evDropped);

  // 7) Unauthenticated sockets cannot send gesture events.
  const viewer = new WebSocket(base);
  await open(viewer);
  await wait(viewer, (m) => m.t === "state");
  const evUnauth = wait(viewer, (m) => m.t === "error");
  const evNoBroadcast = never(display, (m) => m.t === "ev" && m.e === "ripple");
  viewer.send(JSON.stringify({ t: "ev", e: "ripple", u: 0.2, v: 0.2 }));
  check("unauthenticated ev is rejected", (await evUnauth).e === "unauthorized");
  check("unauthenticated ev is not broadcast", await evNoBroadcast);
  viewer.close();

  // 8) Late joiner gets the accumulated valid state — no ev replay, no junk.
  const late = new WebSocket(base);
  await open(late);
  const noEvReplay = never(late, (m) => m.t === "ev");
  const state = (await wait(late, (m) => m.t === "state")).state;
  check("late joiner state is valid", state.bpm === 120 && state.evil === undefined);
  check("ev is not replayed to late joiners", await noEvReplay);

  control.close();
  display.close();
  late.close();
} catch (e) {
  console.error(e);
  failed = true;
} finally {
  await relay.close();
}

console.log(failed ? "\nFAILED" : "\nall relay security checks passed");
process.exit(failed ? 1 : 0);
