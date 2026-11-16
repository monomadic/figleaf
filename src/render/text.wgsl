// One local alpha mask per instance, transformed and tinted. Bilinear sampling
// preserves identity placement and smooths transformed glyph edges.
struct View { size: vec4f }
@group(0) @binding(0) var mask: texture_2d<f32>;
@group(0) @binding(1) var<uniform> view: View;

@group(1) @binding(0) var<storage, read> clip_data: array<vec4f>;

struct Vertex {
    @builtin(position) position: vec4f,
    @location(0) @interpolate(flat) rect: vec4f,
    @location(1) @interpolate(flat) clip: vec4f,
    @location(2) @interpolate(flat) color: vec4f,
    @location(3) local: vec2f,
    @location(4) @interpolate(flat) clip_x: vec4f,
    @location(5) @interpolate(flat) clip_y: vec4f,
    @location(6) @interpolate(flat) clip_range: vec2f,
}

@vertex fn vs(@builtin(vertex_index) i: u32,
              @location(0) rect: vec4f,
              @location(1) clip: vec4f,
              @location(2) color: vec4f,
              @location(3) shape_x: vec4f, @location(4) shape_y: vec4f,
              @location(5) clip_x: vec4f, @location(6) clip_y: vec4f) -> Vertex {
    let corners = array<vec2f,6>(vec2f(0,0),vec2f(1,0),vec2f(1,1),vec2f(0,0),vec2f(1,1),vec2f(0,1));
    let local = mix(rect.xy, rect.zw, corners[i]);
    let p = vec2f(dot(shape_x.xy,local)+shape_x.z,dot(shape_y.xy,local)+shape_y.z);
    var v: Vertex;
    v.position = vec4f(p.x / view.size.x * 2 - 1, 1 - p.y / view.size.y * 2, 0, 1);
    v.clip_range = vec2f(shape_x.w,shape_y.w);
    v.rect = rect;
    v.clip = clip;
    v.color = color;
    v.local = local; v.clip_x = clip_x; v.clip_y = clip_y;
    return v;
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

@fragment fn fs(v: Vertex) -> @location(0) vec4f {
    let p = v.position.xy;
    let cp=vec2f(dot(v.clip_x.xy,p)+v.clip_x.z,dot(v.clip_y.xy,p)+v.clip_y.z);
    if (any(cp < v.clip.xy) || any(cp >= v.clip.zw)) { discard; }
    let at = v.local - v.rect.xy - 0.5;
    let base = vec2i(floor(at)); let t = fract(at);
    let a = mix(textureLoad(mask,base,0).r,textureLoad(mask,base+vec2i(1,0),0).r,t.x);
    let b = mix(textureLoad(mask,base+vec2i(0,1),0).r,textureLoad(mask,base+vec2i(1,1),0).r,t.x);
    let alpha = mix(a,b,t.y);
    return vec4f(v.color.rgb, figma_alpha(v.color.rgb, v.color.a * alpha * clips_coverage(p,v.clip_range)));
}
