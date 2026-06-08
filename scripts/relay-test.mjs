// Headless security + transport test for the performance relay.
//
// Verifies auth (control requires the key), abuse guards (param whitelist +
// ranges), broadcast, and late-joiner state replay. Exits non-zero on failure.
// Run: `node scripts/relay-test.mjs`.

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

  // 5) Late joiner gets only the accumulated valid state.
  const late = new WebSocket(base);
  await open(late);
  const state = (await wait(late, (m) => m.t === "state")).state;
  check("late joiner state is valid", state.bpm === 120 && state.evil === undefined);

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
