# Tasks

figleaf is useful when a real app draws a real Figma-designed control through
it and the result looks right beside the Figma export. Work on the first unmet
item unless the owner says otherwise.

## Where things stand (2026-11-12)

Works: rectangles, rounded rectangles, ellipses, solid and linear-gradient
fills (several per shape), strokes in all three alignments, drop and inner shadows, rectangular and rounded clips,
single-line text with letter spacing and inner shadows through CoreText, the auto layout solver
with a builder API, and a wgpu renderer that draws inside the host's pass.

Checked beside Figma on three controls (task 1) and four timeline pieces
(task 3): rounded rectangles and pills, per-corner radii, linear gradients
with two, three and eight stops, stacked fills, paint opacity,
inside and outside strokes (including 1.5 and 0.5 wide and translucent), an
inner shadow on text, a drop shadow shown behind its shape and one
hidden under it, a shadow cut by a rounded clipping frame, tracked text in two
weights, nested auto layout and an absolutely placed child. Everything else is
checked only by unit tests, GPU pixel tests and a reviewed render of a made-up
design ([docs/switch.png](docs/switch.png)).

Still assumptions, not yet seen beside Figma:

- inner shadows on shapes, and that they paint over the stroke;
- spread, and that it keeps square corners square;
- centre strokes, ellipses that are not circles, shape opacity;
- gradients at an angle (every one checked so far runs straight across or
  straight down).

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

Third control, same day: the ON/OFF switch (node 103:2651) in
[examples/abner_onoff.rs](examples/abner_onoff.rs), both states:
[on](docs/abner-onoff-on-compare.png), [off](docs/abner-onoff-off-compare.png).
Mean difference 2.0/255 on, 1.2/255 off; the largest are on glyph edges.

What it taught:

- Figma's label carries an inner shadow that dims it visibly, so text inner
  shadows were added (the glyph mask, blurred and shifted, on the CPU).
- Inside strokes of 1.5 and 0.5, one of them translucent, match.

The brush cursor (node 4:362 in Abner Screens) has centre strokes on ellipses,
for when those assumptions matter.

## 2. Use it in a real host — started 2026-11-12

Abner's Process rail switch is drawn by figleaf, on branch `figleaf-switch` in
the worktree `~/src/abner-figleaf` (not merged). Seen in the running app in
both states: [docs/abner-in-app.png](docs/abner-in-app.png).

What it taught:

- A host with its own ordered display list needs to draw our items between
  its own, so tooltips and overlays still cover them. `Renderer::draw_items`
  draws a range of canvas items; Abner calls it once per shape.
- Abner's surface is sRGB with premultiplied alpha. The renderer's blending
  works there unchanged; colours are converted and blended in linear light,
  like the rest of Abner's UI.
- The host sets its scissor rectangle; we leave it alone.

A control with text followed: Abner's ON/OFF switch and its label
([docs/abner-onoff-in-app.png](docs/abner-onoff-in-app.png)). Abner keeps its
own glyph atlas for all other text and uses figleaf text, through CoreText and
its bundled font, only where a label needs a Figma text effect.

More that it taught:

- On an sRGB surface a translucent black shadow darkened much less than in
  Figma. The shaders now correct translucent black there. Other translucent
  colours still blend in linear light and differ; Abner writes those as the
  opaque colour they make.
- A host can have several windows from other builds open. Capture by process,
  not by application name.

Left to do: decide whether hosts depend on figleaf by path or by git, and
whether a host should be able to rasterize figleaf text with its own glyphs.

## 3. Gradients and multiple fills — linear done 2026-11-12

Linear gradients and stacked fills, checked on four pieces of Abner's timeline
from the design system file, in [examples/abner_gradients.rs](examples/abner_gradients.rs):
a [trim area](docs/abner-trim-area-compare.png) (14:147, largest difference
2/255), a [trim grip](docs/abner-trim-grip-compare.png) (14:148, 4/255), and
markers [4](docs/abner-marker-4-compare.png) and [5](docs/abner-marker-5-compare.png)
(14:124, 14:128; within 1/255 on the flags, the largest differences are on the
digits' edges).

What it taught:

- Figma blends stops with colour and alpha apart, not premultiplied: white at
  9% fading to grey at 0% turns grey on the way.
- Real designs use hard stops (two stops at one offset) to draw bars, eight
  stops at a time, and Figma antialiases a hard stop that falls inside a
  pixel. The shader averages the ramp over each pixel's width.
- The first row of the Plugin API's `gradientTransform` is all a linear
  gradient needs, so `LinearGradient::transform` takes it as it is.
- Figma dithers gradients by about 1/255; we do not.
- Per-corner radii match (the markers' tabs).

Not done: radial, angular and diamond gradients, gradients on strokes and
text, more than eight stops (reported as a problem, not dropped), and blend
modes on a fill. On an sRGB surface a translucent gradient blends in linear
light like any other translucent colour. Abner does not use these fills yet.

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
