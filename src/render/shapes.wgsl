// One coverage layer per instance; see `paint::Layer`. Positions are physical
// pixels. Edges are antialiased over one pixel of signed distance, which is
// exact for axis-aligned edges on pixel boundaries.
struct View { size: vec4f }
@group(0) @binding(0) var<uniform> view: View;
struct PathEdge { points: vec4f, flags: vec4f }
@group(1) @binding(0) var<storage, read> path_edges: array<PathEdge>;
struct Transforms { shape_x:vec4f, shape_y:vec4f, clip_x:vec4f, clip_y:vec4f, clip_range:vec4f }
@group(1) @binding(1) var<storage, read> transforms: array<Transforms>;
@group(1) @binding(2) var<storage, read> clip_data: array<vec4f>;
// Each ramp stores two mapping rows (kind/count in w), then offset/color pairs.
@group(1) @binding(3) var<storage, read> gradients: array<vec4f>;
@group(2) @binding(0) var paint_image: texture_2d<f32>;
@group(2) @binding(1) var paint_sampler: sampler;
fn image_color(v:Vertex,p:vec2f)->vec4f {
    var uv=mapped(p,v.axis,v.offsets_a);
    let period=v.offsets_b.xy;
    let kind=v.offsets_b.z;
    if (kind>1.5) {
        if (kind>3.5) {uv.y-=0.5*period.y*(floor(uv.x/period.x)-2.0*floor(uv.x/period.x/2.0));}
        else if (kind>2.5) {uv.x-=0.5*period.x*(floor(uv.y/period.y)-2.0*floor(uv.y/period.y/2.0));}
        uv=uv-floor(uv/period)*period;
    }
    let half_texel=0.5/vec2f(textureDimensions(paint_image));
    let wraps=vec2<bool>(kind>1.5) & (period==vec2f(1.0));
    let sample_uv=select(clamp(uv,half_texel,1.0-half_texel),uv,wraps);
    var color=textureSampleLevel(paint_image,paint_sampler,sample_uv,0.0);
    if (kind>0.5) {
        let width=max(vec2f(length(v.axis.xy),length(v.offsets_a.xy)),vec2f(0.000001));
        let coverage=clamp(min(uv,1.0-uv)/width+0.5,vec2f(0.0),vec2f(1.0));
        let covered=select(coverage,vec2f(1.0),wraps);
        color*=covered.x*covered.y;
    }
    var rgb=color.rgb/max(color.a,0.000001);
    if (view.size.z>0.5) {rgb=to_linear(rgb);}
    return vec4f(rgb,color.a);
}
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
fn path_coverage(p: vec2f, range: vec4f, open: vec4f, domain: vec4f, align: f32, clips: vec2f) -> f32 {
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
            if (inside && clips_inside(sample_at,clips)) { coverage += 1.0 / 16.0; }
        }
    }
    return coverage;
}

