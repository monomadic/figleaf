@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var backdrop:texture_2d<f32>;
@group(0) @binding(2) var silhouette:texture_2d<f32>;
@group(0) @binding(3) var linear_sampler:sampler;
struct Params { op:vec4f, direction:vec4f, color:vec4f, other:vec4f }
@group(0) @binding(4) var<uniform> params:Params;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
    let p=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));return vec4f(p[i],0,1);
}
fn lum(c:vec3f)->f32 {return dot(c,vec3f(0.3,0.59,0.11));}
fn sat(c:vec3f)->f32 {return max(c.r,max(c.g,c.b))-min(c.r,min(c.g,c.b));}
fn clip_color(c:vec3f)->vec3f {
    let l=lum(c);let n=min(c.r,min(c.g,c.b));let x=max(c.r,max(c.g,c.b));var r=c;
    if(n<0.0){r=l+((r-l)*l)/max(l-n,0.000001);}
    if(x>1.0){r=l+((r-l)*(1.0-l))/max(x-l,0.000001);}
    return r;
}
fn set_lum(c:vec3f,l:f32)->vec3f {return clip_color(c+(l-lum(c)));}
fn set_sat(c:vec3f,s:f32)->vec3f {let n=min(c.r,min(c.g,c.b));return (c-n)*s/max(sat(c),0.000001);}
fn channel(b:f32,s:f32,m:u32)->f32 {
    switch m {
        case 1u:{return b*s;} case 2u:{return b+s-b*s;}
        case 3u:{return select(2.0*b*s,1.0-2.0*(1.0-b)*(1.0-s),b>0.5);}
        case 4u:{return min(b,s);} case 5u:{return max(b,s);}
        case 6u:{if(b==0.0){return 0.0;}return select(min(1.0,b/max(1.0-s,0.000001)),1.0,s>=1.0);}
        case 7u:{if(b>=1.0){return 1.0;}return select(1.0-min(1.0,(1.0-b)/max(s,0.000001)),0.0,s<=0.0);}
        case 8u:{return select(2.0*b*s,1.0-2.0*(1.0-b)*(1.0-s),s>0.5);}
        case 9u:{if(b<=0.5){return b-(1.0-2.0*s)*b*(1.0-b);}return b+(2.0*s-1.0)*(sqrt(b)-b);}
        case 10u:{return abs(b-s);}case 11u:{return b+s-2.0*b*s;}
        case 16u:{return max(0.0,b+s-1.0);}case 17u:{return min(1.0,b+s);}
        default:{return s;}
    }
}
fn blend(b:vec3f,s:vec3f,m:u32)->vec3f {
    switch m {
        case 12u:{return set_lum(set_sat(s,sat(b)),lum(b));}
        case 13u:{return set_lum(set_sat(b,sat(s)),lum(b));}
        case 14u:{return set_lum(s,lum(b));}
        case 15u:{return set_lum(b,lum(s));}
        default:{return vec3f(channel(b.r,s.r,m),channel(b.g,s.g,m),channel(b.b,s.b,m));}
    }
}
fn over(b:vec4f,s:vec4f,m:u32)->vec4f {
    if(m==16u||m==17u){let a=s.a+b.a-s.a*b.a;let rgb=select(b.rgb+s.rgb-vec3f(b.a*s.a),b.rgb+s.rgb,m==17u);return vec4f(clamp(rgb,vec3f(0.0),vec3f(a)),a);}
    let bc=b.rgb/max(b.a,0.000001);let sc=s.rgb/max(s.a,0.000001);
    return vec4f(s.rgb*(1.0-b.a)+b.rgb*(1.0-s.a)+blend(bc,sc,m)*s.a*b.a,s.a+b.a-s.a*b.a);
}
fn sample_alpha(p:vec2f)->f32 {
    let size=vec2f(textureDimensions(source));
    if(any(p<vec2f(0.0))||any(p>=size)){return 0.0;}
    return textureSampleLevel(source,linear_sampler,p/size,0.0).a;
}
@fragment fn fs(@builtin(position) p:vec4f)->@location(0) vec4f {
    let s=textureLoad(source,vec2i(p.xy),0);let b=textureLoad(backdrop,vec2i(p.xy),0);
    let op=u32(params.op.x);
    if(op==0u){return over(b,s*params.op.z,u32(params.op.y));}
    if(op==1u){return mix(b,s,params.op.z);}
    if(op==2u){
        let radius=i32(ceil(abs(params.op.z)));var a=select(0.0,1.0,params.op.z<0.0);
        for(var y=-radius;y<=radius;y++){for(var x=-radius;x<=radius;x++){
            if(x*x+y*y<=radius*radius){let step=abs(params.op.z)/max(f32(radius),1.0);let at=p.xy+(params.direction.xy*f32(x)+params.direction.zw*f32(y))*step;let v=sample_alpha(at);a=select(max(a,v),min(a,v),params.op.z<0.0);}
        }}return vec4f(a);
    }
    if(op==3u){
        let sigma=params.op.z;let radius=i32(ceil(sigma*3.0));var total=0.0;var weight=0.0;
        for(var i=-radius;i<=radius;i++){let w=exp(-0.5*f32(i*i)/(sigma*sigma));total+=sample_alpha(p.xy+params.direction.xy*f32(i))*w;weight+=w;}
        return vec4f(total/weight);
    }
    if(op==4u){
        let a=textureLoad(silhouette,vec2i(p.xy),0).a;
        var coverage=sample_alpha(p.xy-params.direction.xy);
        if(params.other.x>0.5){coverage=a*(1.0-coverage);}else if(params.other.y<0.5){coverage*=1.0-a;}
        let alpha=coverage*params.color.a;return over(b,vec4f(params.color.rgb*alpha,alpha),u32(params.op.y));
    }
    if(op==6u){let coverage=textureLoad(silhouette,vec2i(p.xy),0).a;return select(s*coverage,mix(b,s,coverage),params.op.y>0.5);}
    if(op==7u){let m=textureLoad(silhouette,vec2i(p.xy),0);let coverage=select(m.a,sqrt(dot(m.rgb*m.rgb,vec3f(0.299,0.587,0.114))),params.op.y>0.5)*params.op.z;return mix(b,s,coverage);}
    // Present an encoded-sRGB premultiplied image into the host's target.
    if(params.op.y>0.5){let c=s.rgb/max(s.a,0.000001);let linear=select(pow((c+0.055)/1.055,vec3f(2.4)),c/12.92,c<=vec3f(0.04045));return vec4f(linear*s.a,s.a);}
    return s;
}
