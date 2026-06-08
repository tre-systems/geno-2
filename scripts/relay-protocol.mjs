// Shared relay protocol: the parameter whitelist, validation, and limits used by
// both the Cloudflare Durable Object (worker.js) and the node dev relay
// (scripts/relay.mjs), so the two can't drift as parameters are added.

export const ALLOWED_KEYS = new Set(["bpm", "detune", "root", "scale", "seed", "paused", "volume"]);

export const LIMITS = {
  maxSocketsPerRoom: 200, // bounds broadcast fan-out per room
  maxMsgPerSec: 16, // per-socket message rate
  maxMsgBytes: 1024, // reject oversized frames
  persistThrottleMs: 3000, // min interval between Durable Object storage writes
};

export function validParam(k, v) {
  switch (k) {
    case "bpm":
      return typeof v === "number" && v >= 1 && v <= 400;
    case "detune":
      return typeof v === "number" && v >= -200 && v <= 200;
    case "root":
      return Number.isInteger(v) && v >= 0 && v <= 127;
    case "seed":
      return Number.isInteger(v) && v >= 0 && v <= 0xffffffff;
    case "volume":
      return typeof v === "number" && v >= 0 && v <= 1;
    case "paused":
      return typeof v === "boolean";
    case "scale":
      return typeof v === "string" && v.length <= 48;
    default:
      return false;
  }
}
