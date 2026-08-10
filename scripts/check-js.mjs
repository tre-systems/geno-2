#!/usr/bin/env node
// Parse every repository-owned JavaScript file. worker.js is an ES module even
// though package.json keeps the Node tooling in CommonJS mode, so parse that
// source through stdin with an explicit module input type.
import { spawnSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { extname, join } from "node:path";

const files = ["sentry.js", "web-test.js", "worker.js"];
collect("scripts", files);

let failed = false;
for (const file of files.sort()) {
  const moduleInCommonJsPackage = file === "worker.js";
  const result = spawnSync(
    process.execPath,
    moduleInCommonJsPackage
      ? ["--input-type=module", "--check"]
      : ["--check", file],
    {
      encoding: "utf8",
      input: moduleInCommonJsPackage ? readFileSync(file, "utf8") : undefined,
      stdio: moduleInCommonJsPackage
        ? ["pipe", "pipe", "pipe"]
        : ["ignore", "pipe", "pipe"],
    },
  );
  if (result.status === 0) continue;
  failed = true;
  process.stderr.write(result.stdout || "");
  process.stderr.write(result.stderr || "");
}

if (failed) process.exit(1);
console.log(`check-js: ${files.length} JavaScript files parsed`);

function collect(dir, output) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) collect(path, output);
    else if (entry.isFile() && [".js", ".mjs"].includes(extname(path))) output.push(path);
  }
}
