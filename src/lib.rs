//! Figma-flavoured paint primitives and auto layout for small native Rust UIs.
//!
//! The host keeps its own window, event loop, state and render pass. It
//! describes a frame as a [`Canvas`] of shapes and text using the values Figma
//! shows in its inspector, optionally asks [`layout`] where things go, and hands
//! the canvas to [`render::Renderer`] (feature `wgpu`).
//!
//! Colours are straight-alpha encoded sRGB, as Figma shows them. Lengths are
//! logical units; the renderer multiplies by the scale it is given.

mod geom;
pub mod layout;
pub mod paint;
pub mod path;
pub mod text;

#[cfg(feature = "wgpu")]
pub mod render;

pub use geom::{Color, Error, Rect, Transform, hex, hexa};
pub use paint::{Canvas, LinearGradient, Paint, Shadow, Shape, Stroke, StrokeAlign, Text};

pub use path::{PathStroke, StrokeCap, StrokeJoin, VectorPath, WindingRule};
