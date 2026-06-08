// Shared relay protocol: the parameter whitelist, validation, and limits used by
// both the Cloudflare Durable Object (worker.js) and the node dev relay
// (scripts/relay.mjs), so the two can't drift as parameters are added.

export const ALLOWED_KEYS = new Set(["bpm", "detune", "root", "scale", "seed", "paused", "volume"]);

export const LIMITS = {
  maxSocketsPerRoom: 200, // bounds broadcast fan-out per room
  maxMsgPerSec: 16, // per-socket rate for persisted {t:"set"} params
  maxMsgBytes: 1024, // reject oversized frames
  persistThrottleMs: 3000, // min interval between Durable Object storage writes
};

// Transient performance events ({t:"ev"}) stream far faster than params (a
// performer's swirl/gesture pulse), so they get their own, higher per-socket
// budget. They are broadcast but never persisted or replayed to late joiners.
// The broadcaster coalesces its own output well under this (swirl ~22/s + drag
// ripples), so the headroom is for bursts, not steady state.
export const TRANSIENT_LIMITS = {
  maxEvPerSec: 100,
};

// The gesture kinds a performer may broadcast: a one-shot flare/carve chord, a
// drag ripple, or the continuous pointer swirl.
export const EV_KINDS = new Set(["flare", "carve", "ripple", "swirl"]);

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

const inUnit = (x) => typeof x === "number" && Number.isFinite(x) && x >= 0 && x <= 1;
const finiteOpt = (x) => x === undefined || (typeof x === "number" && Number.isFinite(x));

// A transient performance event ({t:"ev"}): a whitelisted kind plus a uv anchor,
// and a few optional bounded scalars (s=swirl strength, a=active, m=motion,
// g=angle, p=ripple amp). The client clamps on apply; this just rejects garbage.
export function validEvent(msg) {
  return (
    EV_KINDS.has(msg.e) &&
    inUnit(msg.u) &&
    inUnit(msg.v) &&
    finiteOpt(msg.s) &&
    finiteOpt(msg.m) &&
    finiteOpt(msg.g) &&
    finiteOpt(msg.p)
  );
}
