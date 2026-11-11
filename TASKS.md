# Tasks

figleaf is useful when a real app draws a real Figma-designed control through
it and the result looks right beside the Figma export. Work on the first unmet
item unless the owner says otherwise.

## Where things stand (2026-11-12)

Works: rectangles, rounded rectangles, ellipses, solid fills, strokes in all
three alignments, drop and inner shadows, rectangular clips, single-line text
with letter spacing through CoreText, the auto layout solver with a builder
API, and a wgpu renderer that draws inside the host's pass.

Checked beside Figma on one control (below): rounded rectangles, an inside
stroke, a drop shadow shown behind its shape, tracked text in two weights, and
nested auto layout. Everything else is checked only by unit tests, GPU pixel
tests and a reviewed render of a made-up design ([docs/switch.png](docs/switch.png)).

Still assumptions, not yet seen beside Figma:

- inner shadows paint over the stroke, and shadows follow fill and stroke together;
- growing a shape (outside and centre strokes, spread) keeps square corners
  square and adds to round ones;
- a drop shadow without "show behind" is hidden exactly inside the shape;
- ellipses, per-corner radii and opacity.

## 1. One real control beside its Figma export — done 2026-11-12

"Abner / Processing / Full Range" (node 214:399 of the Abner Timeline Edit
file) written from Figma's values in [examples/full_range.rs](examples/full_range.rs)
and rendered beside Figma's export: [docs/full-range-compare.png](docs/full-range-compare.png).
The layout solves to Figma's 210x111 and every box lands where Figma put it.
Largest differences are at glyph edges; the mean is 1.4/255.

What it taught:

- Sigma is half of Figma's blur: the glow matches within 3/255, slightly
  stronger in the tail (the edge-distance approximation).
- A frame's stroke lies under its children. The glow tints the track's stroke.
- Letter spacing was needed straight away, so it was added. Figma does not
  count the space after the last glyph in a hugging text box, and neither does
  the measured advance.
- Figma's export of a node includes shadow spill to the right and bottom.

Next real control should cover the assumptions still listed above: a switch
with a knob (ellipse, drop shadow hidden under the shape) and an inner shadow.

## 2. Use it in a real host

Replace one hand-drawn control in one of the owner's apps (they are wgpu 30
hosts) with figleaf calls. Needs the owner's go-ahead for that repository. This
is where the API's rough edges show: surface formats, multisampled passes,
scale factors, when to rebuild the canvas.

## 3. Gradients and multiple fills

Linear gradients first, with Figma's handle positions and stops.

## 4. Layout wrapping

Wrapping stacks. Then baseline alignment if a design needs it.

## 5. Text

Letter spacing, then wrapping in a fixed-width box, then a backend that is not
tied to macOS.

## Later, when a design needs it

Rounded clips, flattened layer opacity, images, vectors, corner smoothing,
layer and background blur, blend modes, multisampled targets.

## Not doing

Importing Figma files, a package format, a scene or panel runtime, input
handling, and pixel-exactness research. Those were figui's direction. The host
owns state and events; the agent reads Figma and writes the calls.
