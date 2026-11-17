//! Figma-flavoured paint primitives and auto layout for small native Rust UIs.
//!
//! The host keeps its own window, event loop, state and render pass. It
//! describes a frame as a [`Canvas`] of shapes and text using the values Figma
//! shows in its inspector, optionally asks [`layout`] where things go, and hands
//! the canvas to [`render::Renderer`] (feature `wgpu`).
//!
//! Colours are straight-alpha encoded sRGB, as Figma shows them. Lengths are
//! logical units; the renderer multiplies by the scale it is given.

pub mod clip;
mod geom;
pub mod layout;
pub mod mask;
pub mod paint;
pub mod path;
pub mod primitives;
pub mod text;

#[cfg(feature = "wgpu")]
pub mod render;

pub use clip::{Clip, ClipGeometry};
pub use geom::{Color, Error, Rect, Transform, hex, hexa};
pub use mask::{Mask, MaskType, Masked};
pub use paint::{
    Canvas, Gradient, LinearGradient, Paint, Shadow, Shape, Stroke, StrokeAlign, Text,
};

pub use path::{PathStroke, StrokeCap, StrokeJoin, VectorPath, WindingRule};
pub use primitives::{ArcData, Primitive};

pub mod image;
pub use image::{Image, ImagePaint, ImageScaleMode, Pattern, PatternAlignment, TileType};

pub mod composite;
pub use composite::{BlendMode, Composite};
