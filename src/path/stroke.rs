//! Curve-preserving dash fitting and endpoint geometry, lowered to fill regions.
use super::*;
use crate::{StrokeAlign, StrokeCap, StrokeJoin};
use tiny_skia_path::{LineCap, LineJoin, Path, PathBuilder, PathSegment, Stroke, StrokeDash};

fn commands(path: &Path) -> Vec<Command> {
    path.segments()
        .map(|s| match s {
            PathSegment::MoveTo(p) => Command::MoveTo([p.x, p.y]),
            PathSegment::LineTo(p) => Command::LineTo([p.x, p.y]),
            PathSegment::QuadTo(a, p) => Command::QuadraticTo([a.x, a.y], [p.x, p.y]),
            PathSegment::CubicTo(a, b, p) => Command::CubicTo([a.x, a.y], [b.x, b.y], [p.x, p.y]),
            PathSegment::Close => Command::Close,
        })
        .collect()
}
fn build(commands: &[Command]) -> Option<Path> {
    let mut b = PathBuilder::new();
    for c in commands {
        match *c {
            Command::MoveTo(p) => b.move_to(p[0], p[1]),
            Command::LineTo(p) => b.line_to(p[0], p[1]),
            Command::QuadraticTo(a, p) => b.quad_to(a[0], a[1], p[0], p[1]),
            Command::CubicTo(a, b2, p) => b.cubic_to(a[0], a[1], b2[0], b2[1], p[0], p[1]),
            Command::Close => b.close(),
        }
    }
    b.finish()
}
fn end(c: Command) -> [f32; 2] {
    match c {
        Command::MoveTo(p)
        | Command::LineTo(p)
        | Command::QuadraticTo(_, p)
        | Command::CubicTo(_, _, p) => p,
        Command::Close => unreachable!(),
    }
}
fn contours(cs: &[Command]) -> Vec<Vec<Command>> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    let mut at = [0.; 2];
    let mut first = at;
    for c in cs {
        if matches!(c, Command::MoveTo(_)) && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        if current.is_empty() && !matches!(c, Command::MoveTo(_)) {
            current.push(Command::MoveTo(at));
            first = at;
        }
        current.push(*c);
        if matches!(c, Command::Close) {
            at = first;
            out.push(std::mem::take(&mut current));
        } else {
            at = end(*c);
            if matches!(c, Command::MoveTo(_)) {
                first = at;
            }
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}
fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn cubic_length(p: [[f32; 2]; 4], depth: u32) -> f32 {
    let polygon = distance(p[0], p[1]) + distance(p[1], p[2]) + distance(p[2], p[3]);
    let chord = distance(p[0], p[3]);
    if depth >= 16 || polygon - chord < 0.001 {
        return (polygon + chord) * 0.5;
    }
    let a = mix(p[0], p[1], 0.5);
    let b = mix(p[1], p[2], 0.5);
    let c = mix(p[2], p[3], 0.5);
    let d = mix(a, b, 0.5);
    let e = mix(b, c, 0.5);
    let f = mix(d, e, 0.5);
    cubic_length([p[0], a, d, f], depth + 1) + cubic_length([f, e, c, p[3]], depth + 1)
}
fn segment_length(from: [f32; 2], c: Command) -> f32 {
    match c {
        Command::LineTo(p) => distance(from, p),
        Command::QuadraticTo(a, p) => {
            cubic_length([from, mix(from, a, 2. / 3.), mix(p, a, 2. / 3.), p], 0)
        }
        Command::CubicTo(a, b, p) => cubic_length([from, a, b, p], 0),
        _ => 0.,
    }
}
fn length(cs: &[Command]) -> f32 {
    let mut at = [0.; 2];
    let mut n = 0.;
    for c in cs {
        n += segment_length(at, *c);
        if !matches!(c, Command::Close) {
            at = end(*c);
        }
    }
    n
}
// Figma fits the authored sequence independently to each segment. A half
// first dash touches each vertex; adjoining pieces are joined before stroking.
// Zero entries are removed after fitting; odd sequences repeat (including a
// single dash length). This matches the saved Plugin API reference.
fn fitted_pieces(cs: &[Command], dash: &[f32], res: f32) -> Result<Vec<Vec<Command>>, Error> {
    if dash.iter().all(|v| *v == 0.) {
        return Ok(vec![cs.to_vec()]);
    }
    let period: f32 = dash.iter().sum();
    let mut pattern: Vec<_> = dash.iter().copied().filter(|v| *v > 0.).collect();
    if pattern.len() % 2 == 1 {
        pattern.extend(pattern.clone());
    }
    let first = end(cs[0]);
    let mut at = first;
    let mut pieces: Vec<Vec<Command>> = Vec::new();
    let mut dash_commands = 0;
    let closed = matches!(cs.last(), Some(Command::Close));
    for c in &cs[1..] {
        let c = if matches!(c, Command::Close) {
            Command::LineTo(first)
        } else {
            *c
        };
        let len = segment_length(at, c);
        if len <= 0. {
            at = end(c);
            continue;
        }
        let repeats = (len / period).round().max(1.);
        if repeats * pattern.len() as f32 > 4096. {
            return Err(Error("dash count exceeds 4096"));
        }
        let factor = len / (repeats * period);
        let fitted: Vec<_> = pattern.iter().map(|v| v * factor).collect();
        let phase = fitted[0] * 0.5;
        let dash = StrokeDash::new(fitted, phase).ok_or(Error("invalid fitted dash pattern"))?;
        let source = build(&[Command::MoveTo(at), c]).ok_or(Error("invalid dash segment"))?;
        let dashed = source
            .dash(&dash, res)
            .ok_or(Error("dash subdivision failed"))?;
        let mut parts = contours(&commands(&dashed));
        dash_commands += parts.iter().map(Vec::len).sum::<usize>();
        if dash_commands > 4096 {
            return Err(Error("dashed path exceeds 4096 commands"));
        }
        if let (Some(previous), Some(next)) = (pieces.last_mut(), parts.first())
            && distance(end(*previous.last().unwrap()), end(next[0])) < 0.02 / res
        {
            let next = parts.remove(0);
            previous.extend_from_slice(&next[1..]);
        }
        pieces.extend(parts);
        at = end(c);
    }
    if closed
        && pieces.len() > 1
        && distance(
            end(*pieces.last().unwrap().last().unwrap()),
            end(pieces[0][0]),
        ) < 0.02 / res
    {
        let mut tail = pieces.pop().unwrap();
        let head = pieces.remove(0);
        tail.extend_from_slice(&head[1..]);
        pieces.insert(0, tail);
    }
    // A fully painted closed contour must have a join, not two end caps.
    if closed
        && pieces.len() == 1
        && distance(end(pieces[0][0]), end(*pieces[0].last().unwrap())) < 0.02 / res
    {
        pieces[0].push(Command::Close);
    }
    Ok(pieces)
}
fn tangent(cs: &[Command], start: bool) -> Option<[f32; 2]> {
    let endpoint = if start { end(cs[0]) } else { end(*cs.last()?) };
    let mut points = Vec::new();
    for c in cs {
        match *c {
            Command::MoveTo(p) | Command::LineTo(p) => points.push(p),
            Command::QuadraticTo(a, p) => points.extend([a, p]),
            Command::CubicTo(a, b, p) => points.extend([a, b, p]),
            Command::Close => {}
        }
    }
    if !start {
        points.reverse();
    }
    points.into_iter().find_map(|p| {
        let len = distance(p, endpoint);
        (len > 1e-7).then(|| [(endpoint[0] - p[0]) / len, (endpoint[1] - p[1]) / len])
    })
}
fn add_outline(out: &mut Vec<VectorPath>, p: Path, total: &mut usize) -> Result<(), Error> {
    let cs = commands(&p);
    *total += cs.len();
    if *total > 4096 {
        return Err(Error("stroke outline exceeds 4096 commands"));
    }
    let mut v = VectorPath::new(WindingRule::NonZero);
    v.commands = cs;
    out.push(v);
    Ok(())
}
fn decorated(cap: StrokeCap) -> bool {
    !matches!(cap, StrokeCap::Butt | StrokeCap::Round | StrokeCap::Square)
}
fn triangle(cap: StrokeCap) -> bool {
    matches!(cap, StrokeCap::ArrowEquilateral | StrokeCap::TriangleFilled)
}
// Dimensions measured from Figma's strokeGeometry at widths 1, 2, 4 and 8.
// Geometry is local to the endpoint; +x points out of the contour.
fn cap_shape(cap: StrokeCap, p: [f32; 2], dir: [f32; 2], w: f32) -> Option<Path> {
    let mut b = PathBuilder::new();
    let h = w * 0.5;
    let size = 5. * w;
    let side = size / 3f32.sqrt();
    let polygon: Vec<[f32; 2]> = match cap {
        StrokeCap::Butt => return None,
        StrokeCap::Square => vec![[0., -h], [h, -h], [h, h], [0., h]],
        StrokeCap::ArrowEquilateral => vec![[0., 0.], [-size, -side], [-size, side]],
        StrokeCap::TriangleFilled => vec![[-size, -h], [0., -side], [0., side], [-size, h]],
        StrokeCap::DiamondFilled => vec![[side, 0.], [0., side], [-side, 0.], [0., -side]],
        StrokeCap::Round | StrokeCap::CircleFilled => {
            let radius = if cap == StrokeCap::Round {
                h
            } else {
                8. * w / 3.
            };
            b.push_circle(0., 0., radius);
            Vec::new()
        }
        StrokeCap::ArrowLines => {
            let a = 4.5 * w / std::f32::consts::SQRT_2;
            b.move_to(-a, -a);
            b.line_to(0., 0.);
            b.line_to(-a, a);
            let outline = b.finish()?.stroke(
                &Stroke {
                    width: w,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Stroke::default()
                },
                1.,
            )?;
            return outline.transform(tiny_skia_path::Transform::from_row(
                dir[0], dir[1], -dir[1], dir[0], p[0], p[1],
            ));
        }
    };
    if !polygon.is_empty() {
        b.move_to(polygon[0][0], polygon[0][1]);
        for q in &polygon[1..] {
            b.line_to(q[0], q[1]);
        }
        b.close();
    }
    b.finish()?.transform(tiny_skia_path::Transform::from_row(
        dir[0], dir[1], -dir[1], dir[0], p[0], p[1],
    ))
}
fn trimmed(cs: &[Command], start: f32, end: f32, res: f32) -> Result<Option<Path>, Error> {
    let source = build(cs);
    if start == 0. && end == 0. {
        return Ok(source);
    }
    let len = length(cs);
    let keep = len - start - end;
    if keep <= 0. {
        return Ok(None);
    }
    let gap = len + 1.;
    let phase = if start > 0. { keep + gap - start } else { 0. };
    let dash = StrokeDash::new(vec![keep, gap], phase).ok_or(Error("invalid endpoint trim"))?;
    Ok(Some(
        source
            .ok_or(Error("invalid decorated path"))?
            .dash(&dash, res)
            .ok_or(Error("endpoint trim failed"))?,
    ))
}

pub(crate) fn stroke_outlines(
    paths: &[VectorPath],
    width: f32,
    style: PathStroke,
    align: StrokeAlign,
    res: f32,
    dash: &[f32],
) -> Result<[Vec<VectorPath>; 3], Error> {
    stroke_bounds(paths)?;
    validate_dash(dash)?;
    if !res.is_finite()
        || res <= 0.
        || !width.is_finite()
        || width * res > 1e7
        || width * style.miter_limit.max(6.) > 1e7
    {
        return Err(Error("path stroke exceeds numeric limits"));
    }
    let mut output: [Vec<VectorPath>; 3] = Default::default();
    if width == 0. {
        return Ok(output);
    }
    let mut total = 0;
    for path in paths {
        let mut domain = VectorPath::new(if path.winding_rule == WindingRule::None {
            WindingRule::NonZero
        } else {
            path.winding_rule
        });
        for contour in contours(&path.commands) {
            if contour.len() < 2 {
                continue;
            }
            let closed = matches!(contour.last(), Some(Command::Close));
            if closed {
                domain.commands.extend_from_slice(&contour);
            }
            let which = usize::from(!closed);
            let weight = if closed && align != StrokeAlign::Center {
                width * 2.
            } else {
                width
            };
            let start_cap = path.start_cap.unwrap_or(style.cap);
            let end_cap = path.end_cap.unwrap_or(style.cap);
            if !closed {
                for (start, cap) in [(true, start_cap), (false, end_cap)] {
                    if decorated(cap) {
                        let dir = tangent(&contour, start)
                            .ok_or(Error("decorated endpoint needs a nonzero tangent"))?;
                        let point = if start {
                            end(contour[0])
                        } else {
                            end(*contour.last().unwrap())
                        };
                        let shape = cap_shape(cap, point, dir, weight)
                            .ok_or(Error("endpoint decoration failed"))?;
                        add_outline(&mut output[which], shape, &mut total)?;
                    }
                }
            }
            let pieces = fitted_pieces(&contour, dash, res)?;
            for piece in pieces {
                let piece_closed = matches!(piece.last(), Some(Command::Close));
                let p0 = end(piece[0]);
                let p1 = if piece_closed {
                    p0
                } else {
                    end(*piece.last().unwrap())
                };
                let at_start = !closed && distance(p0, end(contour[0])) < 0.02 / res;
                let at_end = !closed && distance(p1, end(*contour.last().unwrap())) < 0.02 / res;
                let cap0 = if at_start {
                    start_cap
                } else if decorated(style.cap) {
                    StrokeCap::Butt
                } else {
                    style.cap
                };
                let cap1 = if at_end {
                    end_cap
                } else if decorated(style.cap) {
                    StrokeCap::Butt
                } else {
                    style.cap
                };
                let trim0 = if at_start && triangle(cap0) {
                    4.5 * weight
                } else {
                    0.
                };
                let trim1 = if at_end && triangle(cap1) {
                    4.5 * weight
                } else {
                    0.
                };
                let source = trimmed(&piece, trim0, trim1, res)?;
                if let Some(source) = source {
                    if length(&piece) > 0. || piece_closed {
                        let outline = source
                            .stroke(
                                &Stroke {
                                    width: weight,
                                    miter_limit: style.miter_limit,
                                    line_cap: LineCap::Butt,
                                    line_join: match style.join {
                                        StrokeJoin::Miter => LineJoin::Miter,
                                        StrokeJoin::Bevel => LineJoin::Bevel,
                                        StrokeJoin::Round => LineJoin::Round,
                                    },
                                    ..Stroke::default()
                                },
                                res,
                            )
                            .ok_or(Error("path stroke could not be constructed"))?;
                        add_outline(&mut output[which], outline, &mut total)?;
                    }
                    if !piece_closed {
                        for (start, cap, p) in [(true, cap0, p0), (false, cap1, p1)] {
                            if matches!(cap, StrokeCap::Round | StrokeCap::Square) {
                                let dir = tangent(&piece, start).unwrap_or(if start {
                                    [-1., 0.]
                                } else {
                                    [1., 0.]
                                });
                                add_outline(
                                    &mut output[which],
                                    cap_shape(cap, p, dir, weight)
                                        .ok_or(Error("stroke cap failed"))?,
                                    &mut total,
                                )?;
                            }
                        }
                    }
                }
            }
        }
        output[2].push(domain);
    }
    Ok(output)
}

/// Primitive stroke centerlines, retaining per-corner circular radii.
pub(crate) fn primitive_path(r: Rect, radii: [f32; 4], ellipse: bool) -> VectorPath {
    let mut p = VectorPath::new(WindingRule::NonZero);
    let k = 0.552_284_8;
    if ellipse {
        let (cx, cy, rx, ry) = (r.x + r.w / 2., r.y + r.h / 2., r.w / 2., r.h / 2.);
        return p
            .move_to([cx + rx, cy])
            .cubic_to(
                [cx + rx, cy + k * ry],
                [cx + k * rx, cy + ry],
                [cx, cy + ry],
            )
            .cubic_to(
                [cx - k * rx, cy + ry],
                [cx - rx, cy + k * ry],
                [cx - rx, cy],
            )
            .cubic_to(
                [cx - rx, cy - k * ry],
                [cx - k * rx, cy - ry],
                [cx, cy - ry],
            )
            .cubic_to(
                [cx + k * rx, cy - ry],
                [cx + rx, cy - k * ry],
                [cx + rx, cy],
            )
            .close();
    }
    let [tl, tr, br, bl] = radii.map(|v| v.min(r.w.min(r.h) / 2.));
    let (x, y, right, bottom) = (r.x, r.y, r.x + r.w, r.y + r.h);
    p = p.move_to([x + tl, y]).line_to([right - tr, y]);
    if tr > 0. {
        p = p.cubic_to(
            [right - tr + k * tr, y],
            [right, y + tr - k * tr],
            [right, y + tr],
        );
    }
    p = p.line_to([right, bottom - br]);
    if br > 0. {
        p = p.cubic_to(
            [right, bottom - br + k * br],
            [right - br + k * br, bottom],
            [right - br, bottom],
        );
    }
    p = p.line_to([x + bl, bottom]);
    if bl > 0. {
        p = p.cubic_to(
            [x + bl - k * bl, bottom],
            [x, bottom - bl + k * bl],
            [x, bottom - bl],
        );
    }
    p = p.line_to([x, y + tl]);
    if tl > 0. {
        p = p.cubic_to([x, y + tl - k * tl], [x + tl - k * tl, y], [x + tl, y]);
    }
    p.close()
}
