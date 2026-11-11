# Tasks

figleaf is useful when a real app draws a real Figma-designed control through
it and the result looks right beside the Figma export. Work on the first unmet
item unless the owner says otherwise.

## Where things stand (2026-11-12)

Works: rectangles, rounded rectangles, ellipses, solid fills, strokes in all
three alignments, drop and inner shadows, rectangular and rounded clips,
single-line text with letter spacing through CoreText, the auto layout solver
with a builder API, and a wgpu renderer that draws inside the host's pass.

Checked beside Figma on two controls (task 1): rounded rectangles and pills,
inside and outside strokes, a drop shadow shown behind its shape and one
hidden under it, a shadow cut by a rounded clipping frame, tracked text in two
weights, nested auto layout and an absolutely placed child. Everything else is
checked only by unit tests, GPU pixel tests and a reviewed render of a made-up
design ([docs/switch.png](docs/switch.png)).

Still assumptions, not yet seen beside Figma:

- inner shadows, and that they paint over the stroke;
- spread, and that it keeps square corners square;
- centre strokes, ellipses that are not circles, per-corner radii, opacity.

Known differences from Figma:

- curved edges are antialiased over a slightly narrower band (up to 28/255 on
  single edge pixels of the switch);
- shadow tails are a little stronger (up to 4/255);
- a fill under a translucent outside stroke can show a faint seam on curves.
  Opaque strokes are handled.

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

Second control, same day: the orange switch from the Abner design system
(node 143:5040) in [examples/abner_switch.rs](examples/abner_switch.rs),
beside Figma's 2x export: [docs/abner-switch-compare.png](docs/abner-switch-compare.png).
Mean difference 1.6/255, largest 28/255 on curved edge pixels.

What it taught:

- A frame with "clip content" and a corner radius clips its children to the
  rounded shape, so rounded clips were added.
- A fill meeting an outside stroke on a curve showed the background through
  the seam. The fill now runs half way under an opaque outside stroke.
- A drop shadow without "show behind" is hidden inside its shape, as assumed.
- An oversized corner radius (17 on a 16-tall frame) makes a pill, and an
  outside stroke adds its width to the radius, as assumed.

The Abner files hold more worth porting when the remaining assumptions matter:
the ON/OFF switch (node 103:2651) has an inner shadow on text, which figleaf
cannot draw; the brush cursor (node 4:362 in Abner Screens) has centre strokes
on ellipses.

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
