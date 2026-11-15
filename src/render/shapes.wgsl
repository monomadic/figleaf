// One coverage layer per instance; see `paint::Layer`. Positions are physical
// pixels. Edges are antialiased over one pixel of signed distance, which is
// exact for axis-aligned edges on pixel boundaries.
struct View { size: vec4f }
@group(0) @binding(0) var<uniform> view: View;
struct PathEdge { points: vec4f, flags: vec4f }
@group(1) @binding(0) var<storage, read> path_edges: array<PathEdge>;
struct Transforms { shape_x:vec4f, shape_y:vec4f, clip_x:vec4f, clip_y:vec4f }
@group(1) @binding(1) var<storage, read> transforms: array<Transforms>;
fn mapped(p:vec2f, x:vec4f, y:vec4f) -> vec2f { return vec2f(dot(x.xy,p)+x.z,dot(y.xy,p)+y.z); }
fn pixel_width(d:f32, transformed:f32) -> f32 {
    let width=max(length(vec2f(dpdx(d),dpdy(d))),0.000001);
    return select(1.0,width,transformed>0.5);
}

// Regions are unioned per sample. Each region's winding spans every subpath,
// preserving holes and self-intersections without darkening overlapping fills.
fn path_inside(p: vec2f, first: u32, count: u32) -> bool {
    var winding = 0i;
    for (var i = first; i < first + count; i++) {
        let edge = path_edges[i];
        let a = edge.points.xy;
        let b = edge.points.zw;
        let cross = (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y);
        if (a.y <= p.y && b.y > p.y && cross > 0.0) { winding++; }
        if (a.y > p.y && b.y <= p.y && cross < 0.0) { winding--; }
        if (edge.flags.y > 0.5) {
            let inside = select(winding != 0, (abs(winding) % 2) != 0, edge.flags.x > 0.5);
            if (inside) { return true; }
            winding = 0;
        }
    }
    return false;
}
fn path_coverage(p: vec2f, range: vec4f, open: vec4f, domain: vec4f, align: f32) -> f32 {
    var coverage = 0.0;
    for (var y = 0u; y < 4u; y++) {
        for (var x = 0u; x < 4u; x++) {
            let sample_at = p + (vec2f(f32(x), f32(y)) + 0.5) / 4.0 - 0.5;
            var inside=path_inside(sample_at,u32(range.x),u32(range.y));
            if (align>0.5) {
                let filled=path_inside(sample_at,u32(domain.x),u32(domain.y));
                inside=inside && select(filled,!filled,align>1.5);
            }
            inside=inside || path_inside(sample_at,u32(open.x),u32(open.y));
            if (inside) { coverage += 1.0 / 16.0; }
        }
    }
    return coverage;
}

struct Vertex {
    @builtin(position) position: vec4f,
    @location(0) @interpolate(flat) a_rect: vec4f,
    @location(1) @interpolate(flat) a_radii: vec4f,
    @location(2) @interpolate(flat) b_rect: vec4f,
    @location(3) @interpolate(flat) b_radii: vec4f,
    @location(4) @interpolate(flat) color: vec4f,
    // sigma, mode (0 band, 1 drop, 2 inner), kind (0 box, 1 ellipse, 2 line),
    // has a ramp
    @location(5) @interpolate(flat) params: vec4f,
    @location(6) @interpolate(flat) clip: vec4f,
    @location(7) @interpolate(flat) clip_radii: vec4f,
    // Ramp: position = dot(pixel, axis.xy) + axis.z; eight stop offsets;
    // eight stop colours, one packed RGBA8 per component.
    @location(8) @interpolate(flat) axis: vec4f,
    @location(9) @interpolate(flat) offsets_a: vec4f,
    @location(10) @interpolate(flat) offsets_b: vec4f,
    @location(11) @interpolate(flat) stops_a: vec4u,
    @location(12) @interpolate(flat) stops_b: vec4u,
    @location(13) @interpolate(flat) a_radii_y: vec4f,
    @location(14) @interpolate(flat) b_radii_y: vec4f,
}

