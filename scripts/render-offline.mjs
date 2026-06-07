// Headless offline renderer for geno-2.
//
// Drives the real WASM instrument under an OfflineAudioContext in headless
// Chrome and writes a deterministic stereo 32-bit-float WAV. Run `npm run build`
// first so `dist/pkg` exists.
//
//   node scripts/render-offline.mjs <seed> <duration_sec> <out.wav>

import { createServer } from "node:http";
import { readFile, writeFile, copyFile, stat } from "node:fs/promises";
import { join, extname, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import puppeteer from "puppeteer";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const dist = join(root, "dist");

const seed = parseInt(process.argv[2] ?? "12345", 10);
const duration = parseFloat(process.argv[3] ?? "8");
const outPath = process.argv[4] ?? join(root, `render_${seed}.wav`);
const sampleRate = 48000;

const MIME = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".wasm": "application/wasm",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".css": "text/css",
};

async function main() {
  try {
    await stat(join(dist, "pkg", "app_web.js"));
  } catch {
    console.error("dist/ not built — run `npm run build` first.");
    process.exit(1);
  }
  await copyFile(join(root, "offline.html"), join(dist, "offline.html"));

  const server = createServer(async (req, res) => {
    try {
      let p = decodeURIComponent(new URL(req.url, "http://x").pathname);
      if (p === "/") p = "/offline.html";
      const body = await readFile(join(dist, p));
      res.writeHead(200, {
        "content-type": MIME[extname(p)] ?? "application/octet-stream",
      });
      res.end(body);
    } catch {
      res.writeHead(404);
      res.end("not found");
    }
  });
  await new Promise((r) => server.listen(0, r));
  const port = server.address().port;
  const url = `http://localhost:${port}/offline.html`;

  const browser = await puppeteer.launch({
    headless: true,
    args: ["--no-sandbox", "--enable-unsafe-webgpu"],
  });
  try {
    const page = await browser.newPage();
    page.on("console", (m) => console.log("[page]", m.text()));
    page.on("pageerror", (e) => console.error("[pageerror]", e.message));
    await page.goto(url, { waitUntil: "load" });
    await page.waitForFunction("window.__offlineReady === true", {
      timeout: 30000,
    });

    console.log(`rendering seed=${seed} duration=${duration}s @ ${sampleRate}Hz ...`);
    const t0 = Date.now();
    const result = await page.evaluate(
      async (seed, duration, sr) => {
        const u8 = await window.__renderAudioWav(seed, duration, sr);
        const dv = new DataView(u8.buffer, u8.byteOffset, u8.byteLength);
        let peak = 0,
          sumSq = 0,
          n = 0;
        for (let off = 44; off + 4 <= u8.byteLength; off += 4) {
          const s = dv.getFloat32(off, true);
          const a = Math.abs(s);
          if (a > peak) peak = a;
          sumSq += s * s;
          n++;
        }
        let bin = "";
        const chunk = 0x8000;
        for (let i = 0; i < u8.length; i += chunk) {
          bin += String.fromCharCode.apply(null, u8.subarray(i, i + chunk));
        }
        return { b64: btoa(bin), bytes: u8.length, peak, rms: Math.sqrt(sumSq / Math.max(1, n)) };
      },
      seed,
      duration,
      sampleRate,
    );

    await writeFile(outPath, Buffer.from(result.b64, "base64"));
    console.log(
      `wrote ${outPath} (${result.bytes} bytes) in ${Date.now() - t0}ms — peak=${result.peak.toFixed(4)} rms=${result.rms.toFixed(4)}`,
    );
    if (result.peak < 0.001) {
      console.error("WARNING: render appears silent");
      process.exitCode = 2;
    }
  } finally {
    await browser.close();
    server.close();
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
