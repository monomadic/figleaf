# Tasks

figleaf is useful when a real app draws a real Figma-designed control through
it and the result looks right beside the Figma export. Work on the first unmet
item unless the owner says otherwise.

## Where things stand (2026-11-12)

Works: rectangles, rounded rectangles, ellipses, solid and linear-gradient
fills (several per shape), strokes in all three alignments, drop and inner shadows, rectangular and rounded clips,
round-capped lines, text with letter spacing, wrapping, inner and drop shadows through CoreText, the auto layout solver
with wrapping and a builder API, and a wgpu renderer that draws inside the host's pass.

Checked beside Figma on three controls (task 1), four timeline pieces
(task 3) and Switchblade's tag pills (task 6): rounded rectangles and pills, per-corner radii, linear gradients
with two, three and eight stops, stacked fills, paint opacity,
inside and outside strokes (including 1.5 and 0.5 wide and translucent), an
inner shadow on text, a drop shadow shown behind its shape and one
hidden under it, a shadow cut by a rounded clipping frame, tracked text in two
weights, nested auto layout and an absolutely placed child. Everything else is
checked only by unit tests, GPU pixel tests and a reviewed render of a made-up
design ([docs/switch.png](docs/switch.png)).

Still assumptions, not yet seen beside Figma:

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
- text in a box at a half-pixel height (the tag's count, at y 6.5) sits one
  physical pixel lower than Figma's.

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

Abner's Process rail switch is drawn by figleaf, merged into Abner's `main`
on 2026-11-12 (it depends on figleaf by path, `../figleaf`). Seen in the running app in
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

Abner's button bar followed on 2026-11-12: the "Button Bar" component (node
463:6046 of the Signal design system file) in
[examples/abner_button_bar.rs](examples/abner_button_bar.rs), beside Figma's
2x export ([docs/abner-button-bar-compare.png](docs/abner-button-bar-compare.png),
mean difference 1.3/255, largest on glyph edges). In Abner the bar itself is
drawn by figleaf on branch `figleaf-button-bar` (worktree
`../abner-button-bar`, not merged), seen running in the crop editor
([docs/abner-button-bar-in-app.png](docs/abner-button-bar-in-app.png)). The
buttons in Figma's bar are placeholders, so Abner keeps its own.

What it taught:

