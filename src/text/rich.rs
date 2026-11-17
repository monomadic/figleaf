//! Explicit styled runs. Offsets are derived from strings, never caller byte ranges.
use super::{Backend, MAX_TEXT_BYTES, Metrics, Style};
use crate::{Color, Error};

#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub style: Style,
    pub color: Color,
}
impl TextRun {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Style {
                font: 0,
                size: 12.,
                line_height: 16.,
                letter_spacing: 0.,
            },
            color: [0., 0., 0., 1.],
        }
    }
    pub fn font(mut self, font: usize) -> Self {
        self.style.font = font;
        self
    }
    pub fn size(mut self, size: f32, line_height: f32) -> Self {
        self.style.size = size;
        self.style.line_height = line_height;
        self
    }
    pub fn letter_spacing(mut self, value: f32) -> Self {
        self.style.letter_spacing = value;
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}
/// Straight-alpha encoded-sRGB pixels, top row first, for one shaped line.
pub struct RunMask {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub metrics: Metrics,
}
#[derive(Clone, Debug)]
pub struct RunLine {
    pub runs: Vec<TextRun>,
    pub height: f32,
    pub metrics: Metrics,
}

pub fn validate_runs(
    runs: &[TextRun],
    width: f32,
    scale: f32,
    paragraph: bool,
) -> Result<(), Error> {
    if runs.is_empty()
        || runs.len() > 128
        || runs.iter().map(|r| r.text.len()).sum::<usize>() > MAX_TEXT_BYTES
    {
        return Err(Error(
            "styled text requires 1 to 128 runs and at most 512 bytes",
        ));
    }
    for run in runs {
        if run.text.is_empty()
            || run
                .color
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Err(Error("invalid styled text run or color"));
        }
        let sample = if paragraph {
            run.text.replace('\n', " ")
        } else {
            run.text.clone()
        };
        super::validate(&sample, run.style, width, scale)?;
    }
    Ok(())
}
fn slice(runs: &[TextRun], start: usize, end: usize) -> Vec<TextRun> {
    let mut offset = 0;
    let mut out = Vec::new();
    for run in runs {
        let lo = start.saturating_sub(offset).min(run.text.len());
        let hi = end.saturating_sub(offset).min(run.text.len());
        if lo < hi {
            let mut r = run.clone();
            r.text = run.text[lo..hi].into();
            out.push(r);
        }
        offset += run.text.len();
    }
    out
}
fn measure(backend: &mut dyn Backend, runs: &[TextRun]) -> Result<Metrics, Error> {
    let m = backend.measure_runs(runs)?;
    if ![m.advance, m.ascent, m.descent, m.baseline]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.)
        || m.lines != 1
        || m.advance > 1e7
    {
        return Err(Error("text backend violated styled metric limits"));
    }
    Ok(m)
}
/// Wrap across run boundaries, measuring whole attributed lines. Empty paragraphs
/// retain the line height of their newline's run. Latin line breaking matches `wrap`.
pub fn layout_runs(
    backend: &mut dyn Backend,
    runs: &[TextRun],
    width: f32,
    wrap: bool,
) -> Result<Vec<RunLine>, Error> {
    validate_runs(runs, width, 1., wrap)?;
    let string: String = runs.iter().map(|r| r.text.as_str()).collect();
    let mut lines = Vec::new();
    let mut offset = 0;
    for paragraph in string.split('\n') {
        let mut start = offset;
        let end = offset + paragraph.len();
        loop {
            let mut stop = end;
            if wrap && start < end {
                let mut fit = None;
                let mut ends: Vec<usize> = string[start..end]
                    .char_indices()
                    .filter(|(i, c)| *c == ' ' && !string[start..start + *i].ends_with(' '))
                    .map(|(i, _)| start + i)
                    .collect();
                ends.push(end);
                for candidate in ends {
                    let trimmed = string[start..candidate].trim_end_matches(' ').len() + start;
                    let wide = if trimmed == start {
                        0.
                    } else {
                        measure(backend, &slice(runs, start, trimmed))?.advance
                    };
                    if wide > width as f64 + 0.001 {
                        break;
                    }
                    fit = Some(candidate);
                }
                stop = match fit {
                    Some(n) => n,
                    None => {
                        let mut cut = start + string[start..end].chars().next().unwrap().len_utf8();
                        for (i, c) in string[start..end].char_indices().skip(1) {
                            let candidate = start + i + c.len_utf8();
                            if measure(backend, &slice(runs, start, candidate))?.advance
                                > width as f64 + 0.001
                            {
                                break;
                            }
                            cut = candidate;
                        }
                        cut
                    }
                };
            }
            let trimmed = if wrap {
                start + string[start..stop].trim_end_matches(' ').len()
            } else {
                stop
            };
            let part = slice(runs, start, trimmed);
            let mut at = 0;
            let fallback = runs
                .iter()
                .find(|r| {
                    at += r.text.len();
                    start < at
                })
                .unwrap_or(runs.last().unwrap());
            let height = part
                .iter()
                .map(|r| r.style.line_height)
                .reduce(f32::max)
                .unwrap_or(fallback.style.line_height);
            let metrics = if part.is_empty() {
                Metrics {
                    lines: 1,
                    ..Default::default()
                }
            } else {
                measure(backend, &part)?
            };
            lines.push(RunLine {
                runs: part,
                height,
                metrics,
            });
            start = stop;
            if wrap {
                while start < end && string.as_bytes()[start] == b' ' {
                    start += 1;
                }
            }
            if start >= end {
                break;
            }
        }
        offset = end + 1;
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Mono;
    impl Backend for Mono {
        fn raster(
            &mut self,
            _: &str,
            _: Style,
            _: f32,
            _: f32,
        ) -> Result<super::super::Mask, Error> {
            Err(Error("measure only"))
        }
        fn measure_runs(&mut self, runs: &[TextRun]) -> Result<Metrics, Error> {
            Ok(Metrics {
                advance: runs
                    .iter()
                    .map(|r| r.text.chars().count() as f64 * 10.)
                    .sum(),
                lines: 1,
                ..Default::default()
            })
        }
    }
    #[test]
    fn styled_wrapping_crosses_runs_and_preserves_utf8_and_empty_lines() {
        let runs = vec![
            TextRun::new("hé"),
            TextRun::new("llo world\n\né").font(1).size(16., 24.),
        ];
        let lines = layout_runs(&mut Mono, &runs, 60., true).unwrap();
        let strings: Vec<String> = lines
            .iter()
            .map(|l| l.runs.iter().map(|r| r.text.as_str()).collect())
            .collect();
        assert_eq!(strings, ["héllo", "world", "", "é"]);
        assert_eq!(lines[0].runs.len(), 2);
        assert_eq!(lines[0].height, 24.);
        assert_eq!(lines[2].height, 24.);
        assert_eq!(lines[1].runs[0].style.font, 1);
        let long = layout_runs(&mut Mono, &[TextRun::new("ééé")], 10., true).unwrap();
        assert_eq!(long.len(), 3);
    }
    #[test]
    fn styled_validation_rejects_missing_styles_and_invalid_values() {
        for runs in [
            vec![],
            vec![TextRun::new("")],
            vec![TextRun::new("hi").font(64)],
            vec![TextRun::new("x").color([f32::NAN; 4])],
            vec![TextRun::new("x").size(12., 8.)],
            vec![TextRun::new("a"); 129],
            vec![TextRun::new("x".repeat(513))],
        ] {
            assert!(validate_runs(&runs, 100., 1., true).is_err());
        }
        assert!(validate_runs(&[TextRun::new("a\nb")], 100., 1., false).is_err());
        assert!(layout_runs(&mut super::super::NoText, &[TextRun::new("x")], 100., true).is_err());
    }
}
