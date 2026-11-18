//! Explicit-font, grayscale Latin text, borrowed from figui. No system
//! fallback, bidi or emoji. Styled runs retain explicit fonts and colors. A backend turns one line into a bounded
//! alpha mask; the renderer tints it. [`wrap`] breaks a paragraph into lines.
//! Fonts and their variable axes are caller-supplied; nothing here names a font.
use crate::Error;

mod rich;
pub use rich::{
    ParagraphStyle, RunLine, RunMask, TextCase, TextDecoration, TextRun, layout_paragraphs,
    layout_runs, resolve_runs, resolve_style, validate_runs,
};

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
pub fn supported_char(c: char) -> bool {
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

pub fn validate_characters(text: &str) -> Result<(), Error> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(Error("text exceeds 512 bytes"));
    }
    if !text.chars().all(supported_char) {
        return Err(Error(
            "text has characters outside the single-line Latin scope",
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
pub trait Backend {
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
    }
    unsafe extern "C" {
        fn figleaf_ct_height(font: *mut c_void) -> f64;
        fn figleaf_ct_runs(
            runs: *const StyledRun,
            count: usize,
            scale: f64,
            pixels: *mut u8,
            width: u32,
            rows: u32,
            metrics: *mut Metrics,
        ) -> i32;
        fn figleaf_ct_font(bytes: *const u8, len: usize) -> *mut c_void;
        fn figleaf_ct_drop(font: *mut c_void);
        fn figleaf_ct_instance(
            base: *mut c_void,
            size: f64,
            tags: *const u32,
            values: *const f64,
            count: usize,
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
    /// the host could not supply: text using it fails with a diagnostic instead
    /// of falling back. No automatic font discovery.
    pub struct CoreText {
        fonts: Vec<Option<Loaded>>,
        instances: Vec<(usize, u32, Instance)>,
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
            })
        }
        fn instance(&mut self, style: Style) -> Result<usize, Error> {
            if let Some(i) = self
                .instances
                .iter()
                .position(|(f, s, _)| *f == style.font && *s == style.size.to_bits())
            {
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
            // Tags and values stay alive for this synchronous call and have equal length.
            let ptr = NonNull::new(unsafe {
                figleaf_ct_instance(
                    font.0.as_ptr(),
                    style.size as f64,
                    tags.as_ptr(),
                    values.as_ptr(),
                    tags.len(),
                )
            })
            .ok_or(Error("CoreText font instance failed"))?;
            self.instances
                .push((style.font, style.size.to_bits(), Instance(ptr)));
            Ok(self.instances.len() - 1)
        }
        fn styled(
            &mut self,
            runs: &[TextRun],
            scale: f32,
            pixels: Option<(&mut [u8], u32, u32)>,
        ) -> Result<Metrics, Error> {
            let mut input = Vec::new();
            for run in runs {
                let i = self.instance(run.style)?;
                input.push(StyledRun {
                    font: self.instances[i].2.0.as_ptr(),
                    text: run.text.as_ptr(),
                    len: run.text.len(),
                    height: run.style.line_height as f64,
                    tracking: run.style.letter_spacing as f64,
                    color: run.color.map(f64::from),
                    decoration: run.text_decoration as u32,
                });
            }
            let (ptr, w, h) = pixels.map_or((std::ptr::null_mut(), 0, 0), |(p, w, h)| {
                (p.as_mut_ptr(), w, h)
            });
            let mut metrics = Metrics::default();
            // Input strings and retained instances live across this synchronous call;
            // the output buffer is exactly width*rows*4 bytes and both structs are repr(C).
            let status = unsafe {
                figleaf_ct_runs(
                    input.as_ptr(),
                    input.len(),
                    scale as f64,
                    ptr,
                    w,
                    h,
                    &mut metrics,
                )
            };
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
                    self.instances[index].2.0.as_ptr(),
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
        fn natural_line_height(&mut self, style: Style) -> Result<f32, Error> {
            let mut checked = style;
            checked.line_height = checked.size;
            validate("M", checked, 1., 1.)?;
            let index = self.instance(style)?;
            Ok(unsafe { figleaf_ct_height(self.instances[index].2.0.as_ptr()) } as f32)
        }
        fn measure_runs(&mut self, runs: &[TextRun]) -> Result<Metrics, Error> {
            validate_runs(runs, 1., 1., false)?;
            let resolved = resolve_runs(self, runs)?;
            self.styled(&resolved, 1., None)
        }
        fn raster_runs(
            &mut self,
            runs: &[TextRun],
            width: f32,
            scale: f32,
        ) -> Result<RunMask, Error> {
            validate_runs(runs, width, scale, false)?;
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
            let metrics = self.styled(runs, scale, Some((&mut rgba, w, h)))?;
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
/// box does: at spaces, and at every `\n`. The spaces at a break belong to
/// neither line. A word wider than the box is cut where it stops fitting.
///
/// A host that lays text out calls this to learn the height:
/// `lines.len() as f32 * style.line_height`.
pub fn wrap(
    backend: &mut dyn Backend,
    text: &str,
    style: Style,
    width: f32,
) -> Result<Vec<Line>, Error> {
    if !width.is_finite() || width <= 0. {
        return Err(Error("unsupported text request"));
    }
    let style = resolve_style(backend, style)?;
    let mut advance = |s: &str| -> Result<f32, Error> {
        if s.is_empty() {
            return Ok(0.);
        }
        Ok(backend.measure(s, style)?.advance as f32)
    };
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut rest = paragraph;
        loop {
            // The longest run of whole words that fits.
            let mut fit: Option<(usize, f32)> = None;
            let mut ends: Vec<usize> = rest
                .char_indices()
                .filter(|&(i, c)| c == ' ' && !rest[..i].ends_with(' '))
                .map(|(i, _)| i)
                .collect();
            ends.push(rest.len());
            for end in ends {
                let wide = advance(rest[..end].trim_end_matches(' '))?;
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
                    let mut cut = rest.chars().next().map_or(0, char::len_utf8);
                    let mut wide = advance(&rest[..cut])?;
                    for (i, c) in rest.char_indices().skip(1) {
                        let next = advance(&rest[..i + c.len_utf8()])?;
                        if next > width + 0.001 {
                            break;
                        }
                        (cut, wide) = (i + c.len_utf8(), next);
                    }
                    (cut, wide)
                }
            };
            lines.push(Line {
                text: rest[..end].trim_end_matches(' ').to_string(),
                advance: wide,
            });
            rest = rest[end..].trim_start_matches(' ');
            if rest.is_empty() {
                break;
            }
        }
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
        for t in ["", "a\nb", "🙂", "مرحبا"] {
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
