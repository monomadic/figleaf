// One coverage layer per instance; see `paint::Layer`. Positions are physical
// pixels. Edges are antialiased over one pixel of signed distance, which is
// exact for axis-aligned edges on pixel boundaries.
struct View { size: vec4f }
@group(0) @binding(0) var<uniform> view: View;

struct Vertex {
    @builtin(position) position: vec4f,
    @location(0) @interpolate(flat) a_rect: vec4f,
    @location(1) @interpolate(flat) a_radii: vec4f,
    @location(2) @interpolate(flat) b_rect: vec4f,
    @location(3) @interpolate(flat) b_radii: vec4f,
    @location(4) @interpolate(flat) color: vec4f,
    // sigma, mode (0 band, 1 drop, 2 inner), ellipse, has b
    @location(5) @interpolate(flat) params: vec4f,
    @location(6) @interpolate(flat) clip: vec4f,
}

@vertex fn vs(@builtin(vertex_index) i: u32,
              @location(0) a_rect: vec4f,
              @location(1) a_radii: vec4f,
              @location(2) b_rect: vec4f,
              @location(3) b_radii: vec4f,
              @location(4) color: vec4f,
              @location(5) params: vec4f,
              @location(6) clip: vec4f,
              @location(7) quad: vec4f) -> Vertex {
    let corners = array<vec2f,6>(vec2f(0,0),vec2f(1,0),vec2f(1,1),vec2f(0,0),vec2f(1,1),vec2f(0,1));
    let p = mix(quad.xy, quad.zw, corners[i]);
    var v: Vertex;
    v.position = vec4f(p.x / view.size.x * 2 - 1, 1 - p.y / view.size.y * 2, 0, 1);
    v.a_rect = a_rect;
    v.a_radii = a_radii;
    v.b_rect = b_rect;
    v.b_radii = b_radii;
    v.color = color;
    v.params = params;
    v.clip = clip;
    return v;
}

// Signed distance to a rounded box. Radii: tl, tr, br, bl; y grows downwards.
fn sd_round(p: vec2f, r: vec4f, radii: vec4f) -> f32 {
    let q0 = p - 0.5 * (r.xy + r.zw);
    let left = select(radii.x, radii.w, q0.y > 0.0);
    let right = select(radii.y, radii.z, q0.y > 0.0);
    let radius = select(left, right, q0.x > 0.0);
    let q = abs(q0) - 0.5 * (r.zw - r.xy) + vec2f(radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2f(0.0))) - radius;
}

// Distance to the ellipse inscribed in a box: exact for circles, first-order
// elsewhere, which is accurate near the edge where it matters.
fn sd_ellipse(p: vec2f, r: vec4f) -> f32 {
    let h = max(0.5 * (r.zw - r.xy), vec2f(0.0001));
    let q = p - 0.5 * (r.xy + r.zw);
    if (abs(h.x - h.y) < 0.001) { return length(q) - h.x; }
    let k0 = length(q / h);
    let k1 = length(q / (h * h));
    if (k1 < 0.000001) { return -min(h.x, h.y); }
    return k0 * (k0 - 1.0) / k1;
}

fn sd(p: vec2f, r: vec4f, radii: vec4f, ellipse: bool) -> f32 {
    if (any(r.zw <= r.xy)) { return 1000000.0; }
    if (ellipse) { return sd_ellipse(p, r); }
    return sd_round(p, r, radii);
}

fn cover(d: f32) -> f32 { return clamp(0.5 - d, 0.0, 1.0); }

fn erf(x: f32) -> f32 {
    let a = abs(x);
    let t = 1.0 / (1.0 + 0.3275911 * a);
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-a * a);
    return sign(x) * y;
}

// Coverage of a shape blurred by a Gaussian, from the distance to its edge.
// Exact for a straight edge; near corners and for shapes smaller than the blur
// it is an approximation.
fn blurred(d: f32, sigma: f32) -> f32 {
    if (sigma < 0.25) { return cover(d); }
    return 0.5 - 0.5 * erf(d / (sigma * 1.41421356));
}

@fragment fn fs(v: Vertex) -> @location(0) vec4f {
    let p = v.position.xy;
    if (any(p < v.clip.xy) || any(p >= v.clip.zw)) { discard; }
    let ellipse = v.params.z > 0.5;
    let da = sd(p, v.a_rect, v.a_radii, ellipse);
    var b = 0.0;
    if (v.params.w > 0.5) { b = cover(sd(p, v.b_rect, v.b_radii, ellipse)); }
    var alpha = 0.0;
    if (v.params.y < 0.5) {
        alpha = cover(da) * (1.0 - b);
    } else if (v.params.y < 1.5) {
        alpha = blurred(da, v.params.x) * (1.0 - b);
    } else {
        alpha = b * (1.0 - blurred(da, v.params.x));
    }
    return vec4f(v.color.rgb, v.color.a * alpha);
}
