// One alpha mask per instance, tinted. Texels map 1:1 to target pixels; the
// clip is tested at pixel centres. Borrowed from figui's text layer.
struct View { size: vec4f }
@group(0) @binding(0) var mask: texture_2d<f32>;
@group(0) @binding(1) var<uniform> view: View;

struct Vertex {
    @builtin(position) position: vec4f,
    @location(0) @interpolate(flat) rect: vec4f,
    @location(1) @interpolate(flat) clip: vec4f,
    @location(2) @interpolate(flat) color: vec4f,
}

@vertex fn vs(@builtin(vertex_index) i: u32,
              @location(0) rect: vec4f,
              @location(1) clip: vec4f,
              @location(2) color: vec4f) -> Vertex {
    let corners = array<vec2f,6>(vec2f(0,0),vec2f(1,0),vec2f(1,1),vec2f(0,0),vec2f(1,1),vec2f(0,1));
    let p = mix(rect.xy, rect.zw, corners[i]);
    var v: Vertex;
    v.position = vec4f(p.x / view.size.x * 2 - 1, 1 - p.y / view.size.y * 2, 0, 1);
    v.rect = rect;
    v.clip = clip;
    v.color = color;
    return v;
}

@fragment fn fs(v: Vertex) -> @location(0) vec4f {
    let p = v.position.xy;
    if (any(p < v.clip.xy) || any(p >= v.clip.zw)) { discard; }
    let alpha = textureLoad(mask, vec2i(floor(p - v.rect.xy)), 0).r;
    return vec4f(v.color.rgb, v.color.a * alpha);
}
