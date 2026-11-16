@group(0) @binding(0) var content: texture_2d<f32>;
@group(0) @binding(1) var mask: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: vec4f;
@vertex fn vs(@builtin(vertex_index) i:u32) -> @builtin(position) vec4f {
    let p=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));
    return vec4f(p[i],0,1);
}
@fragment fn fs(@builtin(position) p:vec4f) -> @location(0) vec4f {
    let c=textureLoad(content,vec2i(p.xy),0);
    let m=textureLoad(mask,vec2i(p.xy),0);
    // Perceived brightness in encoded sRGB, checked against Figma primary-color
    // masks. Premultiplied input makes the result include alpha exactly once.
    let coverage=select(m.a,sqrt(dot(m.rgb*m.rgb,vec3f(0.299,0.587,0.114))),params.x>0.5)*params.y;
    return c*coverage;
}
