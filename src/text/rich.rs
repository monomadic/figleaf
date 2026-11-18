//! Explicit styled runs. Offsets are derived from strings, never caller byte ranges.
use super::{Backend, MAX_TEXT_BYTES, Metrics, Style};
use crate::{Color, Error};

/// Ending truncation adds an ellipsis to text hidden by the box or line limit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextTruncation {
    #[default]
    Disabled,
    Ending,
}

/// Figma's basic text decoration, using the supplied font's metrics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum TextDecoration {
    #[default]
    None,
    Underline,
    Strikethrough,
}
/// Locale-independent Latin casing. Title capitalizes word initials and preserves
/// the rest of each word, as in Figma. Small caps need font-feature support.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextCase {
    #[default]
    Original,
    Upper,
    Lower,
    Title,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub style: Style,
    pub color: Color,
    pub text_decoration: TextDecoration,
    pub text_case: TextCase,
    /// None inherits the text layer’s paragraph setting.
    pub paragraph_spacing: Option<f32>,
    pub paragraph_indent: Option<f32>,
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
            text_decoration: TextDecoration::None,
            text_case: TextCase::Original,
            paragraph_spacing: None,
            paragraph_indent: None,
        }
    }
    pub fn text_decoration(mut self, value: TextDecoration) -> Self {
        self.text_decoration = value;
        self
    }
    pub fn text_case(mut self, value: TextCase) -> Self {
        self.text_case = value;
        self
    }
    pub fn paragraph_spacing(mut self, value: f32) -> Self {
        self.paragraph_spacing = Some(value);
        self
    }
    pub fn paragraph_indent(mut self, value: f32) -> Self {
        self.paragraph_indent = Some(value);
        self
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
    /// Use the supplied font’s natural line height.
    pub fn auto_line_height(mut self) -> Self {
        self.style.line_height = 0.;
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
    /// First-line inset; zero on wrapped continuation lines.
    pub indent: f32,
    pub available_width: f32,
    /// Extra gap after a paragraph break, never after the final paragraph.
    pub spacing_after: f32,
    // Offsets into the resolved source preserve whitespace and soft-wrap tails
    // when the final visible line is refitted with an ellipsis.
    source_start: usize,
    paragraph_end: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParagraphStyle {
    pub spacing: f32,
    pub indent: f32,
}
impl ParagraphStyle {
    pub fn validate(self, width: f32) -> Result<(), Error> {
        if !width.is_finite()
            || width <= 0.
            || !self.spacing.is_finite()
            || !(0. ..=1e7).contains(&self.spacing)
            || !self.indent.is_finite()
            || !(0. ..=1e7).contains(&self.indent)
            || self.indent >= width
        {
            return Err(Error("invalid paragraph spacing or first-line indent"));
        }
        Ok(())
    }
}
fn paragraph_style(run: &TextRun, default: ParagraphStyle) -> ParagraphStyle {
    ParagraphStyle {
        spacing: run.paragraph_spacing.unwrap_or(default.spacing),
        indent: run.paragraph_indent.unwrap_or(default.indent),
    }
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
        paragraph_style(run, ParagraphStyle::default()).validate(f32::MAX)?;
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
        let mut style = run.style;
        if style.line_height == 0. {
            style.line_height = style.size;
        }
        super::validate(&sample, style, width, scale)?;
    }
    Ok(())
}
/// Resolve Auto (zero) using the selected font. No guessed size multiplier.
pub fn resolve_style(backend: &mut dyn Backend, mut style: Style) -> Result<Style, Error> {
    if style.line_height == 0. {
        let mut check = style;
        check.line_height = check.size;
        super::validate("M", check, 1., 1.)?;
        let height = backend.natural_line_height(style)?;
        if !height.is_finite() || height < style.size || height > 256. {
            return Err(Error("text backend returned invalid automatic line height"));
        }
        style.line_height = height;
    }
    Ok(style)
}
pub fn resolve_runs(backend: &mut dyn Backend, runs: &[TextRun]) -> Result<Vec<TextRun>, Error> {
    let mut resolved = runs.to_vec();
    let mut in_word = false;
    for run in &mut resolved {
        let mut transformed = String::new();
        for c in run.text.chars() {
            let upper =
                run.text_case == TextCase::Upper || (run.text_case == TextCase::Title && !in_word);
            // Simple casing preserves character count (Figma does not expand ß).
            let mapped = if upper {
                let mut chars = c.to_uppercase();
                let first = chars.next().unwrap();
                if chars.next().is_none() { first } else { c }
            } else if run.text_case == TextCase::Lower {
                if c == '\u{0130}' {
                    'i'
                } else {
                    let mut chars = c.to_lowercase();
                    let first = chars.next().unwrap();
                    if chars.next().is_none() { first } else { c }
                }
            } else {
                c
            };
            transformed.push(mapped);
            if c.is_alphanumeric() {
                in_word = true;
            } else if c != '\'' && c != '\u{2019}' {
                in_word = false;
            }
        }
        run.text = transformed;
        // Resolution is idempotent: wrapping must not title-case continuation lines.
        run.text_case = TextCase::Original;
        run.style = resolve_style(backend, run.style)?;
    }
    let bytes: usize = resolved.iter().map(|r| r.text.len()).sum();
    if bytes > MAX_TEXT_BYTES
        || resolved.iter().any(|r| {
            r.text
                .chars()
                .any(|c| c != '\n' && !super::supported_char(c))
        })
    {
        return Err(Error(
            "case-transformed text exceeds the supported Latin scope or byte limit",
        ));
    }
    Ok(resolved)
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
    layout_paragraphs(backend, runs, width, wrap, ParagraphStyle::default())
}
/// Wrap and measure paragraphs with layer defaults and per-run overrides.
/// All runs in the same paragraph (including its newline) must agree on settings.
pub fn layout_paragraphs(
    backend: &mut dyn Backend,
    runs: &[TextRun],
    width: f32,
    wrap: bool,
    defaults: ParagraphStyle,
) -> Result<Vec<RunLine>, Error> {
    defaults.validate(width)?;
    validate_runs(runs, width, 1., wrap)?;
    let resolved = resolve_runs(backend, runs)?;
    let runs = resolved.as_slice();
    let string: String = runs.iter().map(|r| r.text.as_str()).collect();
    let mut lines = Vec::new();
    let mut offset = 0;
    for paragraph in string.split('\n') {
        let mut start = offset;
        let end = offset + paragraph.len();
        let covered_end = (end + 1).min(string.len());
        let mut at = 0;
        let mut settings = None;
        for run in runs {
            let lo = at;
            at += run.text.len();
            if lo < covered_end && at > offset {
                let current = paragraph_style(run, defaults);
                current.validate(width)?;
                if settings.is_some_and(|previous| previous != current) {
                    return Err(Error("conflicting styles within one paragraph"));
                }
                settings = Some(current);
            }
        }
        let settings = settings.unwrap_or_else(|| paragraph_style(runs.last().unwrap(), defaults));
        settings.validate(width)?;
        let mut first = true;
        loop {
            let indent = if first { settings.indent } else { 0. };
            let available_width = width - indent;
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
                    if wide > available_width as f64 + 0.001 {
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
                                > available_width as f64 + 0.001
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
                indent,
                available_width,
                spacing_after: 0.,
                source_start: start,
                paragraph_end: end,
            });
            start = stop;
            if wrap {
                while start < end && string.as_bytes()[start] == b' ' {
                    start += 1;
                }
            }
            if start >= end {
                if end < string.len() {
                    lines.last_mut().unwrap().spacing_after = settings.spacing;
                }
                break;
            }
            first = false;
        }
        offset = end + 1;
    }
    Ok(lines)
}

