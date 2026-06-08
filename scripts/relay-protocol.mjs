// Shared relay protocol: the parameter whitelist, validation, and limits used by
// both the Cloudflare Durable Object (worker.js) and the node dev relay
// (scripts/relay.mjs), so the two can't drift as parameters are added.

export const ALLOWED_KEYS = new Set(["bpm", "detune", "root", "scale", "seed", "paused", "volume"]);

export const LIMITS = {
  maxSocketsPerRoom: 200, // bounds broadcast fan-out per room
  maxMsgPerSec: 16, // per-socket rate for persisted {t:"set"} params
  maxMsgBytes: 1024, // reject oversized frames
  persistThrottleMs: 3000, // min interval between Durable Object storage writes
  maxAuthFails: 8, // wrong-key attempts before the socket is closed (anti-brute-force)
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

// Validate an event and return a CLEAN copy carrying only the known fields (so a
// padded/attacker-tampered message can't be re-broadcast verbatim), or null if
// invalid. Numeric fields are already range-checked by validEvent; `a` is a flag.
export function sanitizeEvent(msg) {
  if (!validEvent(msg)) return null;
  const out = { t: "ev", e: msg.e, u: msg.u, v: msg.v };
  for (const f of ["s", "m", "g", "p"]) {
    if (msg[f] !== undefined) out[f] = msg[f];
  }
  if (msg.a !== undefined) out.a = !!msg.a;
  return out;
}

// Constant-time secret comparison: hash both sides to fixed-length SHA-256
// digests, then XOR-accumulate — so neither the key's length nor how far a guess
// matched leaks through timing. Works in the Worker and Node (Web Crypto).
export async function keyMatches(provided, secret) {
  if (typeof provided !== "string" || typeof secret !== "string" || secret.length === 0) {
    return false;
  }
  const enc = new TextEncoder();
  const [a, b] = await Promise.all([
    crypto.subtle.digest("SHA-256", enc.encode(provided)),
    crypto.subtle.digest("SHA-256", enc.encode(secret)),
  ]);
  const av = new Uint8Array(a);
  const bv = new Uint8Array(b);
  let diff = 0;
  for (let i = 0; i < av.length; i++) diff |= av[i] ^ bv[i];
  return diff === 0;
}
