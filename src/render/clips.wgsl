// clip_data contains a header followed by pairs of edge points/region flags.
// Regions union inside one clip; every clip must contain the coverage sample.
fn clips_inside(p: vec2f, range: vec2f) -> bool {
    // Reject outside any ancestor box before walking expensive path edges.
    var bounds_at = u32(range.x);
    for (var clip = 0u; clip < u32(range.y); clip++) {
        let bounds = clip_data[bounds_at + 1u];
        if (any(p < bounds.xy) || any(p >= bounds.zw)) { return false; }
        bounds_at += 2u + u32(clip_data[bounds_at].x) * 2u;
    }
    var at = u32(range.x);
    for (var clip = 0u; clip < u32(range.y); clip++) {
        let count = u32(clip_data[at].x);
        at += 2u;
        var winding = 0i;
        var inside = false;
        for (var i = 0u; i < count; i++) {
            let edge = clip_data[at + i * 2u];
            let flags = clip_data[at + i * 2u + 1u];
            let a = edge.xy; let b = edge.zw;
            let cross = (b.x-a.x)*(p.y-a.y)-(p.x-a.x)*(b.y-a.y);
            if (a.y <= p.y && b.y > p.y && cross > 0.0) { winding++; }
            if (a.y > p.y && b.y <= p.y && cross < 0.0) { winding--; }
            if (flags.y > 0.5) {
                inside = inside || select(winding != 0, (abs(winding)%2)!=0, flags.x>0.5);
                winding = 0;
            }
        }
        if (!inside) { return false; }
        at += count * 2u;
    }
    return true;
}
fn clips_coverage(p: vec2f, range: vec2f) -> f32 {
    if (range.y == 0.0) { return 1.0; }
    var coverage = 0.0;
    for (var y = 0u; y < 4u; y++) {
        for (var x = 0u; x < 4u; x++) {
            let sample_at = p + (vec2f(f32(x),f32(y))+0.5)/4.0-0.5;
            if (clips_inside(sample_at,range)) { coverage += 1.0/16.0; }
        }
    }
    return coverage;
}
