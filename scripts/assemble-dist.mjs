#!/usr/bin/env node
// Assemble a clean Cloudflare deploy root from the checked-in web surface and
// the two wasm-pack runtime artifacts. Keep pkg/ as raw generated output so
// package metadata and TypeScript declarations cannot leak into production.
import { cpSync, existsSync, mkdirSync, rmSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const root = process.cwd();
const outDir = resolve(root, process.argv[2] || "dist");
const files = [
  ["pkg/app_web.js", "pkg/app_web.js"],
  ["pkg/app_web_bg.wasm", "pkg/app_web_bg.wasm"],
  ["pkg/env.js", "pkg/env.js"],
  ["pkg/sentry-config.js", "pkg/sentry-config.js"],
  ["sentry.js", "sentry.js"],
  ["index.html", "index.html"],
  ["control.html", "control.html"],
  ["control.html", "control/index.html"],
  ["favicon.svg", "favicon.svg"],
  ["docs/screenshot.png", "social-preview.png"],
  ["robots.txt", "robots.txt"],
  ["sitemap.xml", "sitemap.xml"],
  ["_headers", "_headers"],
];

for (const [source] of files) requireFile(join(root, source));

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

for (const [source, destination] of files) {
  const target = join(outDir, destination);
  mkdirSync(dirname(target), { recursive: true });
  cpSync(join(root, source), target);
}

console.log(`assemble-dist: wrote ${files.length} files to ${outDir}`);

function requireFile(path) {
  if (!existsSync(path) || !statSync(path).isFile()) {
    throw new Error(`assemble-dist: missing required file ${path}`);
  }
}
