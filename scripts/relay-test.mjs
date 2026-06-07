// Headless transport test for the performance relay.
//
// Spins up the relay in-process, connects a control client and display clients,
// and verifies broadcast + late-joiner state replay + no self-echo. Exits
// non-zero on failure. Run: `node scripts/relay-test.mjs`.

import { startRelay } from "./relay.mjs";

const open = (ws) => new Promise((r) => ws.addEventListener("open", r, { once: true }));
const wait = (ws, pred) =>
  new Promise((resolve, reject) => {
    const to = setTimeout(() => reject(new Error("timeout waiting for message")), 4000);
    ws.addEventListener("message", (ev) => {
      const msg = JSON.parse(ev.data);
      if (pred(msg)) {
        clearTimeout(to);
        resolve(msg);
      }
    });
  });

let failed = false;
const check = (name, ok) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${name}`);
  if (!ok) failed = true;
};

const relay = await startRelay({ port: 0 });
const base = `ws://localhost:${relay.port}/room/test`;

try {
  const control = new WebSocket(base);
  const display = new WebSocket(base);
  await Promise.all([open(control), open(display)]);
  await wait(display, (m) => m.t === "state"); // initial (empty) state

  // 1) control sets a param; display receives the broadcast.
  const gotBpm = wait(display, (m) => m.t === "set" && m.k === "bpm");
  control.send(JSON.stringify({ t: "set", k: "bpm", v: 120 }));
  check("display receives broadcast param", (await gotBpm).v === 120);

  // 2) a second param.
  const gotScale = wait(display, (m) => m.t === "set" && m.k === "scale");
  control.send(JSON.stringify({ t: "set", k: "scale", v: "Dorian" }));
  check("display receives second param", (await gotScale).v === "Dorian");

  // 3) a late-joining display gets the accumulated state.
  const late = new WebSocket(base);
  await open(late);
  const state = (await wait(late, (m) => m.t === "state")).state;
  check("late joiner catches up: bpm", state.bpm === 120);
  check("late joiner catches up: scale", state.scale === "Dorian");

  // 4) the sender does not receive its own echo.
  let echoed = false;
  control.addEventListener("message", (ev) => {
    if (JSON.parse(ev.data).t === "set") echoed = true;
  });
  control.send(JSON.stringify({ t: "set", k: "root", v: 62 }));
  await new Promise((r) => setTimeout(r, 200));
  check("sender does not receive its own echo", echoed === false);

  control.close();
  display.close();
  late.close();
} catch (e) {
  console.error(e);
  failed = true;
} finally {
  await relay.close();
}

console.log(failed ? "\nFAILED" : "\nall transport checks passed");
process.exit(failed ? 1 : 0);
