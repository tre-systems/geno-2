// Geno-2 waves pass: prismatic ribbon field with voice-driven orbital warps.

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

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    let u2 = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, u2.x), mix(c, d, u2.x), u2.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var a = 0.0;
    var b = 0.5;
    var f = p;
    for (var i = 0; i < 5; i = i + 1) {
        a += b * (noise(f) * 2.0 - 1.0);
        f = rot(0.45) * f * 2.05 + vec2<f32>(0.12, -0.09);
        b *= 0.52;
    }
    return a;
}

fn palette(t: f32) -> vec3<f32> {
    let a = vec3<f32>(0.62, 0.59, 0.54);
    let b = vec3<f32>(0.28, 0.23, 0.18);
    let c = vec3<f32>(0.90, 0.76, 0.68);
    let d = vec3<f32>(0.08, 0.15, 0.24);
    return a + b * cos(6.28318 * (c * t + d));
}

@fragment
fn fs_waves(inp: VsOut) -> @location(0) vec4<f32> {
    let uv = inp.uv;
    let aspect = u.resolution.x / max(u.resolution.y, 1.0);
    let t = u.time;
    let p0 = (uv - 0.5) * vec2<f32>(aspect, 1.0);

    let swirl_center = (u.swirl_uv - 0.5) * vec2<f32>(aspect, 1.0);

    var col = vec3<f32>(0.036, 0.030, 0.042);

    for (var l = 0; l < 4; l = l + 1) {
        let depth = f32(l);
        let d01 = depth / 3.0;
        var p = p0 * mix(1.28, 0.58, d01);

        p += vec2<f32>(
            0.24 * sin(0.14 * t + depth * 1.7),
            0.17 * cos(0.16 * t - depth * 1.1),
        );
        p = rot(0.22 * sin(0.08 * t + depth * 1.3)) * p;

        let base_center = swirl_center * mix(1.18, 0.76, d01);
        let swirl_vec = p - base_center;
        let swirl_r = length(swirl_vec);
        let swirl_ang = u.swirl_active * u.swirl_strength * (1.35 + 0.28 * depth) * exp(-1.55 * swirl_r);
        p = base_center + rot(swirl_ang) * swirl_vec;

        var pulse_field = 0.0;
        var halo = 0.0;
        for (var i = 0; i < 3; i = i + 1) {
            let voice = u.voices[i];
            let vp = vec2<f32>(voice.pos_pulse.x, voice.pos_pulse.z) * 0.36;
            let to = p - vp;
            let dist = length(to);
            let pulse = clamp(voice.pos_pulse.w, 0.0, 1.5);

            let tangent = normalize(vec2<f32>(-to.y, to.x) + vec2<f32>(1e-4, -1e-4));
            let twist = exp(-2.6 * dist) * (0.11 + 0.33 * pulse);
            p += tangent * twist;

            pulse_field += (0.36 + 0.68 * pulse) * exp(-2.7 * dist) * sin(11.8 * dist - (1.1 + 0.22 * depth) * t + f32(i));
            halo += exp(-32.0 * dist * dist) * (0.10 + 0.26 * pulse);
        }

        let ridge = sin((11.0 + depth * 2.4) * p.x + 2.2 * sin(3.8 * p.y - 0.31 * t));
        let sweep = cos((8.4 + depth * 1.9) * p.y - 1.7 * cos(3.1 * p.x + 0.26 * t));
        let grain = fbm(p * (2.4 + depth * 0.45) + vec2<f32>(0.05 * t, -0.04 * t));

        let field = 0.55 * ridge + 0.45 * sweep + 0.24 * grain + 0.58 * pulse_field;
        let bands = smoothstep(0.22, 0.88, 0.5 + 0.5 * sin(field * 5.6 + depth * 0.9));
        let filaments = smoothstep(0.80, 0.98, abs(sin(field * 9.7)));

        let low = vec3<f32>(0.07, 0.11, 0.19);
        let high = vec3<f32>(0.88, 0.80, 0.64);
        let aqua = vec3<f32>(0.42, 0.78, 0.82);
        let ember = vec3<f32>(0.92, 0.53, 0.33);

        var layer_col = mix(low, high, bands);
        layer_col = mix(layer_col, aqua, 0.26 + 0.24 * sin(depth + 0.09 * t));
        layer_col += palette(0.14 * depth + 0.08 * field + 0.11 * grain) * (0.08 + 0.18 * u.ambient);
        layer_col += ember * filaments * (0.18 + 0.24 * u.ambient);

        let h = field + 0.9 * halo;
        let n = normalize(vec3<f32>(-1.5 * dpdx(h), -1.5 * dpdy(h), 1.0));
        let l1 = normalize(vec3<f32>(-0.32, 0.46, 0.82));
        let l2 = normalize(vec3<f32>(0.58, -0.12, 0.81));
        let diff = 0.62 * max(dot(n, l1), 0.0) + 0.38 * max(dot(n, l2), 0.0);
        layer_col *= 0.62 + 0.72 * diff;

        let view = vec3<f32>(0.0, 0.0, 1.0);
        let spec = pow(max(dot(normalize(l1 + view), n), 0.0), 70.0);
        layer_col += vec3<f32>(1.0, 0.98, 0.95) * spec * 0.11;
        layer_col += vec3<f32>(0.95, 0.77, 0.60) * halo * (0.30 + 0.44 * u.ambient);

        let ripple_center = (u.ripple_uv - 0.5) * vec2<f32>(aspect, 1.0) * mix(1.2, 0.7, d01);
        let rv = p - ripple_center;
        let rr = length(rv);
        let age = max(0.0, t - u.ripple_t0);
        let ripple_wave = sin(22.0 * rr - 8.0 * age);
        let ripple_env = u.ripple_amp * exp(-1.45 * age) * exp(-4.3 * rr);
        layer_col += vec3<f32>(0.98, 0.85, 0.66) * ripple_wave * ripple_env * 0.34;

        let alpha = mix(0.56, 0.24, d01);
        col = mix(col, layer_col, alpha);
    }

    let vignette = 1.0 - smoothstep(0.34, 1.08, length(p0));
    let center_glow = exp(-3.4 * length(p0));
    col *= mix(0.64, 1.06, vignette);
    col += vec3<f32>(0.10, 0.11, 0.16) * center_glow * (0.24 + 0.66 * u.ambient);

    let dust = noise(p0 * 780.0 + vec2<f32>(0.23 * t, -0.17 * t));
    col += (dust - 0.5) * 0.010;

    return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(4.0)), 1.0);
}
