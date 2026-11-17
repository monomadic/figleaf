//! Atomic paint scopes, not scene nodes. The host still owns its display list.
use crate::{Canvas, Shadow, Transform};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum BlendMode {
    #[default]
    Normal = 0,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    LinearBurn,
    LinearDodge,
    PassThrough,
}
/// Flatten the content before applying opacity, blend mode and container shadows.
/// Normal isolates children; PassThrough lets them see the enclosing backdrop.
/// Non-normal modes require an enclosing composite canvas. Root composites never
/// sample the host's render target.
#[derive(Debug, Clone, PartialEq)]
pub struct Composite {
    pub content: Canvas,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub shadows: Vec<Shadow>,
    pub shadow_blend_modes: Vec<BlendMode>,
    /// Effects use this local-to-canvas transform; content is already positioned.
    pub effect_transform: Transform,
    pub clips: Vec<crate::Clip>,
}
impl Composite {
    pub fn new(content: Canvas) -> Self {
        Self {
            content,
            opacity: 1.,
            blend_mode: BlendMode::Normal,
            shadows: Vec::new(),
            shadow_blend_modes: Vec::new(),
            effect_transform: Transform::IDENTITY,
            clips: Vec::new(),
        }
    }
    pub fn intersect_clip(mut self, clip: crate::Clip) -> Self {
        self.clips.push(clip);
        self
    }
    pub fn opacity(mut self, value: f32) -> Self {
        self.opacity = value;
        self
    }
    pub fn blend_mode(mut self, value: BlendMode) -> Self {
        self.blend_mode = value;
        self
    }
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadows.push(shadow);
        self
    }
    pub fn shadow_blend_modes(mut self, modes: Vec<BlendMode>) -> Self {
        self.shadow_blend_modes = modes;
        self
    }
    pub fn effect_transform(mut self, value: Transform) -> Self {
        self.effect_transform = value;
        self
    }
}
