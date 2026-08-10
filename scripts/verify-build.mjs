#!/usr/bin/env node
// Verify the exact deploy artifact and the policies that make versioned WASM
// releases safe to serve. This intentionally inspects dist/, not source files.
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { extname, join, resolve } from "node:path";

const outDir = resolve(process.cwd(), process.argv[2] || "dist");
const errors = [];
const requiredFiles = [
  "_headers",
  "control.html",
  "control/index.html",
  "favicon.svg",
  "index.html",
  "pkg/app_web.js",
  "pkg/app_web_bg.wasm",
  "pkg/env.js",
  "pkg/sentry-config.js",
  "robots.txt",
  "sentry.js",
  "sitemap.xml",
  "social-preview.png",
];
const forbiddenFiles = [
  "pkg/LICENSE",
  "pkg/README.md",
  "pkg/app_web.d.ts",
  "pkg/app_web_bg.wasm.d.ts",
  "pkg/package.json",
];
const textExtensions = new Set(["", ".css", ".html", ".js", ".json", ".svg", ".txt", ".xml"]);

if (!existsSync(outDir) || !statSync(outDir).isDirectory()) {
  console.error(`verify-build: deploy root does not exist: ${outDir}`);
  process.exit(1);
}

for (const file of requiredFiles) requireArtifact(file);
for (const file of forbiddenFiles) {
  if (existsSync(join(outDir, file))) errors.push(`raw wasm-pack artifact present: ${file}`);
}

for (const file of listFiles(outDir)) {
  if (file.endsWith(".map")) errors.push(`public source map present: ${relative(file)}`);
  if (!textExtensions.has(extname(file))) continue;
  const text = readFileSync(file, "utf8");
  if (/__[A-Z][A-Z0-9_]*__/.test(text)) {
    errors.push(`unstamped placeholder in ${relative(file)}`);
  }
}

const index = readText("index.html");
const control = readText("control.html");
const controlRoute = readText("control/index.html");
const headers = readText("_headers");
const env = readText("pkg/env.js");
const sentryConfig = readText("pkg/sentry-config.js");
const robots = readText("robots.txt");
const sitemap = readText("sitemap.xml");

for (const token of [
  '<link rel="canonical" href="https://geno-2.tre.systems/"',
  'content="https://geno-2.tre.systems/social-preview.png"',
  '"env": "./pkg/env.js"',
  "./pkg/app_web.js?v=${v}",
  "./pkg/app_web_bg.wasm?v=${v}",
  '<script src="/pkg/sentry-config.js"></script>',
  '<script src="/sentry.js" defer></script>',
]) {
  if (!index.includes(token)) errors.push(`index.html missing ${token}`);
}

for (const token of [
  "Content-Security-Policy:",
  "script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'",
  "object-src 'none'",
  "base-uri 'none'",
  "frame-ancestors 'none'",
  "X-Content-Type-Options: nosniff",
  "Strict-Transport-Security:",
  "/pkg/app_web.js",
  "/pkg/app_web_bg.wasm",
  "Cache-Control: public, max-age=31536000, immutable",
  "/pkg/env.js",
  "Cache-Control: no-cache",
]) {
  if (!headers.includes(token)) errors.push(`_headers missing ${token}`);
}

if (control && controlRoute && control !== controlRoute) {
  errors.push("control.html and control/index.html must be byte-identical");
}
if (!/export const version = "[^"]+";/.test(env)) {
  errors.push("pkg/env.js does not contain a stamped version");
}
for (const token of ['"app": "geno-2"', '"dsn":', '"environment":', '"release":']) {
  if (!sentryConfig.includes(token)) errors.push(`pkg/sentry-config.js missing ${token}`);
}
if (!robots.includes("Sitemap: https://geno-2.tre.systems/sitemap.xml")) {
  errors.push("robots.txt does not advertise the canonical sitemap");
}
if (!sitemap.includes("<loc>https://geno-2.tre.systems/</loc>")) {
  errors.push("sitemap.xml does not contain the canonical root URL");
}
verifyPng("social-preview.png", 902, 607);

if (errors.length) {
  console.error("verify-build failed:");
  for (const error of errors) console.error(`- ${error}`);
  process.exit(1);
}

console.log(`verify-build: ${requiredFiles.length} production artifacts and policies verified`);

function requireArtifact(file) {
  const path = join(outDir, file);
  if (!existsSync(path) || !statSync(path).isFile()) {
    errors.push(`missing required artifact: ${file}`);
  }
}

function readText(file) {
  const path = join(outDir, file);
  return existsSync(path) ? readFileSync(path, "utf8") : "";
}

function verifyPng(file, width, height) {
  const path = join(outDir, file);
  if (!existsSync(path)) return;
  const bytes = readFileSync(path);
  if (bytes.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") {
    errors.push(`${file} is not a PNG`);
    return;
  }
  const actualWidth = bytes.readUInt32BE(16);
  const actualHeight = bytes.readUInt32BE(20);
  if (actualWidth !== width || actualHeight !== height) {
    errors.push(`${file} is ${actualWidth}x${actualHeight}, expected ${width}x${height}`);
  }
}

function listFiles(dir) {
  const files = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) files.push(...listFiles(path));
    else if (entry.isFile()) files.push(path);
  }
  return files;
}

function relative(path) {
  return path.slice(outDir.length + 1);
}
