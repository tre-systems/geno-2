// Geno-2 post pass: bloom blend, cinematic grade, dust, and soft vignette.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct PostUniforms {
    resolution: vec2<f32>,
    time: f32,
    ambient: f32,
    blur_dir: vec2<f32>,
    bloom_strength: f32,
    threshold: f32,
}

@group(0) @binding(0) var hdr_tex: texture_2d<f32>;
@group(0) @binding(1) var hdr_sampler: sampler;
@group(0) @binding(2) var<uniform> u_post: PostUniforms;

@group(1) @binding(0) var blur_tex: texture_2d<f32>;
@group(1) @binding(1) var blur_sampler: sampler;

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

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
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
        f = f * 2.12 + vec2<f32>(0.16, -0.11);
        b *= 0.52;
    }
    return a;
}

fn vignette_mask(uv: vec2<f32>) -> f32 {
    let p = (uv - 0.5) * vec2<f32>(1.10, 1.0);
    let r = length(p);
    return 1.0 - smoothstep(0.34, 0.95, r);
}

@fragment
fn fs_bright(inp: VsOut) -> @location(0) vec4<f32> {
    let col = textureSample(hdr_tex, hdr_sampler, inp.uv).rgb;
    let thr = u_post.threshold;
    let l = luminance(col);
    let k = max(l - thr, 0.0);
    let outc = col * (k / max(l, 1e-5));
    return vec4<f32>(outc, 1.0);
}

@fragment
fn fs_blur(inp: VsOut) -> @location(0) vec4<f32> {
    let texel = u_post.blur_dir / u_post.resolution;

    let w0 = 0.05;
    let w1 = 0.09;
    let w2 = 0.12;
    let w3 = 0.15;

    var acc = vec3<f32>(0.0);
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv - texel * 3.0).rgb * w0;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv - texel * 2.0).rgb * w1;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv - texel * 1.0).rgb * w2;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv).rgb * w3;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv + texel * 1.0).rgb * w2;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv + texel * 2.0).rgb * w1;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv + texel * 3.0).rgb * w0;

    return vec4<f32>(acc, 1.0);
}

@fragment
fn fs_composite(inp: VsOut) -> @location(0) vec4<f32> {
    let center = inp.uv - 0.5;
    let radius = length(center);

    // Mild lens breathing toward frame edges.
    let dist = 1.0 + 0.035 * radius * radius;
    let sample_uv = center * dist + 0.5;
    var base = textureSample(hdr_tex, hdr_sampler, sample_uv).rgb;

    let bloom = textureSample(blur_tex, blur_sampler, sample_uv).rgb * u_post.bloom_strength;
    base += bloom;

    let t = u_post.time;
    let drift = vec3<f32>(
        1.0 + 0.04 * sin(0.13 * t + 0.9),
        1.0 + 0.03 * sin(0.17 * t + 2.1),
        1.0 + 0.05 * sin(0.11 * t + 4.2)
    );
    base *= mix(vec3<f32>(1.0), drift, 0.10 + 0.22 * u_post.ambient);

    // Exposure before tonemap.
    base *= 0.92;
    var mapped = aces_tonemap(base);

    // Soft contrast and slight cyan-to-amber split tone.
    mapped = clamp((mapped - vec3<f32>(0.5)) * 1.10 + vec3<f32>(0.5), vec3<f32>(0.0), vec3<f32>(1.0));
    mapped = pow(mapped, vec3<f32>(1.02, 1.00, 0.98));

    let luma = luminance(mapped);
    let cool = vec3<f32>(0.92, 1.00, 1.08);
    let warm = vec3<f32>(1.08, 0.98, 0.90);
    let grade = mix(cool, warm, smoothstep(0.26, 0.86, luma));
    mapped *= grade;

    let smoke_a = 0.5 + 0.5 * fbm(inp.uv * 2.8 + vec2<f32>(0.04 * t, -0.03 * t));
    let smoke_b = 0.5 + 0.5 * fbm((inp.uv.yx + vec2<f32>(0.12, -0.08)) * 3.1 + vec2<f32>(-0.02 * t, 0.05 * t));
    let smoke = clamp(0.52 * smoke_a + 0.48 * smoke_b, 0.0, 1.0);
    let smoke_k = 0.12 * smoke * smoothstep(0.18, 0.96, radius * 1.4);
    mapped = mapped * (1.0 - smoke_k) + vec3<f32>(0.08, 0.10, 0.14) * (smoke_k * 0.55);

    let vig = vignette_mask(inp.uv);
    mapped *= mix(0.62, 1.0, vig);

    let grain = noise(inp.uv * u_post.resolution + vec2<f32>(23.0 * t, -17.0 * t));
    mapped += (grain - 0.5) * 0.014;

    let shimmer = sin((inp.uv.y * u_post.resolution.y + 12.0 * t) * 0.45);
    mapped *= 1.0 + 0.006 * shimmer;

    return vec4<f32>(clamp(mapped, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
