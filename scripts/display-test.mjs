// End-to-end test for the instrument surface + separate control panel.
//
// Loads the real app, opens /control, pairs the panel by code through the
// instrument help screen, drives the real panel, and verifies the live engine
// state changed (via control_get_state).
// Requires a prior `npm run build`.
// Run: `node scripts/display-test.mjs`.

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
      const file = p === "/" ? "/index.html" : p === "/control" ? "/control.html" : p;
      const body = await readFile(join(dist, file));
      res.writeHead(200, { "content-type": MIME[extname(file)] ?? "application/octet-stream" });
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

const stat = await staticServer();
const browser = await puppeteer.launch({ headless: true, args: ["--no-sandbox", "--enable-unsafe-webgpu"] });

try {
  const surface = await browser.newPage();
  surface.on("console", (m) => {
    const t = m.text();
    if (t.toLowerCase().includes("panic")) console.log("[surface]", t);
  });
  surface.on("pageerror", (e) => console.error("[surface pageerror]", e.message));

  const url = `http://localhost:${stat.port}/`;
  await surface.goto(url, { waitUntil: "load" });
  await surface.waitForFunction(
    "window.__geno && window.__geno.control_ready && window.__geno.control_ready() === true",
    { timeout: 30000 },
  );
  check("surface ready (engine installed)", true);

  const control = await browser.newPage();
  control.on("pageerror", (e) => console.error("[control pageerror]", e.message));
  await control.goto(`http://localhost:${stat.port}/control.html`, { waitUntil: "load" });
  const panelCode = await control.$eval("#panel-code", (el) => el.textContent.trim());
  await surface.keyboard.press("KeyH");
  await surface.waitForFunction(
    `(() => {
      const el = document.getElementById("start-overlay");
      if (!el) return false;
      const style = el.getAttribute("style") || "";
      return !/display:\\s*none/.test(style) && !el.classList.contains("hidden");
    })()`,
    { timeout: 5000 },
  );
  await surface.$eval("#control-code", (el, code) => {
    el.value = code;
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }, panelCode);
  const connectBox = await surface.$eval("#control-connect", (el) => {
    el.scrollIntoView({ block: "center", inline: "center" });
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  });
  await surface.mouse.click(connectBox.x, connectBox.y);
  await control.waitForFunction(
    "document.getElementById('controls') && document.getElementById('controls').disabled === false",
    { timeout: 10000 },
  );
  check("control panel linked by code", true);

  await control.$eval("#bpm", (el) => {
    el.value = "123";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await control.select("#root", "67");
  await control.select("#scale", "Lydian");
  await control.$eval("#seed", (el) => {
    el.value = "777";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });

  // Poll the live engine state until it reflects the local control panel.
  let state = {};
  for (let i = 0; i < 50; i++) {
    state = JSON.parse(await surface.evaluate("window.__geno.control_get_state()"));
    if (state.bpm === 123 && state.root === 67 && state.scale === "Lydian") break;
    await new Promise((r) => setTimeout(r, 100));
  }
  check("bpm panel change applied to live engine", state.bpm === 123);
  check("root panel change applied to live engine", state.root === 67);
  check("scale panel change applied to live engine", state.scale === "Lydian");
} catch (e) {
  console.error(e);
  failed = true;
} finally {
  await browser.close();
  stat.close();
}

console.log(failed ? "\nFAILED" : "\nall surface/control checks passed");
process.exit(failed ? 1 : 0);
