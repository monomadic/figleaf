//! Explicit styled runs. Offsets are derived from strings, never caller byte ranges.
use super::{Backend, MAX_TEXT_BYTES, Metrics, Style, TextDirection};
use crate::{Color, Error};

/// A horizontal OpenType feature tag and its explicit on/off value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OpenTypeFeature {
    pub tag: [u8; 4],
    pub enabled: bool,
}
impl OpenTypeFeature {
    pub const fn new(tag: [u8; 4], enabled: bool) -> Self {
        Self { tag, enabled }
    }
}
/// Figma's paragraph list style. Counters are derived from paragraph order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListOptions {
    #[default]
    None,
    Unordered,
    Ordered,
}

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
/// Figma underline geometry. Percent values are relative to the font size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum DecorationValue {
    #[default]
    Auto,
    Pixels(f32),
    Percent(f32),
}
impl DecorationValue {
    pub(crate) fn resolve(self, size: f32) -> Option<f32> {
        match self {
            Self::Auto => None,
            Self::Pixels(v) => Some(v),
            Self::Percent(v) => Some(v * size / 100.),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum TextDecorationStyle {
    #[default]
    Solid,
    Wavy,
    Dotted,
}
/// Custom underline settings; automatic color follows the run's paints.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecorationOptions {
    pub style: TextDecorationStyle,
    pub offset: DecorationValue,
    pub thickness: DecorationValue,
    pub color: Option<Color>,
    pub skip_ink: bool,
}
impl Default for DecorationOptions {
    fn default() -> Self {
        Self {
            style: TextDecorationStyle::Solid,
            offset: DecorationValue::Auto,
            thickness: DecorationValue::Auto,
            color: None,
            skip_ink: true,
        }
    }
}
impl DecorationOptions {
    pub fn validate(self, decoration: TextDecoration, size: f32) -> Result<(), Error> {
        if self != Self::default() && decoration != TextDecoration::Underline {
            return Err(Error("custom decoration settings require an underline"));
        }
        if self
            .offset
            .resolve(size)
            .is_some_and(|v| !v.is_finite() || v.abs() > 2048.)
            || self
                .thickness
                .resolve(size)
                .is_some_and(|v| !v.is_finite() || !(0. ..=128.).contains(&v))
            || self
                .color
                .is_some_and(|c| c.iter().any(|v| !v.is_finite() || !(0. ..=1.).contains(v)))
        {
            return Err(Error("invalid custom decoration dimensions or color"));
        }
        Ok(())
    }
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
    SmallCaps,
    SmallCapsForced,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub style: Style,
    pub color: Color,
    /// Explicit paint stack; None uses color, Some(empty) leaves glyphs unfilled.
    pub paints: Option<Vec<crate::Paint>>,
    pub text_decoration: TextDecoration,
    pub decoration_options: DecorationOptions,
    pub text_case: TextCase,
    pub open_type_features: Vec<OpenTypeFeature>,
    pub list_options: ListOptions,
    /// List nesting level: 1–5; zero for ordinary paragraphs.
    pub indentation: usize,
    pub list_spacing: f32,
    pub hanging_list: bool,
    /// Auto resolves once per paragraph and is retained across wrapped lines.
    pub direction: TextDirection,
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
            paints: None,
            text_decoration: TextDecoration::None,
            decoration_options: DecorationOptions::default(),
            text_case: TextCase::Original,
            open_type_features: Vec::new(),
            list_options: ListOptions::None,
            indentation: 0,
            list_spacing: 0.,
            hanging_list: false,
            direction: TextDirection::Auto,
            paragraph_spacing: None,
            paragraph_indent: None,
        }
    }
    pub fn open_type_features(mut self, features: Vec<OpenTypeFeature>) -> Self {
        self.open_type_features = features;
        self
    }
    pub fn list_options(mut self, options: ListOptions) -> Self {
        self.list_options = options;
        if options != ListOptions::None && self.indentation == 0 {
            self.indentation = 1;
        }
        self
    }
    pub fn indentation(mut self, level: usize) -> Self {
        self.indentation = level;
        self
    }
    pub fn list_spacing(mut self, spacing: f32) -> Self {
        self.list_spacing = spacing;
        self
    }
    pub fn hanging_list(mut self, hanging: bool) -> Self {
        self.hanging_list = hanging;
        self
    }
    pub fn text_decoration_style(mut self, value: TextDecorationStyle) -> Self {
        self.decoration_options.style = value;
        self
    }
    pub fn text_decoration_offset(mut self, value: DecorationValue) -> Self {
        self.decoration_options.offset = value;
        self
    }
    pub fn text_decoration_thickness(mut self, value: DecorationValue) -> Self {
        self.decoration_options.thickness = value;
        self
    }
    pub fn text_decoration_color(mut self, value: Color) -> Self {
        self.decoration_options.color = Some(value);
        self
    }
    pub fn text_decoration_skip_ink(mut self, value: bool) -> Self {
        self.decoration_options.skip_ink = value;
        self
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
    pub fn paints(mut self, paints: Vec<crate::Paint>) -> Self {
        self.paints = Some(paints);
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
    /// Leading-edge inset: paragraph indent on the first line, plus list gutter.
    pub indent: f32,
    pub available_width: f32,
    /// Extra gap after a paragraph break, never after the final paragraph.
    pub spacing_after: f32,
    /// First-line marker, positioned independently of the editable source.
    pub marker: Option<ListMarker>,
    /// Lists retain their initial marker baseline across item style changes.
    pub baseline_offset: f32,
    // Offsets into the resolved source preserve whitespace and soft-wrap tails
    // when the final visible line is refitted with an ellipsis.
    source_start: usize,
    paragraph_end: usize,
}

#[derive(Clone, Debug)]
pub struct ListMarker {
    pub runs: Vec<TextRun>,
    pub metrics: Metrics,
    /// Leading-edge marker position before alignment; mirror for RTL paragraphs.
    pub x: f32,
}
#[derive(Clone)]
struct ListParagraph {
    marker: Option<ListMarker>,
    inset: f32,
    options: ListOptions,
    spacing: f32,
}
fn list_settings(r: &TextRun) -> (ListOptions, usize, f32, bool) {
    (
        r.list_options,
        r.indentation,
        r.list_spacing,
        r.hanging_list,
    )
}
fn counter(n: usize, level: usize) -> String {
    match (level - 1) % 3 {
        0 => n.to_string(),
        1 => {
            let mut n = n;
            let mut s = Vec::new();
            while n > 0 {
                n -= 1;
                s.push((b'a' + (n % 26) as u8) as char);
                n /= 26;
            }
            s.into_iter().rev().collect()
        }
        _ => {
            let mut n = n;
            let mut s = String::new();
            for (v, d) in [
                (1000, "m"),
                (900, "cm"),
                (500, "d"),
                (400, "cd"),
                (100, "c"),
                (90, "xc"),
                (50, "l"),
                (40, "xl"),
                (10, "x"),
                (9, "ix"),
                (5, "v"),
                (4, "iv"),
                (1, "i"),
            ] {
                while n >= v {
                    s.push_str(d);
                    n -= v;
                }
            }
            s
        }
    }
}
fn list_paragraphs(
    backend: &mut dyn Backend,
    runs: &[TextRun],
    string: &str,
) -> Result<Vec<ListParagraph>, Error> {
    let mut records = Vec::new();
    let mut offset = 0;
    let mut counts = [0usize; 5];
    let mut group = 0usize;
    let mut previous = ListOptions::None;
    let mut prior_style: Option<TextRun> = None;
    let mut widths = std::collections::BTreeMap::<(usize, usize), f32>::new();
    for paragraph in string.split('\n') {
        let end = offset + paragraph.len();
        let parts = slice(runs, offset, (end + 1).min(string.len()));
        let first = parts.first().unwrap_or(runs.last().unwrap());
        let settings = list_settings(first);
        if parts.iter().any(|r| list_settings(r) != settings) {
            return Err(Error("conflicting list styles within one paragraph"));
        }
        let (options, level, spacing, hanging) = settings;
        if options != previous {
            counts = [0; 5];
            group += 1;
            prior_style = None;
        }
        if prior_style.is_none() {
            prior_style = Some(first.clone());
        }
        let marker = if options == ListOptions::None {
            None
        } else {
            counts[level - 1] += 1;
            counts[level..].fill(0);
            let mut marker = prior_style.clone().unwrap_or_else(|| first.clone());
            marker.text = if options == ListOptions::Unordered {
                "•".into()
            } else {
                format!("{}.", counter(counts[level - 1], level))
            };
            marker.direction = first.direction;
            marker.text_decoration = TextDecoration::None;
            marker.decoration_options = DecorationOptions::default();
            marker.list_options = ListOptions::None;
            marker.indentation = 0;
            marker.list_spacing = 0.;
            marker.hanging_list = false;
            marker.paragraph_spacing = None;
            marker.paragraph_indent = None;
            let mut decimal = marker.clone();
            decimal.text = format!("{}.", counts[level - 1]);
            let decimal_runs = resolve_runs(backend, &[decimal])?;
            let gutter_width = if options == ListOptions::Ordered {
                measure(backend, &decimal_runs)?.advance as f32
            } else {
                0.
            };
            let marker_runs = resolve_runs(backend, &[marker])?;
            let metrics = measure(backend, &marker_runs)?;
            let width = widths.entry((group, level)).or_default();
            *width = width.max(gutter_width);
            Some(ListMarker {
                runs: marker_runs,
                metrics,
                x: 0.,
            })
        };
        records.push((
            ListParagraph {
                marker,
                inset: 0.,
                options,
                spacing,
            },
            group,
            level,
            prior_style.as_ref().unwrap().style.size,
            hanging,
        ));
        previous = options;
        offset = end + 1;
    }
    Ok(records
        .into_iter()
        .map(|(mut p, group, level, size, hanging)| {
            if let Some(marker) = &mut p.marker {
                let rtl = marker.runs[0].direction == TextDirection::RightToLeft;
                let gap = if rtl { size * 0.3 } else { 0. };
                let gutter = (size * 1.2).max(widths[&(group, level)]) + gap;
                p.inset =
                    (level - 1) as f32 * (size * 1.2 + gap) + if hanging { 0. } else { gutter };
                marker.x = p.inset - size * 0.3 - marker.metrics.advance as f32;
            }
            p
        })
        .collect())
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
        run.decoration_options
            .validate(run.text_decoration, run.style.size)?;
        validate_features(&run.open_type_features)?;
        if run.indentation > 5
            || run.list_options != ListOptions::None && run.indentation == 0
            || !run.list_spacing.is_finite()
            || !(0. ..=1e7).contains(&run.list_spacing)
        {
            return Err(Error("invalid list indentation or spacing"));
        }
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
pub(crate) fn validate_features(features: &[OpenTypeFeature]) -> Result<(), Error> {
    if features.len() > 64 {
        return Err(Error("more than 64 OpenType overrides"));
    }
    for (i, f) in features.iter().enumerate() {
        if !f
            .tag
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            || features[..i].iter().any(|p| p.tag == f.tag)
            || matches!(
                &f.tag,
                b"vert"
                    | b"vrt2"
                    | b"vkrn"
                    | b"vpal"
                    | b"vhal"
                    | b"valt"
                    | b"vkna"
                    | b"vchw"
                    | b"vrtr"
                    | b"rand"
            )
        {
            return Err(Error(
                "invalid, duplicate, vertical or random OpenType feature",
            ));
        }
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
        run.decoration_options
            .validate(run.text_decoration, run.style.size)?;
        validate_features(&run.open_type_features)?;
        if matches!(
            run.text_case,
            TextCase::SmallCaps | TextCase::SmallCapsForced
        ) {
            let tags: &[[u8; 4]] = if run.text_case == TextCase::SmallCapsForced {
                &[*b"smcp", *b"c2sc"]
            } else {
                &[*b"smcp"]
            };
            for tag in tags {
                if let Some(feature) = run.open_type_features.iter().find(|f| f.tag == *tag) {
                    if !feature.enabled {
                        return Err(Error("small caps conflicts with OpenType override"));
                    }
                } else {
                    run.open_type_features
                        .push(OpenTypeFeature::new(*tag, true));
                }
            }
        }
        run.open_type_features.sort();
        run.decoration_options
            .validate(run.text_decoration, run.style.size)?;
        validate_features(&run.open_type_features)?;
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
        if run.text_case != TextCase::Original
            && transformed
                .chars()
                .any(|c| c != '\n' && !super::latin_char(c))
        {
            return Err(Error(
                "script-specific case transformations are unsupported",
            ));
        }
        run.text = transformed;
        // Resolution is idempotent: wrapping must not title-case continuation lines.
        run.text_case = TextCase::Original;
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
    let mut resolved = backend.resolve_fonts(&resolved)?;
    for run in &mut resolved {
        run.style = resolve_style(backend, run.style)?;
        backend.validate_decoration(run.decoration_options)?;
        backend.validate_open_type_features(run.style, &run.open_type_features)?;
    }
    Ok(resolved)
}
pub(super) fn slice(runs: &[TextRun], start: usize, end: usize) -> Vec<TextRun> {
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
    let boundaries = backend.text_boundaries(&string)?;
    boundaries.validate(&string)?;
    let lists = list_paragraphs(backend, runs, &string)?;
    let mut lines = Vec::new();
    let mut offset = 0;
    for (paragraph_index, paragraph) in string.split('\n').enumerate() {
        let mut start = offset;
        let end = offset + paragraph.len();
        let covered_end = (end + 1).min(string.len());
        let mut at = 0;
        let mut settings = None;
        let mut direction = None;
        for run in runs {
            let lo = at;
            at += run.text.len();
            if lo < covered_end && at > offset {
                if direction.is_some_and(|d| d != run.direction) {
                    return Err(Error("conflicting paragraph direction"));
                }
                direction = Some(run.direction);
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
            let list = &lists[paragraph_index];
            let indent = list.inset + if first { settings.indent } else { 0. };
            if indent >= width {
                return Err(Error("list indentation leaves no line width"));
            }
            let available_width = width - indent;
            let mut stop = end;
            if wrap && start < end {
                let mut fit = None;
                let mut ends: Vec<usize> = boundaries
                    .lines
                    .iter()
                    .copied()
                    .filter(|&i| i > start && i < end)
                    .collect();
                ends.push(end);
                for candidate in ends {
                    let trimmed = string[start..candidate]
                        .trim_end_matches(super::break_space)
                        .len()
                        + start;
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
                        let mut clusters = boundaries
                            .graphemes
                            .iter()
                            .copied()
                            .filter(|&i| i > start && i <= end);
                        let mut cut = clusters.next().ok_or(Error("missing grapheme boundary"))?;
                        for candidate in clusters {
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
                start
                    + string[start..stop]
                        .trim_end_matches(super::break_space)
                        .len()
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
                marker: if first {
                    lists[paragraph_index].marker.clone().map(|mut m| {
                        m.x += settings.indent;
                        m
                    })
                } else {
                    None
                },
                baseline_offset: lists[paragraph_index].marker.as_ref().map_or(0., |m| {
                    let marker_height = m
                        .runs
                        .iter()
                        .map(|r| r.style.line_height)
                        .reduce(f32::max)
                        .unwrap();
                    m.metrics.baseline as f32 + (height - marker_height) / 2.
                        - metrics.baseline as f32
                }),
                source_start: start,
                paragraph_end: end,
            });
            start = stop;
            if wrap {
                while start < end {
                    let c = string[start..end].chars().next().unwrap();
                    if !super::break_space(c) {
                        break;
                    }
                    start += c.len_utf8();
                }
            }
            if start >= end {
                if end < string.len() {
                    let list = &lists[paragraph_index];
                    lines.last_mut().unwrap().spacing_after = if list.options != ListOptions::None
                        && lists
                            .get(paragraph_index + 1)
                            .is_some_and(|p| p.options != ListOptions::None)
                    {
                        list.spacing
                    } else {
                        settings.spacing
                    };
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
    let boundaries = backend.text_boundaries(source)?;
    boundaries.validate(source)?;
    let rtl = line
        .runs
        .first()
        .is_some_and(|r| r.direction == TextDirection::RightToLeft)
        && line.metrics.advance <= f64::from(line.available_width) + 0.001;
    // Figma appends the RTL ending marker to the existing wrapped line, allowing
    // it to hang beyond the left edge instead of removing the final word.
    let ends: Vec<usize> = if rtl {
        vec![line.source_start + line.runs.iter().map(|r| r.text.len()).sum::<usize>()]
    } else {
        std::iter::once(line.source_start)
            .chain(
                boundaries
                    .graphemes
                    .iter()
                    .copied()
                    .filter(|&i| i > line.source_start && i <= line.paragraph_end),
            )
            .collect()
    };
    for end in ends {
        let end = line.source_start
            + source[line.source_start..end]
                .trim_end_matches(super::break_space)
                .len();
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
        ellipsis.decoration_options = DecorationOptions::default();
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
            if previous_fits || rtl {
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
    if line
        .runs
        .first()
        .is_some_and(|r| r.list_options != ListOptions::None)
    {
        line.baseline_offset +=
            line.metrics.baseline as f32 - metrics.baseline as f32 + (height - line.height) / 2.;
    }
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

#[cfg(test)]
mod list_tests {
    use super::*;
    struct Mono;
    impl Backend for Mono {
        fn measure_runs(&mut self, runs: &[TextRun]) -> Result<Metrics, Error> {
            Ok(Metrics {
                advance: runs
                    .iter()
                    .map(|r| r.text.chars().count() as f64 * 6.)
                    .sum(),
                baseline: 9.,
                ascent: 8.,
                descent: 2.,
                lines: 1,
                ..Default::default()
            })
        }
        fn raster(
            &mut self,
            _: &str,
            _: Style,
            _: f32,
            _: f32,
        ) -> Result<super::super::Mask, Error> {
            unreachable!()
        }
    }
    fn list(s: &str, level: usize) -> TextRun {
        TextRun::new(s)
            .size(10., 12.)
            .list_options(ListOptions::Ordered)
            .indentation(level)
    }
    fn marker(line: &RunLine) -> String {
        line.marker
            .as_ref()
            .unwrap()
            .runs
            .iter()
            .map(|r| r.text.as_str())
            .collect()
    }
    #[test]
    fn nested_counters_resume_and_plain_paragraphs_restart() {
        let runs = [
            list("one\n", 1),
            list("two\n", 2),
            list("three\n", 3),
            list("four\n", 4),
            list("five\n", 5),
            list("again\n", 3),
            list("parent\n", 1),
            TextRun::new("plain\n").size(10., 12.),
            list("new", 1),
        ];
        let lines = layout_runs(&mut Mono, &runs, 200., true).unwrap();
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.marker.is_some())
                .map(marker)
                .collect::<Vec<_>>(),
            ["1.", "a.", "i.", "1.", "a.", "ii.", "2.", "1."]
        );
        assert_eq!(counter(27, 2), "aa");
        assert_eq!(counter(49, 3), "xlix");
    }
    #[test]
    fn wraps_keep_one_marker_and_spacing_replaces_paragraph_gap() {
        let run = list("one two three\nnext", 1)
            .list_spacing(4.)
            .paragraph_spacing(20.);
        let lines = layout_runs(&mut Mono, std::slice::from_ref(&run), 48., true).unwrap();
        assert_eq!(lines.iter().filter(|l| l.marker.is_some()).count(), 2);
        assert!(lines[1].marker.is_none());
        assert_eq!(lines[0].indent, lines[1].indent);
        assert_eq!(lines[lines.len() - 2].spacing_after, 4.);
        assert_eq!(lines.last().unwrap().spacing_after, 0.);
        let truncated = layout_text(
            &mut Mono,
            &[run],
            [48., 100.],
            true,
            ParagraphStyle::default(),
            TextTruncation::Ending,
            Some(2),
        )
        .unwrap();
        assert_eq!(truncated.len(), 2);
        assert_eq!(marker(&truncated[0]), "1.");
        assert!(truncated[1].marker.is_none());
        assert!(truncated[1].runs.iter().any(|r| r.text.contains('…')));
    }
    #[test]
    fn counter_gutters_hanging_and_invalid_settings() {
        let source = (0..12).map(|_| "x").collect::<Vec<_>>().join("\n");
        let lines = layout_runs(&mut Mono, &[list(&source, 1)], 100., true).unwrap();
        assert!(lines.iter().all(|l| l.indent == 18.));
        assert_eq!(marker(&lines[11]), "12.");
        let hanging =
            layout_runs(&mut Mono, &[list("x", 1).hanging_list(true)], 100., true).unwrap();
        assert_eq!(hanging[0].indent, 0.);
        assert!(hanging[0].marker.as_ref().unwrap().x < 0.);
        for r in [
            list("x", 0),
            list("x", 6),
            list("x", 1).list_spacing(f32::NAN),
        ] {
            assert!(layout_runs(&mut Mono, &[r], 100., true).is_err());
        }
        assert!(layout_runs(&mut Mono, &[list("a", 1), list("b", 2)], 100., true).is_err());
        assert!(layout_runs(&mut Mono, &[list("x", 1)], 5., true).is_err());
    }
    #[test]
    fn mixed_items_keep_initial_marker_style_and_gutter() {
        let first = list("first\n", 1).color([1., 0., 0., 1.]);
        let second = list("second", 1).size(16., 20.).color([0., 1., 0., 1.]);
        let lines = layout_runs(&mut Mono, &[first.clone(), second], 200., true).unwrap();
        let marker = &lines[1].marker.as_ref().unwrap().runs[0];
        assert_eq!(marker.color, first.color);
        assert_eq!(marker.style, first.style);
        assert_eq!(lines[0].indent, lines[1].indent);
    }
    #[test]
    fn feature_validation_and_backend_opt_in_are_fail_closed() {
        for features in [
            vec![OpenTypeFeature::new(*b"LIGA", true)],
            vec![OpenTypeFeature::new(*b"vert", true)],
            vec![
                OpenTypeFeature::new(*b"liga", true),
                OpenTypeFeature::new(*b"liga", false),
            ],
        ] {
            assert!(
                validate_runs(
                    &[TextRun::new("x").open_type_features(features)],
                    100.,
                    1.,
                    false
                )
                .is_err()
            );
        }
        assert!(
            resolve_runs(
                &mut Mono,
                &[TextRun::new("x")
                    .open_type_features(vec![OpenTypeFeature::new(*b"liga", false)])]
            )
            .is_err()
        );
        assert!(
            resolve_runs(
                &mut Mono,
                &[TextRun::new("x").text_case(TextCase::SmallCaps)]
            )
            .is_err()
        );
    }
}
