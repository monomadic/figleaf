use super::*;
use crate::{Composite, Image, ImagePaint, Paint, Shape, Text};

pub(super) struct Cached {
    snapshot: Text,
    scale: f32,
    result: Result<Composite, Error>,
}
pub(super) fn prepare(
    cache: &mut HashMap<usize, Cached>,
    index: usize,
    t: &Text,
    scale: f32,
    backend: &mut dyn Backend,
) -> Result<Composite, Error> {
    if let Some(c) = cache.get(&index)
        && c.snapshot == *t
        && c.scale == scale
    {
        return c.result.clone();
    }
    let result = build(t, scale, backend);
    cache.insert(
        index,
        Cached {
            snapshot: t.clone(),
            scale,
            result: result.clone(),
        },
    );
    result
}
pub(super) fn build(t: &Text, scale: f32, backend: &mut dyn Backend) -> Result<Composite, Error> {
    if !t.runs.is_empty() && !t.string.is_empty() {
        return Err(Error("styled text cannot also contain a plain string"));
    }
    if !t.runs.is_empty()
        && (t.text_case != text::TextCase::Original
            || t.text_decoration != text::TextDecoration::None
            || t.decoration_options != text::DecorationOptions::default())
    {
        return Err(Error("rich text requires case and decoration on each run"));
    }
    if !t.runs.is_empty()
        && (!t.open_type_features.is_empty()
            || t.list_options != text::ListOptions::None
            || t.indentation != 0
            || t.list_spacing != 0.
            || t.hanging_list)
    {
        return Err(Error(
            "rich text requires OpenType and list settings on each run",
        ));
    }
    if !t.runs.is_empty() && (t.paints.is_some() || !t.fill_blend_modes.is_empty()) {
        return Err(Error("rich text requires paints on each run"));
    }
    if t.stroke.is_none()
        && (!t.stroke_paints.is_empty()
            || !t.stroke_blend_modes.is_empty()
            || t.stroke_geometry.is_some()
            || !t.dash_pattern.is_empty()
            || t.path_stroke != crate::path::PathStroke::default())
    {
        return Err(Error("text stroke paints require a stroke"));
    }
    t.rect.validate()?;
    t.transform.inverse()?;
    t.transform.bounds(t.rect).validate()?;
    let raster_scale = (scale * t.transform.max_scale()).max(scale).max(0.5);
    let runs = if t.runs.is_empty() {
        vec![text::TextRun {
            text: t.string.clone(),
            style: t.style(),
            color: t.color,
            paints: t.paints.clone(),
            fill_blend_modes: t.fill_blend_modes.clone(),
            decoration_paints: None,
            decoration_blend_modes: Vec::new(),
            text_decoration: t.text_decoration,
            decoration_options: t.decoration_options,
            text_case: t.text_case,
            open_type_features: t.open_type_features.clone(),
            list_options: t.list_options,
            indentation: t.indentation,
            list_spacing: t.list_spacing,
            hanging_list: t.hanging_list,
            direction: text::TextDirection::Auto,
            paragraph_spacing: None,
            paragraph_indent: None,
        }]
    } else {
        t.runs.clone()
    };
    // Validate even invisible/whitespace-only styles before looking for outlines.
    for run in &runs {
        let mut check = Shape::vector(
            t.rect,
            vec![
                crate::VectorPath::new(crate::WindingRule::NonZero)
                    .move_to([0., 0.])
                    .line_to([1., 0.])
                    .line_to([1., 1.])
                    .close(),
            ],
        );
        check.fill_blend_modes = run.fill_blend_modes.clone();
        check.path_stroke = t.path_stroke;
        check.dash_pattern = t.dash_pattern.clone();
        check.stroke_geometry = t.stroke_geometry.clone();
        check.fills = run
            .paints
            .clone()
            .unwrap_or_else(|| vec![Paint::Solid(run.color)]);
        check.stroke = t.stroke;
        check.stroke_paints = t.stroke_paints.clone();
        check.stroke_blend_modes = t.stroke_blend_modes.clone();
        if let Some(reason) = check.problem() {
            return Err(Error(reason));
        }
        if let Some(paints) = &run.decoration_paints {
            check.fills = paints.clone();
            check.fill_blend_modes = run.decoration_blend_modes.clone();
            if let Some(reason) = check.problem() {
                return Err(Error(reason));
            }
        } else if !run.decoration_blend_modes.is_empty() {
            return Err(Error(
                "decoration blend modes require resolved decoration paints",
            ));
        }
    }
    let paragraph = text::ParagraphStyle {
        spacing: t.paragraph_spacing,
        indent: t.paragraph_indent,
    };
    paragraph.validate(t.rect.w)?;
    text::validate_truncation(t.rect.h, t.max_lines)?;
    if runs.len() == 1 && runs[0].text.is_empty() {
        return Ok(Composite::new(Canvas::new()));
    }
    text::validate_runs(&runs, t.rect.w, raster_scale, t.wrap)?;
    let lines = text::layout_text(
        backend,
        &runs,
        [t.rect.w, t.rect.h],
        t.wrap,
        paragraph,
        t.text_truncation,
        t.max_lines,
    )?;
    let height: f32 = lines.iter().map(|l| l.height + l.spacing_after).sum();
    if !height.is_finite() || height > 1e7 {
        return Err(Error("paragraph block exceeds layout limits"));
    }
    let place = |align, spare: f32| match align {
        crate::paint::Align::Start => 0.,
        crate::paint::Align::Center => spare / 2.,
        crate::paint::Align::End => spare,
    };
    let mut y = t.rect.y + place(t.vertical_align, t.rect.h - height);
    let mut pixels = 0u64;
    let outlined = t.stroke.is_some()
        || runs.iter().any(|r| {
            r.paints.is_some()
                || !r.fill_blend_modes.is_empty()
                || r.text_decoration != text::TextDecoration::None
                || r.decoration_options != text::DecorationOptions::default()
        });
    let mut stroke_paths = Vec::new();
    let mut content = Canvas::new();
    let mut positioned = Vec::new();
    for mut line in lines {
        if let Some(marker) = line.marker.take() {
            let mut label = line.clone();
            label.runs = marker.runs;
            label.metrics = marker.metrics;
            label.height = label
                .runs
                .iter()
                .map(|r| r.style.line_height)
                .reduce(f32::max)
                .unwrap();
            let align = place(t.align, line.available_width - line.metrics.advance as f32);
            let rtl = label
                .runs
                .first()
                .is_some_and(|r| r.direction == text::TextDirection::RightToLeft);
            let marker_x = if rtl {
                align + line.metrics.advance as f32 + line.indent
                    - marker.x
                    - marker.metrics.advance as f32
            } else {
                marker.x + align
            };
            label.available_width = marker.metrics.advance as f32;
            let dy = line.metrics.baseline as f32 - marker.metrics.baseline as f32
                + line.baseline_offset;
            positioned.push((label, dy, 0., Some(marker_x)));
        }
        let advance = line.height + line.spacing_after;
        let dy = line.baseline_offset;
        positioned.push((line, dy, advance, None));
    }
    for (line, dy, advance_y, marker_x) in positioned {
        y += dy;
        if !line.runs.is_empty() {
            if line.metrics.advance > f64::from(text::MAX_WIDTH) {
                return Err(Error("styled line exceeds bitmap width limit"));
            }
            if outlined {
                let outlines = backend.outline_runs(&line.runs)?;
                if outlines.paths.len() != line.runs.len()
                    || outlines.decoration_paths.len() != line.runs.len()
                    || outlines.metrics.lines != 1
                    || !outlines.metrics.advance.is_finite()
                    || !outlines.metrics.baseline.is_finite()
                    || (outlines.metrics.advance - line.metrics.advance).abs() > 0.01
                    || (outlines.metrics.baseline - line.metrics.baseline).abs() > 0.01
                {
                    return Err(Error("text backend violated outline metrics"));
                }
                let rtl = line.runs[0].direction == text::TextDirection::RightToLeft;
                let x = marker_x.unwrap_or_else(|| if rtl { 0. } else { line.indent }
                    + place(t.align, line.available_width - line.metrics.advance as f32));
                let offset = [x, y - t.rect.y];
                for (mut path, run, decoration) in outlines
                    .decoration_paths
                    .into_iter()
                    .zip(&line.runs)
                    .map(|(p, r)| (p, r, true))
                    .chain(
                        outlines
                            .paths
                            .into_iter()
                            .zip(&line.runs)
                            .map(|(p, r)| (p, r, false)),
                    )
                {
                    if path.commands.is_empty() {
                        continue;
                    }
                    for command in &mut path.commands {
                        use crate::path::Command::*;
                        let add = |p: &mut [f32; 2]| {
                            p[0] += offset[0];
                            p[1] += offset[1];
                        };
                        match command {
                            MoveTo(p) | LineTo(p) => add(p),
                            QuadraticTo(a, b) => {
                                add(a);
                                add(b);
                            }
                            CubicTo(a, b, c) => {
                                add(a);
                                add(b);
                                add(c);
                            }
                            Close => {}
                        }
                    }
                    if t.stroke.is_some() {
                        stroke_paths.push(path.clone());
                    }
                    let mut shape = Shape::vector(t.rect, vec![path]).transform(t.transform);
                    shape.fills =
                        if let (true, Some(color)) = (decoration, run.decoration_options.color) {
                            vec![Paint::Solid(color)]
                        } else {
                            if decoration {
                                run.decoration_paints.clone()
                            } else {
                                run.paints.clone()
                            }
                            .unwrap_or_else(|| vec![Paint::Solid(run.color)])
                        };
                    shape.fill_blend_modes = if decoration {
                        if run.decoration_options.color.is_some() {
                            Vec::new()
                        } else {
                            run.decoration_blend_modes.clone()
                        }
                    } else {
                        run.fill_blend_modes.clone()
                    };
                    add_outline_shapes(&mut content, shape, raster_scale)?;
                }
                y += advance_y - dy;
                continue;
            }
            let width = (line.metrics.advance.ceil() as f32 + 1.).clamp(1., text::MAX_WIDTH);
            let w = (width * raster_scale).ceil() as u32;
            let h = (line.height * raster_scale).ceil() as u32;
            pixels += u64::from(w) * u64::from(h);
            if pixels > 16_000_000 {
                return Err(Error("styled text exceeds bitmap pixel budget"));
            }
            let mask = backend.raster_runs(&line.runs, width, raster_scale)?;
            if mask.width != w
                || mask.height != h
                || mask.rgba.len() != w as usize * h as usize * 4
                || mask.metrics.lines != 1
                || !mask.metrics.advance.is_finite()
                || !mask.metrics.baseline.is_finite()
                || (mask.metrics.advance - line.metrics.advance).abs() > 0.01
                || (mask.metrics.baseline - line.metrics.baseline).abs() > 0.01
            {
                return Err(Error(
                    "text backend violated styled bitmap or metric limits",
                ));
            }
            let rtl = line
                .runs
                .first()
                .is_some_and(|r| r.direction == text::TextDirection::RightToLeft);
            let x = t.rect.x + marker_x.unwrap_or_else(|| if rtl { 0. } else { line.indent }
                + place(t.align, line.available_width - line.metrics.advance as f32));
            let baseline = y + line.metrics.baseline as f32;
            let origin = [x, y + baseline.round() - baseline]
                .map(|v| (v * raster_scale).round() / raster_scale);
            content.shape(
                Shape::rect(Rect::new(
                    origin[0],
                    origin[1],
                    w as f32 / raster_scale,
                    h as f32 / raster_scale,
                ))
                .paint(Paint::Image(ImagePaint::new(Image::new([w, h], mask.rgba))))
                .transform(t.transform),
            );
        }
        y += advance_y - dy;
    }
    if let Some(stroke) = t.stroke
        && !stroke_paths.is_empty()
    {
        let mut shape = Shape::vector(t.rect, stroke_paths).transform(t.transform);
        shape.stroke = Some(stroke);
        shape.path_stroke = t.path_stroke;
        shape.dash_pattern = t.dash_pattern.clone();
        shape.stroke_geometry = t.stroke_geometry.clone();
        shape.stroke_paints = t.stroke_paints.clone();
        shape.stroke_blend_modes = t.stroke_blend_modes.clone();
        if shape.stroke_geometry.is_some() {
            content.shape(shape);
        } else {
            add_outline_shapes(&mut content, shape, raster_scale)?;
        }
    }
    let blended = t
        .stroke_blend_modes
        .iter()
        .chain(runs.iter().flat_map(|r| &r.fill_blend_modes))
        .any(|m| *m != crate::BlendMode::Normal);
    let mut scope = Composite::new(content).effect_transform(t.transform);
    if blended {
        scope.blend_mode = crate::BlendMode::PassThrough;
    }
    for (inner, effect) in [(false, t.drop_shadow), (true, t.inner_shadow)] {
        if let Some((color, offset, blur)) = effect {
            if color.iter().chain(&offset).any(|v| !v.is_finite()) || !(0. ..=64.).contains(&blur) {
                return Err(Error("invalid text shadow"));
            }
            scope.shadows.push(if inner {
                crate::Shadow::inner(color, offset, blur)
            } else {
                crate::Shadow::drop(color, offset, blur).show_behind()
            });
        }
    }
    scope.clips = t.clips.clone();
    if let Some(clip) = t.clip {
        scope
            .clips
            .push(crate::Clip::rect(clip).transform(t.clip_transform));
    }
    Ok(scope)
}

