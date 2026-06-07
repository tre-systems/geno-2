export default {
  async fetch(request, env) {
    const response = await env.ASSETS.fetch(request);

    if (!response.ok) return response;

    const res = new Response(response.body, response);

    const url = new URL(request.url);
    const path = url.pathname;
    const versioned = url.searchParams.has("v");

    if (versioned && (path.endsWith(".wasm") || path.endsWith(".js"))) {
      // The JS glue and wasm are loaded with a ?v=<git-sha> tag that changes
      // every deploy, so each version is immutable and safe to cache forever.
      res.headers.set("Cache-Control", "public, max-age=31536000, immutable");
    } else if (path.endsWith("env.js") || path === "/" || path.endsWith(".html")) {
      // The version pointer (env.js) and the HTML entry must always revalidate
      // so a new deploy is picked up immediately.
      res.headers.set("Cache-Control", "no-cache");
    }

    return res;
  },
};