- Figma strokes one side of a frame (the bar's bottom edge). figleaf strokes
  all sides or none, so the edge is drawn as a 1 wide rectangle. Enough here;
  a rounded corner on a stroked side would need more.
- A stroke that takes room in the layout ("strokes included in layout") is
  written as extra padding on that side.
- The bar's fill is translucent. On Abner's sRGB surface it is written as the
  opaque colour it makes on the panel behind it.

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

## 4. Layout wrapping — done 2026-11-12

Horizontal stacks wrap. Two wrapping frames from the Abner design system file
solve to the rectangles Figma reports for them, as unit tests in
`src/layout.rs`: "Filter Segments" (145:3699; five segments on two lines,
centred across, hugging height 50) and "Application palette" (26:3791; twenty
swatches, five to a line, in a fixed frame).

What it taught:

- A line is as tall as its tallest child, children are aligned inside their
  own line, and a hugging frame is as tall as its lines and the gaps between.
- Heights now depend on widths, through where the lines broke. Widths are
  solved first and hand the line breaks to the height pass.

Still assumptions, not seen in a Figma file: lines in a frame taller than they
need are placed as a block by the counter alignment; a child that fills the
width counts as its minimum width when lines are broken. Not done: "space
between" for the lines themselves, and baseline alignment. Do those when a
design needs them.

## 5. Text

Letter spacing: done with task 1. Wrapping in a fixed-width box: done
2026-11-12. Left: a backend that is not tied to macOS.

Wrapping was checked on two help texts from the Abner Timeline Edit file, in
[examples/abner_help_text.rs](examples/abner_help_text.rs): a
[card](docs/abner-help-card-compare.png) whose paragraph wraps in a 304-wide
box (frame 88:183, mean difference 1.3/255) and
[two texts](docs/abner-help-lines-compare.png), one with a line break in it
(frame 88:275, mean 1.9/255). Lines break where Figma breaks them.

What it taught:

- Wrapping sits above the backend: `text::wrap` measures words with whatever
  backend the host has and each line is drawn as its own single-line mask, so
  a new backend still only needs to shape one line.
- Figma puts the baseline on a whole logical pixel. At 11 / 17 our lines sat
  one physical pixel low until the renderer did the same; this also brought
  marker 5 closer (mean 3.7 to 2.5). The other comparisons did not move.
- Figma's export of a lone text layer is cropped to its ink, so placement has
  to be checked on an export of the frame around it.

Still assumptions: a word wider than its box is cut where it stops fitting;
breaks happen only at spaces, not after hyphens. Not done: paragraph spacing,
truncation with an ellipsis, mixed styles in one text.

For a host's own text field, `text::caret_offsets` gives where a caret can
stand in a line (added 2026-11-12). Editing, focus and keys stay with the host.

## 6. Switchblade's tag pill — done 2026-11-12

The "Tag pill" component set (node 178:105 of the Switchblade Design System
file): five tones in four forms, in
[examples/switchblade_tags.rs](examples/switchblade_tags.rs), beside Figma's
export of the [whole set](docs/switchblade-tags-compare.png) at 1x (mean
difference 1.4/255) and two pills at 2x:
[person, addable](docs/switchblade-tag-person-addable-compare.png) (2.9/255)
and [heat 1, applied](docs/switchblade-tag-heat1-applied-compare.png)
(2.7/255). Every pill solves to Figma's width; the largest differences are on
glyph edges. The design is set in SF Pro, which the example reads from the
system and does not bundle.

What it taught:

- Icons are vectors with a 1.5 centre stroke and round caps, so
  `Shape::line` was added: one straight segment, drawn as a distance field
  like everything else, with drop shadows. Curved paths are still not done.
- The count carries a drop shadow in two tones, so text drop shadows were
  added (the glyph mask, blurred on the CPU, drawn under the text).
- An inner shadow on a shape paints over its outside stroke, and shows on a
  frame whose fill is hidden, as assumed.
- A minimum width on a hugging frame (the count disc, 18.67) solves as Figma
  solves it.

In Switchblade itself, merged into its `master` on 2026-11-12 (it depends on
figleaf by path, `../figleaf`, with only the `wgpu` feature): the chips of the tag popup draw their
capsule, wells and add icon through figleaf, seen running
([docs/switchblade-in-app.png](docs/switchblade-in-app.png)), and so do the
filter bar's applied pills with their remove well and cross
([docs/switchblade-applied-in-app.png](docs/switchblade-applied-in-app.png)). Switchblade
keeps its own text. Its draw list gained a shape item, and its renderer draws
each shape at its place among its own tiles with `draw_items`. Timed in release builds on an M4 Pro (on battery, window in the background,
so not a reference run): a scripted pass with the tag popup open, forty
cursor moves and a filter applied, twice each on master and the branch. Frame
times are the same within run noise (render p50 8.2 ms and p95 9.1 to 9.4 ms
on both; app time under 0.1 ms on both). A temporary probe put figleaf's
`prepare` at 6 to 12 microseconds a frame for ten shapes, 44 at worst, and
zero bytes uploaded on every frame where the chips did not change.

Left there: shapes in its blurred backdrop pass, the add well seen running (it
shows only with ⌘ held), and a reference timing run (AC power, window in
front, a real library), which was not done before the merge.

Not checked beside Figma: the labels and counts of the review frame (other
strings and widths), hover and pressed states, which the file does not hold.

## Later, when a design needs it

Rounded clips, flattened layer opacity, images, vectors, corner smoothing,
layer and background blur, blend modes, multisampled targets.

## Not doing

Importing Figma files, a package format, a scene or panel runtime, input
handling, and pixel-exactness research. Those were figui's direction. The host
owns state and events; the agent reads Figma and writes the calls.
