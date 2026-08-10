const puppeteer = require("puppeteer");

const TARGET_URL = process.env.TEST_URL || "http://localhost:8080";

async function gotoWithRetry(
  page,
  url,
  totalTimeoutMs = 15000,
  intervalMs = 300
) {
  const start = Date.now();
  for (;;) {
    try {
      await page.goto(url, { waitUntil: "networkidle2", timeout: 5000 });
      return;
    } catch (_) {
      if (Date.now() - start > totalTimeoutMs) {
        throw new Error(
          `Server did not become ready at ${url} within ${totalTimeoutMs}ms`
        );
      }
      await new Promise((r) => setTimeout(r, intervalMs));
    }
  }
}

(async () => {
  const browser = await puppeteer.launch({
    headless: "new",
    args: [
      // CI-friendly flags: disable sandbox and GPU to avoid kernel/AppArmor limits
      "--no-sandbox",
      "--disable-setuid-sandbox",
      "--disable-dev-shm-usage",
      "--disable-gpu",
      // keep WebGPU feature flag harmlessly; test does not require GPU
      "--enable-unsafe-webgpu",
    ],
  });

  const page = await browser.newPage();
  const logs = [];
  const runtimeErrors = [];

  page.on("console", (m) => {
    const t = m.text();
    logs.push(t);
    console.log("[console]", t);
  });
  page.on("pageerror", (error) => {
    runtimeErrors.push(error.message);
    console.error("[pageerror]", error.message);
  });

  await gotoWithRetry(page, TARGET_URL);

  const discoverability = await page.evaluate(async () => {
    const meta = (selector) => document.querySelector(selector)?.getAttribute("content") || "";
    const canonical = document.querySelector('link[rel="canonical"]')?.href || "";
    const [robotsResponse, sitemapResponse, previewResponse] = await Promise.all([
      fetch("/robots.txt"),
      fetch("/sitemap.xml"),
      fetch("/social-preview.png"),
    ]);
    return {
      description: meta('meta[name="description"]'),
      canonical,
      ogTitle: meta('meta[property="og:title"]'),
      ogDescription: meta('meta[property="og:description"]'),
      ogImage: meta('meta[property="og:image"]'),
      twitterCard: meta('meta[name="twitter:card"]'),
      robotsStatus: robotsResponse.status,
      robotsType: robotsResponse.headers.get("content-type") || "",
      robotsBody: await robotsResponse.text(),
      sitemapStatus: sitemapResponse.status,
      sitemapType: sitemapResponse.headers.get("content-type") || "",
      sitemapBody: await sitemapResponse.text(),
      previewStatus: previewResponse.status,
      previewType: previewResponse.headers.get("content-type") || "",
    };
  });

  if (!discoverability.description) throw new Error("missing meta description");
  if (discoverability.canonical !== "https://geno-2.tre.systems/")
    throw new Error(`unexpected canonical URL: ${discoverability.canonical}`);
  if (!discoverability.ogTitle || !discoverability.ogDescription)
    throw new Error("missing Open Graph metadata");
  if (discoverability.ogImage !== "https://geno-2.tre.systems/social-preview.png")
    throw new Error(`unexpected Open Graph image: ${discoverability.ogImage}`);
  if (discoverability.twitterCard !== "summary_large_image")
    throw new Error("missing Twitter card metadata");
  if (
    discoverability.robotsStatus !== 200 ||
    !discoverability.robotsType.includes("text/plain") ||
    !discoverability.robotsBody.includes("Sitemap: https://geno-2.tre.systems/sitemap.xml")
  )
    throw new Error("robots.txt is missing or invalid");
  if (
    discoverability.sitemapStatus !== 200 ||
    !discoverability.sitemapType.includes("xml") ||
    !discoverability.sitemapBody.includes("<loc>https://geno-2.tre.systems/</loc>")
  )
    throw new Error("sitemap.xml is missing or invalid");
  if (
    discoverability.previewStatus !== 200 ||
    !discoverability.previewType.includes("image/png")
  )
    throw new Error("social preview image is missing or invalid");

  await page.waitForSelector("#app-canvas", { timeout: 10000 });

  const box = await page.$eval("#app-canvas", (el) => {
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  });

  await page.waitForSelector("#display-start", { timeout: 10000 });

  const overlayInitiallyHidden = await page.evaluate(() => {
    const el = document.getElementById("start-overlay");
    if (!el) return "missing";
    const style = el.getAttribute("style") || "";
    const byStyle = /display:\s*none/.test(style);
    const byClass = el.classList.contains("hidden");
    return byStyle || byClass ? "hidden" : "visible";
  });

  if (overlayInitiallyHidden !== "hidden")
    throw new Error("instrument did not start with the help overlay hidden");

  await page.mouse.click(box.x, box.y);
  await new Promise((r) => setTimeout(r, 400));

  const displayStartRemoved = await page.$("#display-start");
  if (displayStartRemoved) throw new Error("tap-to-start overlay did not hide");

  // Help overlay should still be available with 'H'
  const overlayInitially = await page.$("#start-overlay");
  if (!overlayInitially) throw new Error("start overlay not found");

  await page.keyboard.press("KeyH");
  await new Promise((r) => setTimeout(r, 200));

  const overlayShownAfterH = await page.evaluate(() => {
    const el = document.getElementById("start-overlay");
    if (!el) return "missing";
    const style = el.getAttribute("style") || "";
    const byStyle = /display:\s*none/.test(style);
    const byClass = el.classList.contains("hidden");
    return byStyle || byClass ? "hidden" : "visible";
  });

  if (overlayShownAfterH !== "visible")
    throw new Error("start overlay did not show after H");

  // Click close to hide
  await page.click("#overlay-close");
  await new Promise((r) => setTimeout(r, 200));

  const overlayHidden = await page.evaluate(() => {
    const el = document.getElementById("start-overlay");
    if (!el) return "missing";
    const style = el.getAttribute("style") || "";
    const byStyle = /display:\s*none/.test(style);
    const byClass = el.classList.contains("hidden");
    return byStyle || byClass ? "hidden" : "visible";
  });

  if (overlayHidden !== "hidden")
    throw new Error("start overlay did not hide after close");

  // Press H to show again
  await page.keyboard.press("KeyH");
  await new Promise((r) => setTimeout(r, 200));

  const overlayShown = await page.evaluate(() => {
    const el = document.getElementById("start-overlay");
    if (!el) return "missing";
    const style = el.getAttribute("style") || "";
    const byStyle = /display:\s*none/.test(style);
    const byClass = el.classList.contains("hidden");
    return byStyle || byClass ? "hidden" : "visible";
  });

  if (overlayShown !== "visible")
    throw new Error("start overlay did not show after H");

  // Close again so canvas pointer handlers receive the interaction tests
  await page.click("#overlay-ok");
  await new Promise((r) => setTimeout(r, 150));

  // Engine-dependent checks (only if engine handlers are bound)
  const engineStarted = logs.some((l) => l.includes("[engine] voices="));

  if (engineStarted) {
    // Reseed all
    await page.keyboard.press("KeyR");
    await new Promise((r) => setTimeout(r, 120));
    if (!logs.some((l) => l.includes("[keys] reseeded all voices")))
      throw new Error("missing reseed log");

    // Pause and resume
    await page.keyboard.press("Space");
    await new Promise((r) => setTimeout(r, 120));
    await page.keyboard.press("Space");
    await new Promise((r) => setTimeout(r, 120));

    const sawPause =
      logs.some((l) => l.includes("[keys] paused=true")) &&
      logs.some((l) => l.includes("[keys] paused=false"));
    if (!sawPause) throw new Error("missing pause/resume logs");

    // Tempo up/down (logs only)
    await page.keyboard.down("Shift");
    await page.keyboard.press("Equal");
    await page.keyboard.up("Shift");
    await new Promise((r) => setTimeout(r, 120));
    await page.keyboard.press("Minus");
    await new Promise((r) => setTimeout(r, 120));

    // Master mute toggle (logs only)
    await page.keyboard.press("KeyM");
    await new Promise((r) => setTimeout(r, 120));

    if (!logs.some((l) => /\[keys\] master muted=true/.test(l)))
      throw new Error("missing master mute= true log");

    await page.keyboard.press("KeyM");
    await new Promise((r) => setTimeout(r, 120));

    if (!logs.some((l) => /\[keys\] master muted=false/.test(l)))
      throw new Error("missing master mute= false log");

    // Click center to trigger a gesture flare
    await page.mouse.move(box.x, box.y);
    await page.mouse.click(box.x, box.y);
    await new Promise((r) => setTimeout(r, 120));
    if (!logs.some((l) => /\[gesture\] flare uv=\([0-9.]+,[0-9.]+\)/.test(l)))
      throw new Error("missing gesture flare log");

    // Drag carve should start and commit a carve drop on release
    await page.mouse.move(box.x - 90, box.y + 70);
    await page.mouse.down();
    await new Promise((r) => setTimeout(r, 50));
    await page.mouse.move(box.x + 130, box.y - 120, { steps: 16 });
    await new Promise((r) => setTimeout(r, 50));
    await page.mouse.up();
    await new Promise((r) => setTimeout(r, 120));

    if (!logs.some((l) => l.includes("[gesture] carve begin")))
      throw new Error("missing gesture carve begin log");
    if (
      !logs.some((l) =>
        /\[gesture\] carve drop root=\d+ mode=.* travel=[0-9.]+px spin=-?[0-9.]+/.test(
          l
        )
      )
    )
      throw new Error("missing gesture carve drop log");

    // Test G key support (new functionality)
    await page.keyboard.press("KeyG");
    await new Promise((r) => setTimeout(r, 120));

    // Test root note changes A-G
    const rootKeys = ["KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG"];
    for (const key of rootKeys) {
      await page.keyboard.press(key);
      await new Promise((r) => setTimeout(r, 50));
    }

    // Test mode changes 1-7
    const modeKeys = [
      "Digit1",
      "Digit2",
      "Digit3",
      "Digit4",
      "Digit5",
      "Digit6",
      "Digit7",
    ];
    for (const key of modeKeys) {
      await page.keyboard.press(key);
      await new Promise((r) => setTimeout(r, 50));
    }

    // Test random key+mode (T key)
    await page.keyboard.press("KeyT");
    await new Promise((r) => setTimeout(r, 120));
  } else {
    console.log("[note] audio engine unavailable; checking explicit degraded state");
  }

  const renderState = await page.evaluate(() => ({
    audioErrorVisible:
      getComputedStyle(document.getElementById("audio-error")).display !== "none",
    webGpuErrorVisible:
      getComputedStyle(document.getElementById("no-webgpu")).display !== "none",
  }));
  const gpuInitialized = logs.some((line) =>
    line.includes("WebGPU initialized successfully")
  );

  if (!engineStarted && !renderState.audioErrorVisible)
    throw new Error("audio engine neither initialized nor showed its error state");
  if (!gpuInitialized && !renderState.webGpuErrorVisible)
    throw new Error("renderer neither initialized nor showed its WebGPU error state");

  // Animation timing is meaningful only when the actual renderer started.
  if (gpuInitialized) {
    console.log("[perf] measuring frame rate performance...");

    const perfMetrics = await page.evaluate(() => {
      return new Promise((resolve) => {
        let frameCount = 0;
        let startTime = performance.now();
        let minFrameTime = Infinity;
        let maxFrameTime = 0;
        let frameTimes = [];

        function measureFrame() {
          const currentTime = performance.now();
          const frameTime = currentTime - startTime;

          if (frameCount > 0) {
            // Skip first frame
            frameTimes.push(frameTime);
            minFrameTime = Math.min(minFrameTime, frameTime);
            maxFrameTime = Math.max(maxFrameTime, frameTime);
          }

          frameCount++;
          startTime = currentTime;

          if (frameCount < 60) {
            // Measure 60 frames (~1 second at 60fps)
            requestAnimationFrame(measureFrame);
          } else {
            const avgFrameTime =
              frameTimes.reduce((a, b) => a + b, 0) / frameTimes.length;
            const avgFPS = 1000 / avgFrameTime;
            const minFPS = 1000 / maxFrameTime;
            const maxFPS = 1000 / minFrameTime;

            resolve({
              avgFPS: Math.round(avgFPS * 10) / 10,
              minFPS: Math.round(minFPS * 10) / 10,
              maxFPS: Math.round(maxFPS * 10) / 10,
              frameCount: frameTimes.length,
            });
          }
        }

        requestAnimationFrame(measureFrame);
      });
    });

    console.log(`[perf] Average FPS: ${perfMetrics.avgFPS}`);
    console.log(`[perf] Min FPS: ${perfMetrics.minFPS}`);
    console.log(`[perf] Max FPS: ${perfMetrics.maxFPS}`);
    console.log(`[perf] Measured ${perfMetrics.frameCount} frames`);

    // Warn if performance is concerning (but don't fail CI)
    if (perfMetrics.avgFPS < 30) {
      console.warn(
        `[perf] WARNING: Average FPS (${perfMetrics.avgFPS}) is below 30fps`
      );
    }
    if (perfMetrics.minFPS < 15) {
      console.warn(
        `[perf] WARNING: Minimum FPS (${perfMetrics.minFPS}) dropped below 15fps`
      );
    }
  }

  // Basic assertions
  const hasWebGPU = await page.evaluate(() => !!navigator.gpu);
  console.log("WEBGPU", hasWebGPU);

  const controlPage = await browser.newPage();
  controlPage.on("pageerror", (error) => {
    runtimeErrors.push(`control: ${error.message}`);
    console.error("[control pageerror]", error.message);
  });
  await gotoWithRetry(controlPage, new URL("/control/", TARGET_URL).href);
  await controlPage.waitForSelector("#panel-code", { timeout: 10000 });
  const controlState = await controlPage.evaluate(() => ({
    code: document.getElementById("panel-code")?.textContent || "",
    controlsDisabled: document.getElementById("controls")?.disabled,
    title: document.title,
  }));
  if (!/^\d{6}$/.test(controlState.code))
    throw new Error(`control panel generated an invalid pairing code: ${controlState.code}`);
  if (controlState.controlsDisabled !== true)
    throw new Error("unpaired control panel should keep its controls disabled");
  if (controlState.title !== "Geno-2 — Control")
    throw new Error(`unexpected control panel title: ${controlState.title}`);
  await controlPage.close();

  if (runtimeErrors.length)
    throw new Error(`browser runtime errors:\n- ${runtimeErrors.join("\n- ")}`);

  await browser.close();

  process.exit(0);
})().catch((err) => {
  console.error("TEST_ERROR", err);
  process.exit(1);
});
