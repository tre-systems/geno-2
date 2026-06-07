// End-to-end test for display mode.
//
// Loads the real app in ?mode=display against an in-process relay, broadcasts
// parameters from a performer socket, and verifies the *live engine state*
// changed (via control_get_state). Requires a prior `npm run build`.
// Run: `node scripts/display-test.mjs`.

import { startRelay } from "./relay.mjs";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { join, extname, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import puppeteer from "puppeteer";

const dist = join(dirname(fileURLToPath(import.meta.url)), "..", "dist");
const MIME = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".wasm": "application/wasm",
  ".json": "application/json",
  ".svg": "image/svg+xml",
};

function staticServer() {
  const s = createServer(async (req, res) => {
    try {
      const p = new URL(req.url, "http://x").pathname;
      const body = await readFile(join(dist, p === "/" ? "/index.html" : p));
      res.writeHead(200, { "content-type": MIME[extname(p)] ?? "application/octet-stream" });
      res.end(body);
    } catch {
      res.writeHead(404);
      res.end();
    }
  });
  return new Promise((r) => s.listen(0, () => r({ port: s.address().port, close: () => s.close() })));
}

let failed = false;
const check = (n, ok) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${n}`);
  if (!ok) failed = true;
};

const relay = await startRelay({ port: 0 });
const stat = await staticServer();
const browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox", "--enable-unsafe-webgpu"] });

try {
  const page = await browser.newPage();
  page.on("console", (m) => {
    const t = m.text();
    if (t.includes("[display]") || t.toLowerCase().includes("panic")) console.log("[page]", t);
  });
  page.on("pageerror", (e) => console.error("[pageerror]", e.message));

  const url = `http://localhost:${stat.port}/index.html?mode=display&room=test&relay=ws://localhost:${relay.port}`;
  await page.goto(url, { waitUntil: "load" });
  await page.waitForFunction(
    "window.__geno && window.__geno.control_ready && window.__geno.control_ready() === true",
    { timeout: 30000 },
  );
  check("display client ready (engine installed)", true);

  // Performer control socket on the same room.
  const control = new WebSocket(`ws://localhost:${relay.port}/room/test`);
  await new Promise((res, rej) => {
    control.addEventListener("open", res, { once: true });
    control.addEventListener("error", () => rej(new Error("control ws error")), { once: true });
  });
  for (const [k, v] of [["bpm", 123], ["root", 67], ["scale", "Lydian"], ["seed", 777]]) {
    control.send(JSON.stringify({ t: "set", k, v }));
  }

  // Poll the live engine state until it reflects the broadcast.
  let state = {};
  for (let i = 0; i < 50; i++) {
    state = JSON.parse(await page.evaluate("window.__geno.control_get_state()"));
    if (state.bpm === 123 && state.root === 67 && state.scale === "Lydian") break;
    await new Promise((r) => setTimeout(r, 100));
  }
  check("bpm broadcast applied to live engine", state.bpm === 123);
  check("root broadcast applied to live engine", state.root === 67);
  check("scale broadcast applied to live engine", state.scale === "Lydian");
  control.close();
} catch (e) {
  console.error(e);
  failed = true;
} finally {
  await browser.close();
  stat.close();
  await relay.close();
}

console.log(failed ? "\nFAILED" : "\nall display checks passed");
process.exit(failed ? 1 : 0);
