//! Bounded Unicode segmentation supplied by the text backend.
use super::{Error, MAX_TEXT_BYTES};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum TextDirection {
    #[default]
    Auto = -1,
    LeftToRight = 0,
    RightToLeft = 1,
}
/// UTF-8 end offsets. Line opportunities must also be grapheme boundaries.
#[derive(Clone, Debug)]
pub struct TextBoundaries {
    pub graphemes: Vec<usize>,
    pub lines: Vec<usize>,
}
impl TextBoundaries {
    pub fn validate(&self, text: &str) -> Result<(), Error> {
        for values in [&self.graphemes, &self.lines] {
            if text.len() > MAX_TEXT_BYTES
                || values.len() > MAX_TEXT_BYTES
                || values.last().copied() != Some(text.len())
                || values.iter().any(|&i| i == 0 || !text.is_char_boundary(i))
                || values.windows(2).any(|w| w[0] >= w[1])
            {
                return Err(Error("invalid text boundary result"));
            }
        }
        if self
            .lines
            .iter()
            .any(|i| self.graphemes.binary_search(i).is_err())
        {
            return Err(Error("line break splits a grapheme"));
        }
        Ok(())
    }
    pub(crate) fn latin(text: &str) -> Result<Self, Error> {
        if !text.chars().all(|c| super::latin_char(c) || c == '\n') {
            return Err(Error("backend does not support Unicode segmentation"));
        }
        let graphemes: Vec<_> = text.char_indices().map(|(i, c)| i + c.len_utf8()).collect();
        let mut lines: Vec<_> = text
            .char_indices()
            .filter(|(_, c)| *c == ' ' || *c == '\n')
            .map(|(i, c)| i + c.len_utf8())
            .collect();
        if lines.last().copied() != Some(text.len()) {
            lines.push(text.len());
        }
        let result = Self { graphemes, lines };
        result.validate(text)?;
        Ok(result)
    }
}
pub(crate) fn break_space(c: char) -> bool {
    c == '\u{200b}' || c.is_whitespace() && c != '\u{a0}' && c != '\u{202f}'
}