// Split spatially disjoint contours before GPU coverage work. Holes and touching
// glyphs stay together, so translucent paints are applied once to their union.
// Every piece keeps the original text box as its paint coordinate system.
fn add_outline_shapes(content: &mut Canvas, mut shape: Shape, scale: f32) -> Result<(), Error> {
    let paths = shape.vector_paths.take().unwrap_or_default();
    let mut groups: Vec<(Rect, Vec<(usize, crate::VectorPath)>)> = Vec::new();
    for (source, path) in paths.into_iter().enumerate() {
        let mut contours = Vec::new();
        for command in path.commands {
            if matches!(command, crate::path::Command::MoveTo(_)) {
                contours.push(crate::VectorPath::new(path.winding_rule));
            }
            contours
                .last_mut()
                .ok_or(Error("invalid text contour"))?
                .commands
                .push(command);
        }
        for contour in contours {
            let mut bounds = crate::path::bounds(std::slice::from_ref(&contour))?;
            if let Some(stroke) = shape.stroke {
                let [closed, open, _] = crate::path::stroke_outlines(
                    std::slice::from_ref(&contour),
                    stroke.width,
                    shape.path_stroke,
                    stroke.align,
                    scale,
                    &shape.dash_pattern,
                )?;
                for path in closed.iter().chain(&open) {
                    bounds = union(bounds, crate::path::bounds(std::slice::from_ref(path))?);
                }
            }
            // Include antialias fringe so separate draws cannot overlap coverage.
            bounds = bounds.inflate(1. / scale);
            let mut parts = vec![(source, contour)];
            let mut i = 0;
            while i < groups.len() {
                let other = groups[i].0;
                if bounds.x <= other.x + other.w
                    && other.x <= bounds.x + bounds.w
                    && bounds.y <= other.y + other.h
                    && other.y <= bounds.y + bounds.h
                {
                    let (r, mut p) = groups.swap_remove(i);
                    bounds = union(bounds, r);
                    parts.append(&mut p);
                    i = 0;
                } else {
                    i += 1;
                }
            }
            groups.push((bounds, parts));
        }
    }
    for (_, parts) in groups {
        let mut regions: std::collections::BTreeMap<usize, crate::VectorPath> = Default::default();
        for (source, path) in parts {
            regions
                .entry(source)
                .or_insert_with(|| crate::VectorPath::new(path.winding_rule))
                .commands
                .extend(path.commands);
        }
        let mut piece = shape.clone();
        piece.vector_paths = Some(regions.into_values().collect());
        if let Some(reason) = piece.problem() {
            return Err(Error(reason));
        }
        content.shape(piece);
    }
    Ok(())
}
fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect::new(
        x,
        y,
        (a.x + a.w).max(b.x + b.w) - x,
        (a.y + a.h).max(b.y + b.h) - y,
    )
}
