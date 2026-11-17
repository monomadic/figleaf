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
fn build(t: &Text, scale: f32, backend: &mut dyn Backend) -> Result<Composite, Error> {
    if !t.string.is_empty() {
        return Err(Error("styled text cannot also contain a plain string"));
    }
    t.rect.validate()?;
    t.transform.inverse()?;
    t.transform.bounds(t.rect).validate()?;
    let raster_scale = (scale * t.transform.max_scale()).max(scale).max(0.5);
    text::validate_runs(&t.runs, t.rect.w, raster_scale, t.wrap)?;
    let lines = text::layout_runs(backend, &t.runs, t.rect.w, t.wrap)?;
    let height: f32 = lines.iter().map(|l| l.height).sum();
    let place = |align, spare: f32| match align {
        crate::paint::Align::Start => 0.,
        crate::paint::Align::Center => spare / 2.,
        crate::paint::Align::End => spare,
    };
    let mut y = t.rect.y + place(t.vertical_align, t.rect.h - height);
    let mut pixels = 0u64;
    let mut content = Canvas::new();
    for line in lines {
        if !line.runs.is_empty() {
            if line.metrics.advance > f64::from(text::MAX_WIDTH) {
                return Err(Error("styled line exceeds bitmap width limit"));
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
            let x = t.rect.x + place(t.align, t.rect.w - line.metrics.advance as f32);
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
        y += line.height;
    }
    let mut scope = Composite::new(content).effect_transform(t.transform);
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
