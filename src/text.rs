//! Explicit-font monochrome text with script-aware shaping and bounded Unicode
//! segmentation. Styled runs retain fonts, colors and paragraph direction. CoreText
//! fallback uses only host-supplied slots; no system font discovery or emoji.
//! Fonts and their variable axes are caller-supplied; nothing here names a font.
use crate::Error;

mod rich;
mod unicode;
#[cfg(feature = "wgpu")]
pub(crate) use rich::validate_truncation;
pub use rich::{
    DecorationOptions, DecorationValue, ListMarker, ListOptions, OpenTypeFeature, ParagraphStyle,
    RunLine, RunMask, TextCase, TextDecoration, TextDecorationStyle, TextRun, TextTruncation,
    layout_paragraphs, layout_runs, layout_text, resolve_runs, resolve_style, validate_runs,
};
pub(crate) use unicode::break_space;
pub use unicode::{TextBoundaries, TextDirection};

pub const MAX_FONTS: usize = 64;
pub const MAX_AXES: usize = 8;
pub const MAX_TEXT_BYTES: usize = 512;
/// Widest mask, in logical units. Longer lines are cut there.
pub const MAX_WIDTH: f32 = 2048.;

/// A variable font axis, such as `wght` 600.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    pub tag: [u8; 4],
    pub value: f32,
}

/// The characters the single-line Latin scope accepts: printable Basic Latin
/// through Latin Extended-B, general punctuation, currency, letterlike symbols,
/// arrows and mathematical operators. No controls, line breaks, bidi or emoji.
pub(crate) fn latin_char(c: char) -> bool {
    matches!(
        c as u32,
        0x20..=0x7E
            | 0xA0..=0xAC
            | 0xAE..=0x24F
            | 0x2010..=0x2027
            | 0x2030..=0x205E
            | 0x20A0..=0x20BF
            | 0x2100..=0x22FF
    )
}

/// Monochrome BMP text: Latin, Greek/Cyrillic, Hebrew/Arabic, Indic, Southeast
/// Asian and CJK scripts. Emoji, variation selectors and bidi controls fail closed.
pub fn supported_char(c: char) -> bool {
    latin_char(c)
        || matches!(c as u32,
        0x0300..=0x1fff | 0x2000..=0x200d | 0x202f | 0x2060 |
        0x2c00..=0xd7ff | 0xf900..=0xfdcf | 0xfdf0..=0xffef)
            && !matches!(c as u32, 0x061c | 0x180e | 0xfe00..=0xfe0f | 0xfeff)
            && !c.is_control()
}

#[cfg(any(feature = "text-coretext", feature = "wgpu"))]
pub(crate) fn is_rtl_char(c: char) -> bool {
    matches!(c as u32,0x0590..=0x08ff|0xfb1d..=0xfdff|0xfe70..=0xfefc)
}

pub fn validate_characters(text: &str) -> Result<(), Error> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(Error("text exceeds 512 bytes"));
    }
    if !text.chars().all(supported_char) {
        return Err(Error(
            "text has characters outside the supported single-line Unicode scope",
        ));
    }
    Ok(())
}

