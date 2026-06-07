// Geno-2 post pass: punchy print-like grade with subtle scan and grain.

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

@fragment
fn fs_bright(inp: VsOut) -> @location(0) vec4<f32> {
    let col = textureSample(hdr_tex, hdr_sampler, inp.uv).rgb;
    let l = luminance(col);
    let k = max(l - u_post.threshold, 0.0);
    let outc = col * (k / max(l, 1e-5));
    return vec4<f32>(outc, 1.0);
}

@fragment
fn fs_blur(inp: VsOut) -> @location(0) vec4<f32> {
    let texel = u_post.blur_dir / u_post.resolution;

    // Normalized weights (sum = 1.0) so each blur pass preserves brightness.
    let w0 = 0.0746;
    let w1 = 0.1343;
    let w2 = 0.1791;
    let w3 = 0.2239;

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
    let dir = normalize(center + vec2<f32>(1e-5, 0.0));

    // Small chroma split at frame edges.
    let ca = (0.001 + 0.002 * radius) * (0.8 + 0.6 * u_post.ambient);
    let r = textureSample(hdr_tex, hdr_sampler, inp.uv + dir * ca).r;
    let g = textureSample(hdr_tex, hdr_sampler, inp.uv).g;
    let b = textureSample(hdr_tex, hdr_sampler, inp.uv - dir * ca).b;
    var base = vec3<f32>(r, g, b);

    let bloom = textureSample(blur_tex, blur_sampler, inp.uv).rgb * u_post.bloom_strength;
    base += bloom;

    // Slight exposure and tonemap.
    base *= 0.96;
    var mapped = aces_tonemap(base);

    // Print-like contrast and channel bend.
    mapped = clamp((mapped - vec3<f32>(0.5)) * 1.22 + vec3<f32>(0.5), vec3<f32>(0.0), vec3<f32>(1.0));
    mapped = pow(mapped, vec3<f32>(0.95, 1.02, 1.07));

    // Soft posterization gives a distinctive non-photoreal finish.
    let levels = 20.0;
    mapped = floor(mapped * levels) / levels;

    let luma = luminance(mapped);
    let cool = vec3<f32>(0.88, 1.03, 1.10);
    let warm = vec3<f32>(1.10, 0.97, 0.86);
    let grade = mix(cool, warm, smoothstep(0.24, 0.86, luma));
    mapped *= grade;

    let vignette = 1.0 - smoothstep(0.24, 0.94, radius * 1.18);
    mapped *= mix(0.54, 1.02, vignette);

    let t = u_post.time;
    let scan = sin((inp.uv.y * u_post.resolution.y + 20.0 * t) * 0.58);
    mapped *= 1.0 + 0.010 * scan;

    let grain = hash21(inp.uv * u_post.resolution + vec2<f32>(37.0 * t, -29.0 * t));
    mapped += (grain - 0.5) * 0.016;

    return vec4<f32>(clamp(mapped, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