@vertex fn vs(@builtin(vertex_index) i: u32,
              @location(0) a_rect: vec4f,
              @location(1) a_radii: vec4f,
              @location(2) b_rect: vec4f,
              @location(3) b_radii: vec4f,
              @location(4) color: vec4f,
              @location(5) params: vec4f,
              @location(6) clip: vec4f,
              @location(7) quad: vec4f,
              @location(8) clip_radii: vec4f,
              @location(9) axis: vec4f,
              @location(10) offsets_a: vec4f,
              @location(11) offsets_b: vec4f,
              @location(12) stops_a: vec4u,
              @location(13) stops_b: vec4u,
              @location(14) a_radii_y: vec4f,
              @location(15) b_radii_y: vec4f) -> Vertex {
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
    v.clip_radii = clip_radii;
    v.axis = axis;
    v.offsets_a = offsets_a;
    v.offsets_b = offsets_b;
    v.stops_a = stops_a;
    v.stops_b = stops_b;
    v.a_radii_y = a_radii_y;
    v.b_radii_y = b_radii_y;
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

// Distance to a round-ended line from r.xy to r.zw, `half` its width.
fn sd_line(p: vec2f, r: vec4f, half: f32) -> f32 {
    let pa = p - r.xy;
    let ba = r.zw - r.xy;
    let h = clamp(dot(pa, ba) / max(dot(ba, ba), 0.000001), 0.0, 1.0);
    return length(pa - ba * h) - half;
}

fn sd(p: vec2f, r: vec4f, radii: vec4f, ry: vec4f, kind: f32) -> f32 {
    if (kind > 1.5) { return sd_line(p, r, radii.x); }
    if (any(r.zw <= r.xy)) { return 1000000.0; }
    if (kind > 0.5) { return sd_ellipse(p, r); }
    if (all(radii == ry)) { return sd_round(p, r, radii); }
    let center = 0.5 * (r.xy + r.zw);
    let right = p.x > center.x;
    let bottom = p.y > center.y;
    let rx = select(select(radii.x, radii.w, bottom), select(radii.y, radii.z, bottom), right);
    let radius_y = select(select(ry.x, ry.w, bottom), select(ry.y, ry.z, bottom), right);
    let radius = vec2f(rx, radius_y);
    let q = abs(p - center) - 0.5 * (r.zw - r.xy) + radius;
    if (rx <= 0.0 || radius_y <= 0.0) {
        return sd_round(p, r, vec4f(0));
    }
    if (q.x > 0.0 && q.y > 0.0) {
        let k0 = length(q / radius);
        let k1 = length(q / (radius * radius));
        return k0 * (k0 - 1.0) / max(k1, 0.000001);
    }
    return max(q.x - rx, q.y - radius_y);
}

// On a target that stores linear light (an sRGB format; `view.size.z` is 1)
// translucent black would darken less than it does in Figma, which blends
// encoded values. Raising its transparency to the display gamma gives black
// the darkening Figma shows. Other colours blend in linear light there.
fn figma_alpha(color: vec3f, a: f32) -> f32 {
    if (view.size.z > 0.5 && all(color == vec3f(0.0))) {
        return 1.0 - pow(1.0 - a, 2.2);
    }
    return a;
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
fn blurred(d: f32, sigma: f32, pixel: f32) -> f32 {
    if (sigma < 0.25 * pixel) { return cover(d / pixel); }
    return 0.5 - 0.5 * erf(d / (sigma * 1.41421356));
}

fn premultiplied(c: vec4f) -> vec4f { return vec4f(c.rgb * c.a, c.a); }

fn to_linear(c: vec3f) -> vec3f {
    return select(pow((c + 0.055) / 1.055, vec3f(2.4)), c / 12.92, c <= vec3f(0.04045));
}

// The ramp's colour averaged over the pixel's width along the axis, so a hard
// stop that falls inside a pixel gets an antialiased edge. Between stops,
// colour and alpha blend separately (straight alpha), as Figma blends them.
fn ramp(v: Vertex, p: vec2f) -> vec4f {
    var o = array<f32, 8>(v.offsets_a.x, v.offsets_a.y, v.offsets_a.z, v.offsets_a.w,
                          v.offsets_b.x, v.offsets_b.y, v.offsets_b.z, v.offsets_b.w);
    var c = array<vec4f, 8>(
        unpack4x8unorm(v.stops_a.x), unpack4x8unorm(v.stops_a.y),
        unpack4x8unorm(v.stops_a.z), unpack4x8unorm(v.stops_a.w),
        unpack4x8unorm(v.stops_b.x), unpack4x8unorm(v.stops_b.y),
        unpack4x8unorm(v.stops_b.z), unpack4x8unorm(v.stops_b.w));
    let t = dot(p, v.axis.xy) + v.axis.z;
    let width = max(length(v.axis.xy), 0.000001);
    let lo = t - 0.5 * width;
    let hi = t + 0.5 * width;
    var sum = premultiplied(c[0]) * max(min(hi, o[0]) - lo, 0.0);
    for (var i = 0; i < 7; i++) {
        let left = max(lo, o[i]);
        let right = min(hi, o[i + 1]);
        if (right > left) {
            let m = (0.5 * (left + right) - o[i]) / (o[i + 1] - o[i]);
            sum += premultiplied(mix(c[i], c[i + 1], m)) * (right - left);
        }
    }
    sum += premultiplied(c[7]) * max(hi - max(lo, o[7]), 0.0);
    sum /= width;
    var rgb = sum.rgb / max(sum.a, 0.000001);
    if (view.size.z > 0.5) { rgb = to_linear(rgb); }
    return vec4f(rgb, sum.a);
}

@fragment fn fs(v: Vertex) -> @location(0) vec4f {
    let p = v.position.xy;
    // The quad is already cut to the clip's box; this rounds its corners.
    let transform = transforms[u32(v.axis.w)];
    let local = mapped(p,transform.shape_x,transform.shape_y);
    let clip_point = mapped(p,transform.clip_x,transform.clip_y);
    let dc = sd_round(clip_point,v.clip,v.clip_radii);
    let clipped = cover(dc / pixel_width(dc,transform.clip_x.w));
    let kind = v.params.z;
    let da = sd(local, v.a_rect, v.a_radii, v.a_radii_y, kind);
    var b = 0.0;
    // An absent `b` is an empty box or a line of no width: it covers nothing.
    let db = sd(local, v.b_rect, v.b_radii, v.b_radii_y, kind);
    let aw = pixel_width(da,transform.shape_x.w);
    let bw = pixel_width(db,transform.shape_x.w);
    b = cover(db / bw);
    var alpha = 0.0;
    if (kind > 2.5) {
        alpha = path_coverage(p, v.b_rect, v.a_radii, v.b_radii, select(0.0,v.params.y,kind>3.5));
    } else if (v.params.y < 0.5) {
        alpha = max(cover(da / aw) - b, 0.0);
    } else if (v.params.y < 1.5) {
        alpha = blurred(da, v.params.x, aw) * (1.0 - b);
    } else {
        alpha = b * (1.0 - blurred(da, v.params.x, aw));
    }
    var color = v.color;
    if (v.params.w > 0.5) {
        let r = ramp(v, p);
        color = vec4f(r.rgb, r.a * v.color.a);
    }
    return vec4f(color.rgb, figma_alpha(color.rgb, color.a * alpha * clipped));
}
