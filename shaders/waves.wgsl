// Geno-2 waves pass: geometric kaleidoscope lattice with voice-reactive spokes.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct Voice {
    // xyz position (x,z used), w = pulse (0..1.5)
    pos_pulse: vec4<f32>,
};

struct WaveUniforms {
    resolution: vec2<f32>,
    time: f32,
    ambient: f32,
    voices: array<Voice, 3>,
    swirl_uv: vec2<f32>,
    swirl_strength: f32,
    swirl_active: f32,
    ripple_uv: vec2<f32>,
    ripple_t0: f32,
    ripple_amp: f32,
};

@group(0) @binding(0) var<uniform> u: WaveUniforms;

@vertex
fn vs_fullscreen(@builtin(vertex_index) vid: u32) -> VsOut {
    let pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(3.0, 1.0),
    );
    let uv = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 2.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(2.0, 0.0),
    );

    var out: VsOut;
    out.pos = vec4<f32>(pos[vid], 0.0, 1.0);
    out.uv = uv[vid];
    return out;
}

fn rot(a: f32) -> mat2x2<f32> {
    let c = cos(a);
    let s = sin(a);
    return mat2x2<f32>(
        vec2<f32>(c, -s),
        vec2<f32>(s, c),
    );
}

fn hash21(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

fn hash22(p: vec2<f32>) -> vec2<f32> {
    let x = hash21(p + vec2<f32>(19.4, 7.1));
    let y = hash21(p + vec2<f32>(83.2, 53.8));
    return vec2<f32>(x, y);
}

fn palette(t: f32) -> vec3<f32> {
    let a = vec3<f32>(0.45, 0.48, 0.44);
    let b = vec3<f32>(0.35, 0.21, 0.17);
    let c = vec3<f32>(0.92, 0.84, 0.72);
    let d = vec3<f32>(0.08, 0.16, 0.26);
    return a + b * cos(6.28318 * (c * t + d));
}

@fragment
fn fs_waves(inp: VsOut) -> @location(0) vec4<f32> {
    let uv = inp.uv;
    let aspect = u.resolution.x / max(u.resolution.y, 1.0);
    let tau = 6.28318;
    let t = u.time;

    var p = (uv - 0.5) * vec2<f32>(aspect, 1.0);

    // Pointer-driven shear before kaleidoscope fold.
    let swirl_center = (u.swirl_uv - 0.5) * vec2<f32>(aspect, 1.0);
    let dv_swirl = p - swirl_center;
    let swirl_r = length(dv_swirl);
    let swirl_ang = u.swirl_active * u.swirl_strength * 1.55 * exp(-2.1 * swirl_r);
    p = swirl_center + rot(swirl_ang) * dv_swirl;

    // Kaleidoscope fold (12 sectors) produces a faceted look unlike wave fields.
    let sector = tau / 12.0;
    let radius = length(p);
    var ang = atan2(p.y, p.x);
    ang = abs(fract((ang / sector) + 0.5) - 0.5) * sector;
    p = vec2<f32>(cos(ang), sin(ang)) * radius;

    // Voice-reactive swirl/spoke warp.
    var tangential_warp = vec2<f32>(0.0);
    var radial_glow = 0.0;
    var spoke_field = 0.0;
    for (var i = 0; i < 3; i = i + 1) {
        let voice = u.voices[i];
        let vp = vec2<f32>(voice.pos_pulse.x, voice.pos_pulse.z) * 0.38;
        let v = p - vp;
        let d = length(v);
        let pulse = clamp(voice.pos_pulse.w, 0.0, 1.5);
        let n = normalize(v + vec2<f32>(1e-4, -1e-4));
        tangential_warp += vec2<f32>(-n.y, n.x) * exp(-2.5 * d) * (0.03 + 0.11 * pulse);

        let theta = atan2(v.y, v.x);
        let spokes = abs(sin(theta * (5.0 + f32(i) * 2.0) + 0.6 * t + f32(i) * 0.9));
        spoke_field += spokes * exp(-1.9 * d) * (0.26 + 0.34 * pulse);

        radial_glow += exp(-28.0 * d * d) * (0.12 + 0.18 * pulse);
    }
    p += tangential_warp;

    // Moving Voronoi lattice in folded space.
    let lattice_p = p * 6.8 + vec2<f32>(0.12 * t, -0.09 * t);
    let cell = floor(lattice_p);
    let f = fract(lattice_p) - 0.5;

    var d1 = 1e9;
    var d2 = 1e9;
    var nearest_id = vec2<f32>(0.0);
    for (var iy = -1; iy <= 1; iy = iy + 1) {
        for (var ix = -1; ix <= 1; ix = ix + 1) {
            let off = vec2<f32>(f32(ix), f32(iy));
            let rnd = hash22(cell + off);
            let orbit = vec2<f32>(
                sin(t * (0.18 + rnd.x * 0.45) + rnd.y * tau),
                cos(t * (0.16 + rnd.y * 0.55) + rnd.x * tau)
            );
            let q = off + (rnd - 0.5) * 0.70 + orbit * 0.18;
            let d = length(f - q);
            if (d < d1) {
                d2 = d1;
                d1 = d;
                nearest_id = cell + off + rnd;
            } else if (d < d2) {
                d2 = d;
            }
        }
    }

    let edge = d2 - d1;
    let edge_band = smoothstep(0.012, 0.085, edge);
    let contour = 0.5 + 0.5 * sin((d1 * 34.0 - 0.95 * t) + 2.8 * spoke_field);
    let contour_band = smoothstep(0.22, 0.82, contour);

    let id_noise = hash21(nearest_id);
    let pat = palette(0.18 * id_noise + 0.06 * t);

    let bg = vec3<f32>(0.032, 0.040, 0.055);
    let teal = vec3<f32>(0.10, 0.62, 0.60);
    let ember = vec3<f32>(0.94, 0.55, 0.34);
    let ivory = vec3<f32>(0.95, 0.90, 0.82);

    var col = bg;
    col = mix(col, teal, edge_band * (0.30 + 0.24 * u.ambient));
    col = mix(col, ember, contour_band * 0.56);
    col += pat * (0.12 + 0.30 * u.ambient);
    col += ivory * radial_glow * (0.34 + 0.38 * u.ambient);

    // Click ripple as angular shimmer.
    let ripple_center = (u.ripple_uv - 0.5) * vec2<f32>(aspect, 1.0);
    let rv = p - ripple_center;
    let rr = length(rv);
    let age = max(0.0, t - u.ripple_t0);
    let ring = sin(24.0 * rr - 8.5 * age);
    let ring_env = u.ripple_amp * exp(-1.7 * age) * exp(-4.8 * rr);
    col += vec3<f32>(0.98, 0.88, 0.72) * ring * ring_env * 0.30;

    let vignette = 1.0 - smoothstep(0.34, 1.10, length((uv - 0.5) * vec2<f32>(1.2, 1.0)));
    col *= mix(0.58, 1.06, vignette);

    let grain = hash21((uv * u.resolution) + vec2<f32>(17.0 * t, -23.0 * t));
    col += (grain - 0.5) * 0.010;

    return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(4.0)), 1.0);
}