/// Font bytes and the axis values to instantiate them with. An axis left out
/// stays with the font: optical size then follows the logical font size.
#[derive(Clone, Copy)]
pub struct FontSource<'a> {
    pub bytes: &'a [u8],
    pub axes: &'a [Axis],
}
#[cfg(feature = "text-coretext")]
fn check_source(f: &FontSource) -> Result<(), Error> {
    if f.bytes.is_empty() || f.bytes.len() > 32 * 1024 * 1024 {
        return Err(Error("font byte budget exceeded"));
    }
    if f.axes.len() > MAX_AXES {
        return Err(Error("too many font axes"));
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub font: usize,
    pub size: f32,
    /// Explicit logical height; zero selects the supplied font’s natural height.
    pub line_height: f32,
    /// Extra space after each glyph, in logical units: Figma's letter spacing
    /// in pixels. The space after the last glyph is not counted in the advance.
    pub letter_spacing: f32,
}
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Metrics {
    pub advance: f64,
    pub ascent: f64,
    pub descent: f64,
    pub baseline: f64,
    pub glyphs: u32,
    pub lines: u32,
}
pub struct Mask {
    pub width: u32,
    pub height: u32,
    pub alpha: Vec<u8>,
    pub metrics: Metrics,
}
/// Shaped, line-local glyph contours grouped by input run, including decorations.
/// Coordinates are logical units, x right and y down from the line top.
pub struct RunOutlines {
    pub paths: Vec<crate::VectorPath>,
    /// Separate decoration contours, one per run, painted behind glyphs.
    pub decoration_paths: Vec<crate::VectorPath>,
    pub metrics: Metrics,
}
pub trait Backend {
    fn validate_decoration(&mut self, options: DecorationOptions) -> Result<(), Error> {
        if options != DecorationOptions::default() {
            return Err(Error(
                "custom text decorations are unsupported by this backend",
            ));
        }
        Ok(())
    }
    /// Backends must explicitly support overrides; never silently ignore them.
    fn validate_open_type_features(
        &mut self,
        _style: Style,
        features: &[OpenTypeFeature],
    ) -> Result<(), Error> {
        if features.is_empty() {
            Ok(())
        } else {
            Err(Error("backend does not support OpenType overrides"))
        }
    }

    /// One nonzero path per input run, preserving its holes. Call `resolve_runs`
    /// first for case, direction, font fallback and natural heights.
    fn outline_runs(&mut self, _runs: &[TextRun]) -> Result<RunOutlines, Error> {
        Err(Error("backend does not support text outlines"))
    }

    /// Return grapheme boundaries and legal Unicode line-break opportunities.
    fn text_boundaries(&mut self, text: &str) -> Result<TextBoundaries, Error> {
        TextBoundaries::latin(text)
    }
    /// Resolve explicit font fallback before line-height resolution and layout.
    fn resolve_fonts(&mut self, runs: &[TextRun]) -> Result<Vec<TextRun>, Error> {
        Ok(runs.to_vec())
    }
    fn natural_line_height(&mut self, _style: Style) -> Result<f32, Error> {
        Err(Error("backend does not support automatic line height"))
    }
    /// Shape a complete attributed line; unsupported backends fail closed.
    /// Implementations must resolve case before measuring (see `resolve_runs`)
    /// and render each run's decoration, or return an error.
    fn measure_runs(&mut self, _runs: &[TextRun]) -> Result<Metrics, Error> {
        Err(Error("backend does not support styled text"))
    }
    fn raster_runs(
        &mut self,
        _runs: &[TextRun],
        _width: f32,
        _scale: f32,
    ) -> Result<RunMask, Error> {
        Err(Error("backend does not support styled text"))
    }
    fn raster(&mut self, text: &str, style: Style, width: f32, scale: f32) -> Result<Mask, Error>;
    /// Shapes without rasterizing. The default rasterizes at the widest mask.
    fn measure(&mut self, text: &str, style: Style) -> Result<Metrics, Error> {
        Ok(self.raster(text, style, 2048., 1.)?.metrics)
    }
}
/// Validate a resolved style; call `resolve_style` first for automatic height.
pub fn validate(text: &str, s: Style, width: f32, scale: f32) -> Result<[u32; 2], Error> {
    if text.is_empty()
        || validate_characters(text).is_err()
        || s.font >= MAX_FONTS
        || ![s.size, s.line_height, width, scale]
            .iter()
            .all(|x| x.is_finite() && *x > 0.)
        || !s.letter_spacing.is_finite()
        || s.letter_spacing.abs() > s.size
        || s.size > 128.
        || s.line_height > 256.
        || s.line_height < s.size
        || width > MAX_WIDTH
        || !(0.5..=4.).contains(&scale)
    {
        return Err(Error("unsupported single-line text request"));
    }
    Ok([
        (width * scale).ceil() as u32,
        (s.line_height * scale).ceil() as u32,
    ])
}
#[cfg(feature = "text-coretext")]
pub mod coretext {
    use super::*;
    use std::{ffi::c_void, ptr::NonNull};
    #[repr(C)]
    struct StyledRun {
        font: *mut c_void,
        text: *const u8,
        len: usize,
        height: f64,
        tracking: f64,
        color: [f64; 4],
        decoration: u32,
        decoration_style: u32,
        decoration_offset: f64,
        decoration_thickness: f64,
        custom_decoration: u32,
        skip_ink: u32,
        direction: i32,
        kerning: i32,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct OutlineCommand {
        run: u32,
        kind: u32,
        points: [f64; 6],
    }
    unsafe extern "C" {
        fn figleaf_ct_boundaries(
            text: *const u8,
            len: usize,
            graphemes: *mut u32,
            lines: *mut u32,
            counts: *mut u32,
        ) -> i32;
        fn figleaf_ct_covers(font: *mut c_void, text: *const u8, len: usize) -> bool;
        fn figleaf_ct_direction(text: *const u8, len: usize) -> i32;
        fn figleaf_ct_height(font: *mut c_void) -> f64;
        fn figleaf_ct_runs(
            runs: *const StyledRun,
            count: usize,
            scale: f64,
            pixels: *mut u8,
            width: u32,
            rows: u32,
            metrics: *mut Metrics,
            commands: *mut OutlineCommand,
            command_count: *mut usize,
        ) -> i32;
        fn figleaf_ct_font(bytes: *const u8, len: usize) -> *mut c_void;
        fn figleaf_ct_drop(font: *mut c_void);
        fn figleaf_ct_instance(
            base: *mut c_void,
            size: f64,
            tags: *const u32,
            values: *const f64,
            count: usize,
            feature_tags: *const u32,
            feature_values: *const i32,
            feature_count: usize,
        ) -> *mut c_void;
        fn figleaf_ct_instance_drop(font: *mut c_void);
        fn figleaf_ct_run(
            font: *mut c_void,
            text: *const u8,
            len: usize,
            height: f64,
            tracking: f64,
            scale: f64,
            pixels: *mut u8,
            width: u32,
            rows: u32,
            metrics: *mut Metrics,
        ) -> i32;
    }
    struct Font(NonNull<c_void>);
    impl Drop for Font {
        fn drop(&mut self) {
            unsafe { figleaf_ct_drop(self.0.as_ptr()) }
        }
    }
    struct Instance(NonNull<c_void>);
    impl Drop for Instance {
        fn drop(&mut self) {
            unsafe { figleaf_ct_instance_drop(self.0.as_ptr()) }
        }
    }
    /// A font with its axis tags and values, in matching order.
    type Loaded = (Font, Vec<u32>, Vec<f64>);
    /// Caller-supplied fonts, indexed by `Style::font`. A `None` slot is a font
    /// the host could not supply: text using it fails with a diagnostic. Optional
    /// fallback chains cover missing glyphs, never missing primary fonts.
    pub struct CoreText {
        fonts: Vec<Option<Loaded>>,
        instances: Vec<(usize, u32, Vec<OpenTypeFeature>, Instance)>,
        fallbacks: Vec<usize>,
        font_fallbacks: Vec<(usize, Vec<usize>)>,
    }
    impl CoreText {
        pub fn new(sources: &[Option<FontSource>]) -> Result<Self, Error> {
            if sources.len() > MAX_FONTS {
                return Err(Error("too many fonts"));
            }
            let mut fonts = Vec::new();
            for source in sources {
                fonts.push(match source {
                    None => None,
                    Some(f) => {
                        check_source(f)?;
                        let font = NonNull::new(unsafe {
                            figleaf_ct_font(f.bytes.as_ptr(), f.bytes.len())
                        })
                        .map(Font)
                        .ok_or(Error("invalid CoreText font bytes"))?;
                        Some((
                            font,
                            f.axes.iter().map(|a| u32::from_be_bytes(a.tag)).collect(),
                            f.axes.iter().map(|a| f64::from(a.value)).collect(),
                        ))
                    }
                });
            }
            Ok(Self {
                fonts,
                instances: Vec::new(),
                fallbacks: Vec::new(),
                font_fallbacks: Vec::new(),
            })
        }
        /// Ordered fallback slots supplied by the host. Missing primary fonts are
        /// still errors; only missing glyphs select a fallback. No system discovery.
        pub fn with_fallback_fonts(mut self, slots: &[usize]) -> Result<Self, Error> {
            if slots.len() > MAX_FONTS
                || slots
                    .iter()
                    .any(|&i| self.fonts.get(i).is_none_or(Option::is_none))
            {
                return Err(Error("invalid explicit fallback font slots"));
            }
            self.fallbacks = slots.to_vec();
            Ok(self)
        }
        /// Override the fallback order for one primary slot (for example, use a
        /// bold fallback for a bold primary instead of guessing a matching face).
        pub fn with_fallback_for(mut self, primary: usize, slots: &[usize]) -> Result<Self, Error> {
            if self.fonts.get(primary).is_none_or(Option::is_none)
                || slots.len() > MAX_FONTS
                || slots
                    .iter()
                    .any(|&i| self.fonts.get(i).is_none_or(Option::is_none))
            {
                return Err(Error("invalid explicit fallback font slots"));
            }
            if let Some((_, chain)) = self
                .font_fallbacks
                .iter_mut()
                .find(|(font, _)| *font == primary)
            {
                *chain = slots.to_vec();
            } else {
                self.font_fallbacks.push((primary, slots.to_vec()));
            }
            Ok(self)
        }
        fn fallback_slots(&self, primary: usize) -> &[usize] {
            self.font_fallbacks
                .iter()
                .find(|(font, _)| *font == primary)
                .map_or(self.fallbacks.as_slice(), |(_, chain)| chain.as_slice())
        }
        fn instance(&mut self, style: Style) -> Result<usize, Error> {
            self.featured_instance(style, &[])
        }
        fn featured_instance(
            &mut self,
            style: Style,
            features: &[OpenTypeFeature],
        ) -> Result<usize, Error> {
            rich::validate_features(features)?;
            if let Some(i) = self.instances.iter().position(|(f, s, fs, _)| {
                *f == style.font && *s == style.size.to_bits() && fs == features
            }) {
                return Ok(i);
            }
            let (font, tags, values) = self
                .fonts
                .get(style.font)
                .ok_or(Error("font index outside the supplied fonts"))?
                .as_ref()
                .ok_or(Error("font was not supplied"))?;
            if self.instances.len() >= 64 {
                return Err(Error("CoreText font-instance budget exceeded"));
            }
            let feature_tags: Vec<_> = features.iter().map(|f| u32::from_be_bytes(f.tag)).collect();
            let feature_values: Vec<_> = features.iter().map(|f| i32::from(f.enabled)).collect();
            // Tags and values stay alive for this synchronous call and have equal length.
            let ptr = NonNull::new(unsafe {
                figleaf_ct_instance(
                    font.0.as_ptr(),
                    style.size as f64,
                    tags.as_ptr(),
                    values.as_ptr(),
                    tags.len(),
                    feature_tags.as_ptr(),
                    feature_values.as_ptr(),
                    features.len(),
                )
            })
            .ok_or(Error(
                "CoreText font or requested OpenType feature unavailable",
            ))?;
            self.instances.push((
                style.font,
                style.size.to_bits(),
                features.to_vec(),
                Instance(ptr),
            ));
            Ok(self.instances.len() - 1)
        }
        fn styled(
            &mut self,
            runs: &[TextRun],
            scale: f32,
            pixels: Option<(&mut [u8], u32, u32)>,
            mut outlines: Option<&mut Vec<OutlineCommand>>,
        ) -> Result<Metrics, Error> {
            let mut input = Vec::new();
            for run in runs {
                if run.style.letter_spacing != 0.
                    && run
                        .open_type_features
                        .iter()
                        .any(|f| f.tag == *b"kern" && f.enabled)
                {
                    return Err(Error(
                        "enabled kerning with nonzero tracking is unsupported",
                    ));
                }
                let i = self.featured_instance(run.style, &run.open_type_features)?;
                input.push(StyledRun {
                    font: self.instances[i].3.0.as_ptr(),
                    text: run.text.as_ptr(),
                    len: run.text.len(),
                    height: run.style.line_height as f64,
                    tracking: run.style.letter_spacing as f64,
                    color: run.color.map(f64::from),
                    decoration: run.text_decoration as u32,
                    decoration_style: run.decoration_options.style as u32,
                    decoration_offset: run
                        .decoration_options
                        .offset
                        .resolve(run.style.size)
                        .map_or(f64::NAN, f64::from),
                    decoration_thickness: run
                        .decoration_options
                        .thickness
                        .resolve(run.style.size)
                        .map_or(f64::NAN, f64::from),
                    custom_decoration: u32::from(
                        run.decoration_options != DecorationOptions::default()
                            || run.text_decoration == TextDecoration::Underline
                                && run.text.chars().any(is_rtl_char),
                    ),
                    skip_ink: u32::from(run.decoration_options.skip_ink),
                    direction: run.direction as i32,
                    kerning: run
                        .open_type_features
                        .iter()
                        .find(|f| f.tag == *b"kern")
                        .map_or(-1, |f| i32::from(f.enabled)),
                });
            }
            let (ptr, w, h) = pixels.map_or((std::ptr::null_mut(), 0, 0), |(p, w, h)| {
                (p.as_mut_ptr(), w, h)
            });
            let mut command_count = 0;
            let commands = outlines
                .as_mut()
                .map_or(std::ptr::null_mut(), |v| v.as_mut_ptr());
            let mut metrics = Metrics::default();
            // Input strings and retained instances live across this synchronous call;
            // Pixel output is width*rows*4 bytes; optional outline output has 65,536
            // initialized commands, matching the native cap. All FFI structs are repr(C).
            let status = unsafe {
                figleaf_ct_runs(
                    input.as_ptr(),
                    input.len(),
                    scale as f64,
                    ptr,
                    w,
                    h,
                    &mut metrics,
                    commands,
                    &mut command_count,
                )
            };
            if let Some(outlines) = outlines {
                outlines.truncate(command_count);
            }
            match status {
                0 => Ok(metrics),
                3 => Err(Error("font has no glyph for a character")),
                _ => Err(Error("CoreText rejected styled text")),
            }
        }
        fn run(
            &mut self,
            text: &str,
            style: Style,
            scale: f32,
            mut pixels: Option<(&mut [u8], u32, u32)>,
        ) -> Result<Metrics, Error> {
            let index = self.instance(style)?;
            let mut metrics = Metrics::default();
            let (ptr, w, h) = match &mut pixels {
                Some((p, w, h)) => (p.as_mut_ptr(), *w, *h),
                None => (std::ptr::null_mut(), 0, 0),
            };
            // Fonts and strings remain alive for this synchronous C call; output spans
            // exactly w*h bytes and the repr(C) metrics struct matches the shim.
            let result = unsafe {
                figleaf_ct_run(
                    self.instances[index].3.0.as_ptr(),
                    text.as_ptr(),
                    text.len(),
                    style.line_height as f64,
                    style.letter_spacing as f64,
                    scale as f64,
                    ptr,
                    w,
                    h,
                    &mut metrics,
                )
            };
            match result {
                0 => Ok(metrics),
                3 => Err(Error("font has no glyph for a character")),
                _ => Err(Error("CoreText rejected the text or bitmap context")),
            }
        }
    }
    impl Backend for CoreText {
        fn validate_decoration(&mut self, _options: DecorationOptions) -> Result<(), Error> {
            Ok(())
        }
        fn validate_open_type_features(
            &mut self,
            style: Style,
            features: &[OpenTypeFeature],
        ) -> Result<(), Error> {
            if style.letter_spacing != 0. && features.iter().any(|f| f.tag == *b"kern" && f.enabled)
            {
                return Err(Error(
                    "enabled kerning with nonzero tracking is unsupported",
                ));
            }
            self.featured_instance(style, features).map(|_| ())
        }

        fn outline_runs(&mut self, runs: &[TextRun]) -> Result<RunOutlines, Error> {
            validate_runs(runs, MAX_WIDTH, 1., false)?;
            if runs.iter().any(|r| {
                r.text_case != TextCase::Original
                    || r.direction == TextDirection::Auto
                    || r.style.line_height == 0.
            }) {
                return Err(Error("text outlines require resolved runs"));
            }
            // Layout has already resolved case, fallback and natural heights.
            let mut commands = vec![OutlineCommand::default(); 65536];
            let metrics = self.styled(runs, 1., None, Some(&mut commands))?;
            let mut paths =
                vec![crate::VectorPath::new(crate::WindingRule::NonZero); runs.len() * 2];
            for c in commands {
                let path = paths
                    .get_mut(c.run as usize)
                    .ok_or(Error("invalid outline run"))?;
                let p = |i: usize| [c.points[i] as f32, c.points[i + 1] as f32];
                use crate::path::Command::*;
                path.commands.push(match c.kind {
                    0 => MoveTo(p(0)),
                    1 => LineTo(p(0)),
                    2 => QuadraticTo(p(0), p(2)),
                    3 => CubicTo(p(0), p(2), p(4)),
                    4 => Close,
                    _ => return Err(Error("invalid glyph outline")),
                });
            }
            let decoration_paths = paths.split_off(runs.len());
            Ok(RunOutlines {
                paths,
                decoration_paths,
                metrics,
            })
        }

        fn text_boundaries(&mut self, text: &str) -> Result<TextBoundaries, Error> {
            if text.len() > MAX_TEXT_BYTES || text.is_empty() {
                return Err(Error("invalid segmentation input"));
            }
            let mut graphemes = [0u32; MAX_TEXT_BYTES];
            let mut lines = [0u32; MAX_TEXT_BYTES];
            let mut counts = [0u32; 2];
            let status = unsafe {
                figleaf_ct_boundaries(
                    text.as_ptr(),
                    text.len(),
                    graphemes.as_mut_ptr(),
                    lines.as_mut_ptr(),
                    counts.as_mut_ptr(),
                )
            };
            if status != 0 || counts.iter().any(|&n| n as usize > MAX_TEXT_BYTES) {
                return Err(Error("Unicode segmentation failed"));
            }
            let result = TextBoundaries {
                graphemes: graphemes[..counts[0] as usize]
                    .iter()
                    .map(|&n| n as usize)
                    .collect(),
                lines: lines[..counts[1] as usize]
                    .iter()
                    .map(|&n| n as usize)
                    .collect(),
            };
            result.validate(text)?;
            Ok(result)
        }
        fn resolve_fonts(&mut self, runs: &[TextRun]) -> Result<Vec<TextRun>, Error> {
            let source: String = runs.iter().map(|r| r.text.as_str()).collect();
            if source.is_empty() {
                return Ok(runs.to_vec());
            }
            let boundaries = self.text_boundaries(&source)?;
            let mut result: Vec<TextRun> = Vec::new();
            let mut start = 0;
            for end in boundaries.graphemes {
                let cluster = &source[start..end];
                let mut parts = rich::slice(runs, start, end);
                let mut checked = parts[0].style;
                if checked.line_height == 0. {
                    checked.line_height = checked.size;
                }
                validate("M", checked, 1., 1.)?;
                let primary = parts[0].style.font;
                if parts.iter().any(|r| r.style.font != primary) {
                    return Err(Error("font changes inside a grapheme are unsupported"));
                }
                // Keep a joining-script word in one font if its primary font
                // lacks any character. Per-letter fallback would break joining.
                let mut probe = cluster;
                if cluster.chars().any(is_rtl_char) {
                    let lo = source[..start]
                        .char_indices()
                        .rev()
                        .find(|(_, c)| !is_rtl_char(*c) && *c != '\u{200c}' && *c != '\u{200d}')
                        .map_or(0, |(i, c)| i + c.len_utf8());
                    let hi = source[end..]
                        .char_indices()
                        .find(|(_, c)| !is_rtl_char(*c) && *c != '\u{200c}' && *c != '\u{200d}')
                        .map_or(source.len(), |(i, _)| end + i);
                    if rich::slice(runs, lo, hi)
                        .iter()
                        .all(|r| r.style.font == primary)
                    {
                        probe = &source[lo..hi];
                    }
                }
                let primary_instance = self.instance(parts[0].style)?;
                let mut chosen = None;
                if cluster == "\n"
                    || unsafe {
                        figleaf_ct_covers(
                            self.instances[primary_instance].3.0.as_ptr(),
                            probe.as_ptr(),
                            probe.len(),
                        )
                    }
                {
                    chosen = Some(primary);
                }
                if chosen.is_none() {
                    for slot in self.fallback_slots(primary).to_vec() {
                        let mut style = parts[0].style;
                        style.font = slot;
                        let index = self.instance(style)?;
                        if unsafe {
                            figleaf_ct_covers(
                                self.instances[index].3.0.as_ptr(),
                                probe.as_ptr(),
                                probe.len(),
                            )
                        } {
                            chosen = Some(slot);
                            break;
                        }
                    }
                }
                let font = chosen.ok_or(Error("no supplied font covers a complete grapheme"))?;
                let paragraph_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
                let paragraph_end = source[start..]
                    .find('\n')
                    .map_or(source.len(), |i| start + i);
                let paragraph = &source[paragraph_start..paragraph_end];
                let rtl = unsafe { figleaf_ct_direction(paragraph.as_ptr(), paragraph.len()) } == 1;
                for part in &mut parts {
                    part.style.font = font;
                    if part.direction == TextDirection::Auto {
                        part.direction = if rtl {
                            TextDirection::RightToLeft
                        } else {
                            TextDirection::LeftToRight
                        };
                    }
                    let merged = result.last_mut().is_some_and(|last| {
                        let mut check = last.clone();
                        check.text.clone_from(&part.text);
                        if check == *part {
                            last.text.push_str(&part.text);
                            true
                        } else {
                            false
                        }
                    });
                    if !merged {
                        result.push(part.clone());
                    }
                }
                start = end;
            }
            if result.len() > 128 {
                return Err(Error("fallback exceeds styled run limit"));
            }
            Ok(result)
        }

        fn natural_line_height(&mut self, style: Style) -> Result<f32, Error> {
            let mut checked = style;
            checked.line_height = checked.size;
            validate("M", checked, 1., 1.)?;
            let index = self.instance(style)?;
            Ok(unsafe { figleaf_ct_height(self.instances[index].3.0.as_ptr()) } as f32)
        }
        fn measure_runs(&mut self, runs: &[TextRun]) -> Result<Metrics, Error> {
            validate_runs(runs, 1., 1., false)?;
            let resolved = resolve_runs(self, runs)?;
            self.styled(&resolved, 1., None, None)
        }
        fn raster_runs(
            &mut self,
            runs: &[TextRun],
            width: f32,
            scale: f32,
        ) -> Result<RunMask, Error> {
            validate_runs(runs, width, scale, false)?;
            if runs.iter().any(|r| r.paints.is_some()) {
                return Err(Error("painted text requires outline rendering"));
            }
            let resolved = resolve_runs(self, runs)?;
            let runs = resolved.as_slice();
            let height = runs
                .iter()
                .map(|r| r.style.line_height)
                .reduce(f32::max)
                .unwrap();
            let w = (width * scale).ceil() as u32;
            let h = (height * scale).ceil() as u32;
            let mut rgba = vec![0; w as usize * h as usize * 4];
            let metrics = self.styled(runs, scale, Some((&mut rgba, w, h)), None)?;
            for pixel in rgba.as_chunks_mut::<4>().0 {
                let a = u32::from(pixel[3]);
                for value in &mut pixel[..3] {
                    *value = (u32::from(*value) * 255 + a / 2)
                        .checked_div(a)
                        .unwrap_or(0)
                        .min(255) as u8;
                }
            }
            Ok(RunMask {
                width: w,
                height: h,
                rgba,
                metrics,
            })
        }

        fn raster(
            &mut self,
            text: &str,
            style: Style,
            width: f32,
            scale: f32,
        ) -> Result<Mask, Error> {
            if !self.fallback_slots(style.font).is_empty() || !text.chars().all(latin_char) {
                let mut run = TextRun::new(text);
                run.style = style;
                run.color = [1.; 4];
                let mask = self.raster_runs(&[run], width, scale)?;
                return Ok(Mask {
                    width: mask.width,
                    height: mask.height,
                    alpha: mask.rgba.as_chunks::<4>().0.iter().map(|p| p[3]).collect(),
                    metrics: mask.metrics,
                });
            }
            let style = resolve_style(self, style)?;
            let [w, h] = validate(text, style, width, scale)?;
            let mut alpha = vec![0; w as usize * h as usize];
            let metrics = self.run(text, style, scale, Some((&mut alpha, w, h)))?;
            Ok(Mask {
                width: w,
                height: h,
                alpha,
                metrics,
            })
        }
        fn measure(&mut self, text: &str, style: Style) -> Result<Metrics, Error> {
            if !self.fallback_slots(style.font).is_empty() || !text.chars().all(latin_char) {
                let mut run = TextRun::new(text);
                run.style = style;
                return self.measure_runs(&[run]);
            }
            let style = resolve_style(self, style)?;
            validate(text, style, 1., 1.)?;
            self.run(text, style, 1., None)
        }
    }
}

/// One line of a wrapped paragraph and its width.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub text: String,
    pub advance: f32,
}

/// Breaks `text` into lines no wider than `width`, as Figma's fixed-width text
/// box does: at backend-provided Unicode opportunities and every `\n`. Breakable
/// spaces are trimmed. Emergency breaks preserve complete grapheme clusters.
///
/// A host that lays text out calls this to learn the height:
/// `lines.len() as f32 * style.line_height`.
pub fn wrap(
    backend: &mut dyn Backend,
    text: &str,
    style: Style,
    width: f32,
) -> Result<Vec<Line>, Error> {
    if !text.chars().all(|c| latin_char(c) || c == '\n') {
        let mut run = TextRun::new(text);
        run.style = style;
        return layout_runs(backend, &[run], width, true).map(|lines| {
            lines
                .into_iter()
                .map(|l| Line {
                    text: l.runs.iter().map(|r| r.text.as_str()).collect(),
                    advance: l.metrics.advance as f32,
                })
                .collect()
        });
    }
    if !width.is_finite() || width <= 0. {
        return Err(Error("unsupported text request"));
    }
    validate_characters(&text.replace('\n', " "))?;
    if text.is_empty() {
        return Ok(vec![Line {
            text: String::new(),
            advance: 0.,
        }]);
    }
    let boundaries = backend.text_boundaries(text)?;
    boundaries.validate(text)?;
    let style = resolve_style(backend, style)?;
    let mut advance = |s: &str| -> Result<f32, Error> {
        if s.is_empty() {
            return Ok(0.);
        }
        Ok(backend.measure(s, style)?.advance as f32)
    };
    let mut lines = Vec::new();
    let mut paragraph_start = 0;
    for paragraph in text.split('\n') {
        let mut rest = paragraph;
        loop {
            // The longest run of whole words that fits.
            let mut fit: Option<(usize, f32)> = None;
            let offset = paragraph_start + paragraph.len() - rest.len();
            let mut ends: Vec<usize> = boundaries
                .lines
                .iter()
                .copied()
                .filter(|&i| i > offset && i < offset + rest.len())
                .map(|i| i - offset)
                .collect();
            ends.push(rest.len());
            for end in ends {
                let wide = advance(rest[..end].trim_end_matches(break_space))?;
                if wide > width + 0.001 {
                    break;
                }
                fit = Some((end, wide));
            }
            let (end, wide) = match fit {
                Some(fit) => fit,
                // Not even the first word fits: keep the characters that do,
                // and at least one.
                None => {
                    let mut clusters = boundaries
                        .graphemes
                        .iter()
                        .copied()
                        .filter(|&i| i > offset && i <= offset + rest.len())
                        .map(|i| i - offset);
                    let mut cut = clusters.next().unwrap_or(0);
                    let mut wide = advance(&rest[..cut])?;
                    for end in clusters {
                        let next = advance(&rest[..end])?;
                        if next > width + 0.001 {
                            break;
                        }
                        (cut, wide) = (end, next);
                    }
                    (cut, wide)
                }
            };
            lines.push(Line {
                text: rest[..end].trim_end_matches(break_space).to_string(),
                advance: wide,
            });
            rest = rest[end..].trim_start_matches(break_space);
            if rest.is_empty() {
                break;
            }
        }
        paragraph_start += paragraph.len() + 1;
    }
    Ok(lines)
}

/// Where a caret can stand in one line of `text`: the distance from the start
/// of the line to before each character, and then to the end, so there is one
/// more offset than there are characters. A host's text field draws its caret
/// at `offsets[n]` and a selection between two of them.
///
/// Each offset is the measured width of the text before it, so kerning and
/// letter spacing are as drawn.
pub fn caret_offsets(
    backend: &mut dyn Backend,
    text: &str,
    style: Style,
) -> Result<Vec<f32>, Error> {
    if !text.chars().all(latin_char) {
        return Err(Error("scalar caret offsets support Latin text only"));
    }
    let style = resolve_style(backend, style)?;
    let mut offsets = vec![0.];
    for (i, c) in text.char_indices() {
        let end = i + c.len_utf8();
        offsets.push(backend.measure(&text[..end], style)?.advance as f32);
    }
    Ok(offsets)
}

/// A backend for hosts that draw no text: every request fails, so text items
/// show up in the renderer's problems instead of vanishing quietly.
pub struct NoText;
impl Backend for NoText {
    fn raster(&mut self, _: &str, _: Style, _: f32, _: f32) -> Result<Mask, Error> {
        Err(Error("no text backend"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Every character is 10 wide.
    struct Mono;
    impl Backend for Mono {
        fn raster(&mut self, _: &str, _: Style, _: f32, _: f32) -> Result<Mask, Error> {
            Err(Error("measure only"))
        }
        fn measure(&mut self, text: &str, _: Style) -> Result<Metrics, Error> {
            Ok(Metrics {
                advance: text.chars().count() as f64 * 10.,
                ..Default::default()
            })
        }
    }

    #[test]
    fn paragraphs_break_at_spaces_line_breaks_and_inside_long_words() {
        let s = Style {
            font: 0,
            size: 12.,
            line_height: 16.,
            letter_spacing: 0.,
        };
        let lines = |text: &str, width: f32| -> Vec<(String, f32)> {
            wrap(&mut Mono, text, s, width)
                .unwrap()
                .into_iter()
                .map(|l| (l.text, l.advance))
                .collect()
        };
        let l = |t: &str| (t.to_string(), t.len() as f32 * 10.);
        assert_eq!(lines("one two three", 70.), [l("one two"), l("three")]);
        // The space at the break hangs: "one two" fits 70 exactly.
        assert_eq!(lines("one two  three", 75.), [l("one two"), l("three")]);
        assert_eq!(lines("one\n\ntwo", 500.), [l("one"), l(""), l("two")]);
        assert_eq!(
            lines("a abcdefgh b", 30.),
            [l("a"), l("abc"), l("def"), l("gh"), l("b")]
        );
        assert_eq!(lines("wide", 5.), [l("w"), l("i"), l("d"), l("e")]);
        assert!(wrap(&mut Mono, "a", s, 0.).is_err());
    }

    #[test]
    fn carets_stand_before_each_character_and_at_the_end() {
        let s = Style {
            font: 0,
            size: 12.,
            line_height: 16.,
            letter_spacing: 0.,
        };
        assert_eq!(caret_offsets(&mut Mono, "", s).unwrap(), [0.]);
        assert_eq!(
            caret_offsets(&mut Mono, "a é", s).unwrap(),
            [0., 10., 20., 30.]
        );
        assert!(caret_offsets(&mut NoText, "a", s).is_err());
    }

    #[test]
    fn rejects_unimplemented_text_before_raster() {
        let s = Style {
            font: 0,
            size: 12.,
            line_height: 16.,
            letter_spacing: 0.,
        };
        assert!(validate("abc", s, 100., 2.).is_ok());
        for t in ["", "a\nb", "🙂", "a\u{202e}b"] {
            assert!(validate(t, s, 100., 1.).is_err());
        }
        assert!(validate("abc", s, f32::NAN, 1.).is_err());
        assert!(validate("abc", s, 100., 8.).is_err());
    }
}

#[cfg(all(test, feature = "text-coretext"))]
mod backend_tests {
    use super::*;
    fn exercise(b: &mut dyn Backend) {
        let s = Style {
            font: 1,
            size: 10.,
            line_height: 18.,
            letter_spacing: 0.,
        };
        let a = b.raster("source3", s, 100., 2.).unwrap();
        let changed = b.raster("source4", s, 100., 2.).unwrap();
        assert_eq!([a.width, a.height], [200, 36]);
        assert_eq!(a.metrics.lines, 1);
        assert!((a.metrics.advance - 42.).abs() < 0.01);
        assert!(a.alpha.iter().any(|a| *a > 0));
        assert_ne!(a.alpha, changed.alpha);
        assert!(b.raster("🙂", s, 100., 1.).is_err());
        // Letter spacing goes between glyphs, not after the last one.
        let tracked = Style {
            letter_spacing: 1.5,
            ..s
        };
        let wide = b.measure("source3", tracked).unwrap().advance;
        assert!((wide - (42. + 6. * 1.5)).abs() < 0.01, "{wide}");
        assert_ne!(
            b.raster("source3", tracked, 100., 2.).unwrap().alpha,
            a.alpha
        );
        // A caret after a space stands past the space.
        let carets = caret_offsets(b, "so 3", s).unwrap();
        assert_eq!(carets.len(), 5);
        assert!(carets.windows(2).all(|w| w[1] > w[0]), "{carets:?}");
        assert!((carets[4] - 24.).abs() < 0.01, "{carets:?}");
    }
    const FONT: &[u8] = include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf");
    fn font(axes: &[Axis]) -> Option<FontSource<'_>> {
        Some(FontSource { bytes: FONT, axes })
    }
    fn bad() -> Option<FontSource<'static>> {
        Some(FontSource {
            bytes: b"bad",
            axes: &[],
        })
    }
    /// Slot 0 of `b` was not supplied. Caller-supplied weight changes the glyphs.
    fn missing_and_axes(b: &mut dyn Backend, make: impl Fn(&[Axis]) -> Box<dyn Backend>) {
        let s = Style {
            font: 0,
            size: 10.,
            line_height: 18.,
            letter_spacing: 0.,
        };
        assert!(b.raster("abc", s, 100., 1.).is_err());
        assert!(b.measure("abc", s).is_err());
        let bold = [Axis {
            tag: *b"wght",
            value: 800.,
        }];
        let regular = make(&[]).raster("source", s, 100., 2.).unwrap();
        let heavy = make(&bold).raster("source", s, 100., 2.).unwrap();
        assert_ne!(regular.alpha, heavy.alpha);
        let measured = make(&[]).measure("source", s).unwrap();
        assert!((measured.advance - regular.metrics.advance).abs() < 0.01);
    }
    #[test]
    fn coretext_shapes_pinned_bytes_and_releases_repeated_runs() {
        let mut b = coretext::CoreText::new(&[None, font(&[])]).unwrap();
        exercise(&mut b);
        for _ in 0..16 {
            exercise(&mut b)
        }
        missing_and_axes(&mut b, |axes| {
            Box::new(coretext::CoreText::new(&[font(axes)]).unwrap())
        });
        assert!(coretext::CoreText::new(&[bad()]).is_err());
    }
}

#[cfg(all(test, feature = "text-coretext"))]
mod script_tests {
    use super::*;
    fn backend(fallback: bool) -> coretext::CoreText {
        let fonts = [
            Some(FontSource {
                bytes: include_bytes!("../fonts/JetBrainsMono-wght-2.211.ttf"),
                axes: &[],
            }),
            Some(FontSource {
                bytes: include_bytes!("../fonts/NotoSansDevanagari.ttf"),
                axes: &[],
            }),
            Some(FontSource {
                bytes: include_bytes!("../fonts/NotoSansArabic.ttf"),
                axes: &[],
            }),
        ];
        let b = coretext::CoreText::new(&fonts).unwrap();
        if fallback {
            b.with_fallback_fonts(&[1, 2]).unwrap()
        } else {
            b
        }
    }
    #[test]
    fn native_boundaries_preserve_clusters_and_unicode_break_rules() {
        let mut b = backend(false);
        let style = Style {
            font: 0,
            size: 12.,
            line_height: 16.,
            letter_spacing: 0.,
        };
        assert_eq!(wrap(&mut b, "ab-cd", style, 32.).unwrap()[0].text, "ab-");
        let marks = b.text_boundaries("e\u{301}x").unwrap();
        assert_eq!(marks.graphemes, vec![3, 4]);
        let conjunct = b.text_boundaries("क्षि").unwrap();
        assert_eq!(conjunct.graphemes, vec!["क्षि".len()]);
        let cjk = b.text_boundaries("你好，世界。").unwrap();
        assert!(!cjk.lines.contains(&"你好".len()), "no break before comma");
        assert!(
            !cjk.lines.contains(&"你好，世界".len()),
            "no break before full stop"
        );
        assert!(cjk.lines.len() > 1);
        let nbsp = b.text_boundaries("hello\u{a0}world").unwrap();
        assert_eq!(nbsp.lines, vec!["hello\u{a0}world".len()]);
        let thai = "ภาษาไทยภาษาไทย";
        let breaks = b.text_boundaries(thai).unwrap();
        assert!(breaks.lines.len() > 1 && breaks.lines.len() < thai.chars().count());
        for text in ["a\u{200b}b", "क्\u{200d}ष"] {
            assert!(
                backend(true)
                    .measure_runs(&[TextRun::new(text).size(24., 40.)])
                    .is_ok(),
                "{text}"
            );
        }
    }
    #[test]
    fn explicit_fallback_preserves_complete_clusters_and_automatic_font_height() {
        let input = [TextRun::new("Fonts: हिन्दी / العربية").size(22., 40.)];
        assert!(backend(false).measure_runs(&input).is_err());
        let mut b = backend(true);
        let resolved = resolve_runs(&mut b, &input).unwrap();
        assert_eq!(
            resolved.iter().map(|r| r.style.font).collect::<Vec<_>>(),
            [0, 1, 0, 2]
        );
        assert_eq!(
            resolved.iter().map(|r| r.text.as_str()).collect::<String>(),
            input[0].text
        );
        let explicit = [
            TextRun::new("Fonts: ").size(22., 40.),
            TextRun::new("हिन्दी").font(1).size(22., 40.),
            TextRun::new(" / ").size(22., 40.),
            TextRun::new("العربية").font(2).size(22., 40.),
        ];
        let actual = b.raster_runs(&input, 400., 2.).unwrap();
        let expected = b.raster_runs(&explicit, 400., 2.).unwrap();
        assert_eq!(actual.rgba, expected.rgba);
        assert_eq!(actual.metrics.advance, expected.metrics.advance);
        let auto = resolve_runs(&mut b, &[TextRun::new("हिन्दी").auto_line_height()]).unwrap();
        assert_eq!(auto[0].style.font, 1);
        assert_eq!(
            auto[0].style.line_height,
            b.natural_line_height(auto[0].style).unwrap()
        );
        assert!(
            coretext::CoreText::new(&[None])
                .unwrap()
                .with_fallback_fonts(&[0])
                .is_err()
        );
        assert!(b.measure_runs(&[TextRun::new("valid").font(63)]).is_err());
        let mut restricted = backend(true).with_fallback_for(0, &[2]).unwrap();
        assert!(restricted.measure_runs(&[TextRun::new("हिन्दी")]).is_err());
    }
    #[test]
    fn rtl_context_and_cluster_safe_truncation_survive_wrapping() {
        let mut b = backend(true);
        let input = [
            TextRun::new("مرحبا بالعالم هذا نص عربي مع English 123 لاختبار التفاف السطور")
                .font(2)
                .size(24., 40.),
        ];
        let lines = layout_runs(&mut b, &input, 285., true).unwrap();
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| {
            l.runs
                .iter()
                .all(|r| r.direction == TextDirection::RightToLeft)
        }));
        let truncated = layout_text(
            &mut b,
            &input,
            [285., 40.],
            true,
            ParagraphStyle::default(),
            TextTruncation::Ending,
            None,
        )
        .unwrap();
        let first: String = lines[0].runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(
            truncated[0]
                .runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<String>(),
            format!("{first}…")
        );
        let text = "a\u{301}".repeat(12);
        let lines = layout_text(
            &mut b,
            &[TextRun::new(&text).size(22., 32.)],
            [60., 32.],
            true,
            ParagraphStyle::default(),
            TextTruncation::Ending,
            None,
        )
        .unwrap();
        let displayed: String = lines[0].runs.iter().map(|r| r.text.as_str()).collect();
        assert!(displayed.ends_with("\u{301}…"));
        assert!(layout_runs(&mut NoText, &[TextRun::new("हिन्दी")], 100., true).is_err());
        for text in ["🙂", "a\u{202e}b", "a\u{fe0f}"] {
            assert!(validate_characters(text).is_err());
        }
        assert!(
            b.measure_runs(&[TextRun::new("عربي").text_decoration(TextDecoration::Underline)])
                .is_ok()
        );
        assert!(
            b.measure_runs(&[TextRun::new("ΕΛΛΗΝΙΚΑ").text_case(TextCase::Lower)])
                .is_err()
        );
    }
}