/// Layout and optionally truncate a text box. With Ending, line midpoints decide
/// height visibility (including paragraph gaps); at least the first line remains.
/// `max_lines` only takes effect with Ending. Returned metrics include ellipses.
pub fn layout_text(
    backend: &mut dyn Backend,
    runs: &[TextRun],
    size: [f32; 2],
    wrap: bool,
    paragraph: ParagraphStyle,
    truncation: TextTruncation,
    max_lines: Option<usize>,
) -> Result<Vec<RunLine>, Error> {
    validate_truncation(size[1], max_lines)?;
    validate_runs(runs, size[0], 1., wrap)?;
    let resolved = resolve_runs(backend, runs)?;
    let mut lines = layout_paragraphs(backend, &resolved, size[0], wrap, paragraph)?;
    if truncation == TextTruncation::Disabled {
        return Ok(lines);
    }
    let mut top = 0.;
    let mut visible = 0;
    for line in &lines {
        if visible >= max_lines.unwrap_or(usize::MAX)
            || (visible > 0 && top + line.height / 2. > size[1] + 0.001)
        {
            break;
        }
        visible += 1;
        top += line.height + line.spacing_after;
    }
    let hidden = visible < lines.len();
    lines.truncate(visible);
    let source: String = resolved.iter().map(|r| r.text.as_str()).collect();
    let last = lines.len().saturating_sub(1);
    for (index, line) in lines.iter_mut().enumerate() {
        if index == last && hidden || line.metrics.advance > f64::from(line.available_width) + 0.001
        {
            ellipsize(backend, &resolved, &source, line)?;
        }
    }
    Ok(lines)
}
pub(crate) fn validate_truncation(height: f32, max_lines: Option<usize>) -> Result<(), Error> {
    if !height.is_finite()
        || !(0. ..=1e7).contains(&height)
        || max_lines.is_some_and(|n| n == 0 || n > 10_000_000)
    {
        return Err(Error("invalid text height or maximum line count"));
    }
    Ok(())
}
fn ellipsize(
    backend: &mut dyn Backend,
    runs: &[TextRun],
    source: &str,
    line: &mut RunLine,
) -> Result<(), Error> {
    let mut best = None;
    let mut previous_fits = false;
    // Include the whole remaining paragraph, not just the old wrapped line.
    // Shape every candidate as a complete attributed line to retain kerning.
    for end in source[line.source_start..line.paragraph_end]
        .char_indices()
        .map(|(i, _)| line.source_start + i)
        .chain(std::iter::once(line.paragraph_end))
    {
        let end = line.source_start + source[line.source_start..end].trim_end_matches(' ').len();
        let mut offset = 0;
        let style = runs
            .iter()
            .find(|r| {
                offset += r.text.len();
                end < offset
            })
            .unwrap_or_else(|| runs.last().unwrap());
        let mut ellipsis = style.clone();
        ellipsis.text = "…".into();
        // Figma's ellipsis takes the first omitted character's style, but is not
        // underlined or struck through along with the source text.
        ellipsis.text_decoration = TextDecoration::None;
        let mut candidate = slice(runs, line.source_start, end);
        let merged = candidate.last_mut().is_some_and(|last| {
            let mut comparable = last.clone();
            comparable.text.clone_from(&ellipsis.text);
            if comparable == ellipsis {
                last.text.push('…');
                true
            } else {
                false
            }
        });
        if !merged {
            candidate.push(ellipsis);
        }
        // Do not shorten an otherwise fitting line just to satisfy a resource
        // cap. Oversized candidates beyond the width do not invalidate an earlier
        // fitting prefix of a valid 512-byte source.
        if candidate.iter().map(|r| r.text.len()).sum::<usize>() > MAX_TEXT_BYTES
            || candidate.len() > 128
        {
            if previous_fits {
                return Err(Error("ellipsis exceeds the styled text resource limits"));
            }
            break;
        }
        let metrics = measure(backend, &candidate)?;
        previous_fits = metrics.advance <= f64::from(line.available_width) + 0.001;
        if previous_fits || best.is_none() {
            let height = candidate
                .iter()
                .map(|r| r.style.line_height)
                .reduce(f32::max)
                .unwrap();
            best = Some((candidate, metrics, height));
        }
    }
    let (runs, metrics, height) = best.unwrap();
    line.runs = runs;
    line.metrics = metrics;
    line.height = height;
    line.spacing_after = 0.;
    Ok(())
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

#[cfg(test)]
mod auto_tests {
    use super::*;
    struct FontMetrics(f32);
    impl Backend for FontMetrics {
        fn natural_line_height(&mut self, style: Style) -> Result<f32, Error> {
            Ok(self.0 * style.size)
        }
        fn raster(
            &mut self,
            _: &str,
            _: Style,
            _: f32,
            _: f32,
        ) -> Result<super::super::Mask, Error> {
            Err(Error("measurement backend"))
        }
        fn measure_runs(&mut self, runs: &[TextRun]) -> Result<Metrics, Error> {
            Ok(Metrics {
                advance: runs.iter().map(|r| r.text.len() as f64 * 5.).sum(),
                lines: 1,
                ..Default::default()
            })
        }
    }
    #[test]
    fn auto_resolves_each_font_size_before_layout_and_keeps_empty_lines() {
        let runs = vec![
            TextRun::new("large").size(20., 20.).auto_line_height(),
            TextRun::new("\n\nsmall").size(10., 10.).auto_line_height(),
        ];
        let lines = layout_runs(&mut FontMetrics(1.25), &runs, 100., true).unwrap();
        assert_eq!(
            lines.iter().map(|l| l.height).collect::<Vec<_>>(),
            [25., 12.5, 12.5]
        );
        assert_eq!(runs[0].style.line_height, 0.);
        let explicit = TextRun::new("explicit").size(16., 32.);
        assert_eq!(
            resolve_runs(&mut super::super::NoText, &[explicit]).unwrap()[0]
                .style
                .line_height,
            32.
        );
    }
    #[test]
    fn auto_rejects_invalid_metrics_and_unsupported_backends() {
        let runs = [TextRun::new("hello").auto_line_height()];
        for metric in [f32::NAN, f32::INFINITY, 0., -1., 100.] {
            assert!(layout_runs(&mut FontMetrics(metric), &runs, 100., true).is_err());
        }
        assert!(layout_runs(&mut super::super::NoText, &runs, 100., true).is_err());
    }
}

#[cfg(test)]
mod paragraph_tests {
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
    fn paragraph_breaks_space_once_and_only_first_lines_indent() {
        let lines = layout_paragraphs(
            &mut Mono,
            &[TextRun::new("one two three\n\nlast")],
            80.,
            true,
            ParagraphStyle {
                spacing: 10.,
                indent: 30.,
            },
        )
        .unwrap();
        let strings: Vec<String> = lines
            .iter()
            .map(|l| l.runs.iter().map(|r| r.text.as_str()).collect())
            .collect();
        assert_eq!(strings, ["one", "two", "three", "", "last"]);
        assert_eq!(
            lines.iter().map(|l| l.indent).collect::<Vec<_>>(),
            [30., 0., 0., 30., 30.]
        );
        assert_eq!(
            lines.iter().map(|l| l.spacing_after).collect::<Vec<_>>(),
            [0., 0., 10., 10., 0.]
        );
        assert_eq!(
            lines
                .iter()
                .map(|l| l.height + l.spacing_after)
                .sum::<f32>(),
            100.
        );
    }
    #[test]
    fn paragraph_overrides_follow_newlines_and_have_no_final_gap() {
        let runs = [
            TextRun::new("one\n")
                .paragraph_spacing(6.)
                .paragraph_indent(10.),
            TextRun::new("two\n")
                .paragraph_spacing(12.)
                .paragraph_indent(20.),
            TextRun::new("last")
                .paragraph_spacing(100.)
                .paragraph_indent(0.),
        ];
        let lines = layout_paragraphs(
            &mut Mono,
            &runs,
            100.,
            true,
            ParagraphStyle {
                spacing: 50.,
                indent: 5.,
            },
        )
        .unwrap();
        assert_eq!(
            lines.iter().map(|l| l.spacing_after).collect::<Vec<_>>(),
            [6., 12., 0.]
        );
        assert_eq!(
            lines.iter().map(|l| l.indent).collect::<Vec<_>>(),
            [10., 20., 0.]
        );
        let trailing = layout_runs(
            &mut Mono,
            &[TextRun::new("a\n").paragraph_spacing(8.)],
            100.,
            true,
        )
        .unwrap();
        assert_eq!(trailing.len(), 2);
        assert_eq!(trailing[0].spacing_after, 8.);
        assert_eq!(trailing[1].spacing_after, 0.);
    }
    #[test]
    fn malformed_or_conflicting_paragraph_styles_fail_closed() {
        for style in [
            ParagraphStyle {
                spacing: -1.,
                indent: 0.,
            },
            ParagraphStyle {
                spacing: f32::NAN,
                indent: 0.,
            },
            ParagraphStyle {
                spacing: 0.,
                indent: -1.,
            },
            ParagraphStyle {
                spacing: 0.,
                indent: 80.,
            },
            ParagraphStyle {
                spacing: 0.,
                indent: f32::INFINITY,
            },
        ] {
            assert!(
                layout_paragraphs(&mut Mono, &[TextRun::new("text")], 80., true, style).is_err()
            );
        }
        let conflict = [
            TextRun::new("same ").paragraph_spacing(4.),
            TextRun::new("paragraph").paragraph_spacing(8.),
        ];
        assert!(layout_runs(&mut Mono, &conflict, 200., true).is_err());
    }
}

#[cfg(test)]
mod case_tests {
    use super::*;
    #[test]
    fn case_preserves_source_and_word_context_across_runs() {
        let runs = [
            TextRun::new("title CA").text_case(TextCase::Title),
            TextRun::new("SE: don't stop-at O’NEILL 2nd").text_case(TextCase::Title),
        ];
        let resolved = resolve_runs(&mut super::super::NoText, &runs).unwrap();
        assert_eq!(
            resolved.iter().map(|r| r.text.as_str()).collect::<String>(),
            "Title CASE: Don't Stop-At O’NEILL 2nd"
        );
        assert_eq!(runs[0].text, "title CA");
        assert_eq!(
            resolve_runs(&mut super::super::NoText, &resolved).unwrap(),
            resolved
        );
    }
    #[test]
    fn simple_case_and_transformed_scope_are_validated() {
        let runs = [
            TextRun::new("Straße déjà vu").text_case(TextCase::Upper),
            TextRun::new(" İ MIXED").text_case(TextCase::Lower),
        ];
        let resolved = resolve_runs(&mut super::super::NoText, &runs).unwrap();
        assert_eq!(resolved[0].text, "STRAßE DÉJÀ VU");
        assert_eq!(resolved[1].text, " i mixed");
        // Uppercase micro sign maps to Greek, outside the current Latin scope.
        assert!(
            resolve_runs(
                &mut super::super::NoText,
                &[TextRun::new("µ").text_case(TextCase::Upper)]
            )
            .is_err()
        );
        // Latin s with swash tail grows from 2 UTF-8 bytes to 3 when uppercased.
        assert!(
            resolve_runs(
                &mut super::super::NoText,
                &[TextRun::new("ȿ").text_case(TextCase::Upper)]
            )
            .is_err()
        );
        assert!(resolve_runs(&mut super::super::NoText, &[TextRun::new("a".repeat(513))]).is_err());
    }
}

#[cfg(test)]
mod truncation_tests {
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
                baseline: 20.,
                ascent: 20.,
                descent: 8.,
                lines: 1,
                ..Default::default()
            })
        }
    }
    const SOURCE: &str = "Alpha beta gamma delta epsilon zeta eta theta";
    fn layout(height: f32, limit: Option<usize>, mode: TextTruncation) -> Vec<RunLine> {
        layout_text(
            &mut Mono,
            &[TextRun::new(SOURCE).size(20., 28.)],
            [200., height],
            true,
            ParagraphStyle::default(),
            mode,
            limit,
        )
        .unwrap()
    }
    fn strings(lines: &[RunLine]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.runs.iter().map(|r| r.text.as_str()).collect())
            .collect()
    }
    #[test]
    fn last_line_refits_the_remaining_paragraph_and_partial_height_matches_figma() {
        for height in [0., 10., 28., 41.] {
            let lines = layout(height, None, TextTruncation::Ending);
            assert_eq!(strings(&lines), ["Alpha beta gamma de…"]);
            assert_eq!(lines[0].metrics.advance, 200.);
        }
        for height in [42., 55., 56.] {
            assert_eq!(
                strings(&layout(height, None, TextTruncation::Ending)),
                ["Alpha beta gamma", "delta epsilon zeta…"]
            );
        }
        assert_eq!(
            strings(&layout(200., Some(1), TextTruncation::Ending)),
            ["Alpha beta gamma de…"]
        );
        assert_eq!(
            strings(&layout(0., Some(1), TextTruncation::Disabled)),
            ["Alpha beta gamma", "delta epsilon zeta", "eta theta"]
        );
        assert_eq!(
            strings(&layout(200., Some(3), TextTruncation::Ending)),
            ["Alpha beta gamma", "delta epsilon zeta", "eta theta"]
        );
    }
    #[test]
    fn paragraph_gaps_indent_and_empty_lines_keep_their_meaning() {
        let runs = [TextRun::new(
            "First paragraph\nSecond paragraph has a longer ending\nThird paragraph",
        )
        .size(20., 28.)];
        let paragraph = ParagraphStyle {
            spacing: 12.,
            indent: 20.,
        };
        let lines = layout_text(
            &mut Mono,
            &runs,
            [200., 85.],
            true,
            paragraph,
            TextTruncation::Ending,
            None,
        )
        .unwrap();
        assert_eq!(
            strings(&lines),
            [
                "First paragraph",
                "Second paragraph",
                "has a longer ending…"
            ]
        );
        assert_eq!(lines[0].spacing_after, 12.);
        assert_eq!(lines[2].spacing_after, 0.);
        let lines = layout_text(
            &mut Mono,
            &runs,
            [200., 76.],
            true,
            paragraph,
            TextTruncation::Ending,
            None,
        )
        .unwrap();
        assert_eq!(strings(&lines), ["First paragraph", "Second paragraph…"]);
        assert_eq!(lines[1].spacing_after, 0.);
        let lines = layout_text(
            &mut Mono,
            &[TextRun::new("a\n\nb").size(20., 28.)],
            [200., 200.],
            true,
            ParagraphStyle::default(),
            TextTruncation::Ending,
            Some(2),
        )
        .unwrap();
        assert_eq!(strings(&lines), ["a", "…"]);
    }
    #[test]
    fn ellipsis_uses_first_omitted_style_without_decoration_or_case_changes() {
        let runs = [
            TextRun::new("Alpha beta gamma de").size(20., 28.),
            TextRun::new("lta epsilon zeta")
                .size(20., 28.)
                .color([1., 0., 0., 1.])
                .text_decoration(TextDecoration::Underline)
                .text_case(TextCase::Upper),
        ];
        let lines = layout_text(
            &mut Mono,
            &runs,
            [200., 28.],
            true,
            ParagraphStyle::default(),
            TextTruncation::Ending,
            None,
        )
        .unwrap();
        assert_eq!(strings(&lines), ["Alpha beta gamma de…"]);
        let ellipsis = lines[0].runs.last().unwrap();
        assert_eq!(ellipsis.text, "…");
        assert_eq!(ellipsis.color, [1., 0., 0., 1.]);
        assert_eq!(ellipsis.text_decoration, TextDecoration::None);
        assert_eq!(runs[1].text, "lta epsilon zeta");
        // Removing source characters cannot split a UTF-8 character.
        let lines = layout_text(
            &mut Mono,
            &[TextRun::new("é".repeat(256))],
            [25., 10.],
            false,
            ParagraphStyle::default(),
            TextTruncation::Ending,
            None,
        )
        .unwrap();
        assert_eq!(strings(&lines), ["é…"]);
    }
    #[test]
    fn narrow_boxes_and_full_source_budget_are_bounded() {
        let lines = layout_text(
            &mut Mono,
            &[TextRun::new("a".repeat(512))],
            [200., 10.],
            false,
            ParagraphStyle::default(),
            TextTruncation::Ending,
            None,
        )
        .unwrap();
        assert_eq!(strings(&lines), [format!("{}…", "a".repeat(19))]);
        let lines = layout_text(
            &mut Mono,
            &[TextRun::new("abc")],
            [1., 10.],
            false,
            ParagraphStyle::default(),
            TextTruncation::Ending,
            None,
        )
        .unwrap();
        assert_eq!(strings(&lines), ["…"]);
        for (height, limit) in [
            (f32::NAN, None),
            (-1., None),
            (10., Some(0)),
            (10., Some(usize::MAX)),
        ] {
            assert!(
                layout_text(
                    &mut Mono,
                    &[TextRun::new("abc")],
                    [100., height],
                    true,
                    ParagraphStyle::default(),
                    TextTruncation::Ending,
                    limit
                )
                .is_err()
            );
        }
    }
    #[test]
    fn ellipsis_failures_are_errors_instead_of_fallback_or_extra_shortening() {
        struct Tiny {
            missing_ellipsis: bool,
        }
        impl Backend for Tiny {
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
                if self.missing_ellipsis && runs.iter().any(|r| r.text.contains('…')) {
                    return Err(Error("font has no ellipsis"));
                }
                Ok(Metrics {
                    advance: runs.iter().map(|r| r.text.chars().count() as f64).sum(),
                    lines: 1,
                    ..Default::default()
                })
            }
        }
        let args = &[TextRun::new("first\nsecond")];
        assert!(
            layout_text(
                &mut Tiny {
                    missing_ellipsis: true
                },
                args,
                [100., 10.],
                true,
                ParagraphStyle::default(),
                TextTruncation::Ending,
                None
            )
            .is_err()
        );
        let source = format!("{}\nb", "a".repeat(510));
        // The fitting prefix plus ellipsis would exceed the byte budget. Do not
        // quietly remove extra characters to make that resource limit fit.
        assert!(
            layout_text(
                &mut Tiny {
                    missing_ellipsis: false
                },
                &[TextRun::new(source)],
                [600., 10.],
                true,
                ParagraphStyle::default(),
                TextTruncation::Ending,
                None
            )
            .is_err()
        );
    }
}