// Same signed-distance shadow model as primitive shadows, evaluated against
// the actual path silhouette in local space before the affine transform.
fn path_distance(p:vec2f, range:vec4f) -> f32 {
    var distance=1e7;
    for (var i=u32(range.x); i<u32(range.x+range.y); i++) {
        let edge=path_edges[i].points; let d=edge.zw-edge.xy;
        let t=clamp(dot(p-edge.xy,d)/max(dot(d,d),1e-12),0.0,1.0);
        distance=min(distance,length(p-edge.xy-t*d));
    }
    return select(distance,-distance,path_inside(p,u32(range.x),u32(range.y)));
}
fn silhouette_distance(p:vec2f, fill:vec4f, closed:vec4f, open:vec4f, flags:f32) -> f32 {
    let domain=path_distance(p,fill);
    var stroke=path_distance(p,closed);
    let align=flags%4.0;
    if (align>0.5) { stroke=max(stroke,select(domain,-domain,align>1.5)); }
    var distance=min(stroke,path_distance(p,open));
    if (flags>=4.0) { distance=min(distance,domain); }
    return distance;
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
    let base = u32(v.axis.x);
    let axis = gradients[base];
    let second = gradients[base+1u];
    let count = u32(second.w);
    var o: array<f32,64>;
    var c: array<vec4f,64>;
    for (var i=0u; i<count; i++) {
        o[i]=gradients[base+2u+i*2u].x;
        c[i]=gradients[base+3u+i*2u];
    }
    if (axis.w>0.5) {
        var sum=vec4f(0.0);
        for (var y=0u; y<4u; y++) {
            for (var x=0u; x<4u; x++) {
                let at=p+(vec2f(f32(x),f32(y))+0.5)/4.0-0.5;
                let q=mapped(at,axis,second)-0.5;
                var t=2.0*length(q);
                if (axis.w>2.5) { t=2.0*(abs(q.x)+abs(q.y)); }
                else if (axis.w>1.5) { t=fract(atan2(q.y,q.x)/6.28318530718); }
                var color=c[0];
                for (var i=0u; i<count; i++) {
                    if (t>=o[i]) { color=c[i]; }
                    if (i+1u<count && t>=o[i] && t<o[i+1u]) {
                        color=mix(c[i],c[i+1u],(t-o[i])/(o[i+1u]-o[i]));
                        break;
                    }
                }
                sum+=premultiplied(color)/16.0;
            }
        }
        var rgb=sum.rgb/max(sum.a,0.000001);
        if (view.size.z>0.5) { rgb=to_linear(rgb); }
        return vec4f(rgb,sum.a);
    }
    let t = dot(p, axis.xy) + axis.z;
    let width = max(length(axis.xy), 0.000001);
    let lo = t - 0.5 * width;
    let hi = t + 0.5 * width;
    var sum = premultiplied(c[0]) * max(min(hi, o[0]) - lo, 0.0);
    for (var i = 0; i < i32(count)-1; i++) {
        let left = max(lo, o[i]);
        let right = min(hi, o[i + 1]);
        if (right > left) {
            let m = (0.5 * (left + right) - o[i]) / (o[i + 1] - o[i]);
            sum += premultiplied(mix(c[i], c[i + 1], m)) * (right - left);
        }
    }
    sum += premultiplied(c[count-1u]) * max(hi - max(lo, o[count-1u]), 0.0);
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
    var clipped = cover(dc / pixel_width(dc,transform.clip_x.w));
    if (v.params.z < 2.5 || v.params.z > 4.5) { clipped *= clips_coverage(p,transform.clip_range.xy); }
    let kind = v.params.z;
    let da = sd(local, v.a_rect, v.a_radii, v.a_radii_y, kind);
    var b = 0.0;
    // An absent `b` is an empty box or a line of no width: it covers nothing.
    let db = sd(local, v.b_rect, v.b_radii, v.b_radii_y, kind);
    let aw = pixel_width(da,transform.shape_x.w);
    let bw = pixel_width(db,transform.shape_x.w);
    b = cover(db / bw);
    var alpha = 0.0;
    if (kind > 4.5) {
        let original=silhouette_distance(local,v.b_rect,v.a_radii,v.b_radii,v.params.w);
        let shifted=silhouette_distance(local-v.a_rect.yz,v.b_rect,v.a_radii,v.b_radii,v.params.w);
        let spread=select(v.a_rect.x,-v.a_rect.x,v.params.y>1.5);
        let d=shifted-spread;
        let source=blurred(d,v.params.x,pixel_width(d,transform.shape_x.w));
        let base=cover(original/pixel_width(original,transform.shape_x.w));
        if (v.params.y>1.5) { alpha=base*(1.0-source); }
        else { alpha=source*select(1.0-base,1.0,v.a_rect.w>0.5); }
    } else if (kind > 2.5) {
        alpha = path_coverage(p, v.b_rect, v.a_radii, v.b_radii, select(0.0,v.params.y,kind>3.5),transform.clip_range.xy);
    } else if (v.params.y < 0.5) {
        alpha = max(cover(da / aw) - b, 0.0);
    } else if (v.params.y < 1.5) {
        alpha = blurred(da, v.params.x, aw) * (1.0 - b);
    } else {
        alpha = b * (1.0 - blurred(da, v.params.x, aw));
    }
    var color = v.color;
    if (v.params.w > 0.5 && kind < 4.5) {
        var r=vec4f(0.0);
        if (v.params.w>1.5) {r=image_color(v,p);} else {r=ramp(v,p);}
        color = vec4f(r.rgb, r.a * v.color.a);
    }
    return vec4f(color.rgb, figma_alpha(color.rgb, color.a * alpha * clipped));
}
