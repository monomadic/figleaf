# Progress archive — through 2026-11-17

Historical implementation notes and validation evidence, preserved when the roadmap
was reorganized. Dated limitations below may have been superseded by later entries.
Use [TASKS.md](../TASKS.md) for the current priorities and acceptance criteria.
This archive does not authorize work in host applications.

figleaf is useful when a real app draws a real Figma-designed control through
it and the result looks right beside the Figma export. Work on the first unmet
item unless the owner says otherwise.

## Geometry priorities — 2026-11-15

This queue takes precedence over the older numbered progress notes below.
Work top to bottom, ordered by reusable foundations and usefulness for native
UI controls. Extend both the paint API/renderer and the plugin's validation and
Rust output; do not merely remove an exporter error.

1. **General vector paths — done 2026-11-15 (fills).** Open/closed contours, straight segments, quadratic
   and cubic Béziers, multiple subpaths, holes, and nonzero/even-odd fill rules.
   Support Figma vector networks without dropping branches or holes. This
   unlocks real icons and shapes such as the person icon's shoulder lens.
   Implemented `VectorPath`, winding rules, filled-region unions, unfilled
   branches, GPU coverage and plugin Rust output. The generated person icon was
   reviewed beside its Figma export ([comparison](vector-person-compare.png),
   mean 0.82/255 at 4×). Its variable color was resolved only for the fixture;
   the live exporter now resolves bindings in each consumer’s active mode. Other curve/winding cases are
   synthetic. Strokes were added in task 3; effects and per-region paints remain errors.
2. **Affine transforms — done 2026-11-15.** Rotation, reflection, nonuniform
   scale and skew; composed plugin matrices and local geometry, borders,
   gradients, clips, supported effects and text. Singular matrices fail closed.
   Rotation/reflection/skew reviewed beside Figma ([comparison](affine-compare.png),
   mean 0.36/255, edge maximum 78/255). Other cases have synthetic tests.
   Crossing transformed clip intersections and rounded text clips were added in task 7;
   transformed path strokes were added in task 3; path effects remain unsupported.
3. **Complete path strokes — done for non-branching paths 2026-11-15.**
   Butt/square/round caps, miter/bevel/round joins, SVG miter limits, closed
   inside/center/outside alignment with holes, and centered open contours.
   Renderer strokes curves before transforms; plugin traces original network
   centerlines separately from resolved fill regions. Generated Rust reviewed
   beside Figma ([comparison](path-strokes-compare.png), mean 0.07/255).
   Miter limits, degeneracy, transforms, clipping, alpha and caching also tested.
   Branching/isolated networks, per-vertex join overrides and rounded vector vertices
   still hard-error; extend topology support separately. Unequal rounded frame
   borders still have synthetic validation only.
4. **Dashed strokes and endpoint decorations — done 2026-11-16.**
   Fitted dash/gap patterns on curves, closed contours and uniform primitive
   borders, with cap/join behavior and all five Figma endpoint decorations.
   Independent endpoint overrides, tangent orientation and triangle shaft trim
   use native path geometry. Generated Rust reviewed beside Figma
   ([comparison](dashed-endpoints-compare.png), mean 0.12/255; largest local
   difference 232/255 near a curved decorated endpoint). Cache, reflection,
   opacity, malformed data and resource limits have synthetic tests. Dashed
   effects/unequal borders and mixed LINE caps without endpoint data hard-error.
5. **Remaining geometric primitives — done for supported rounding 2026-11-16.**
   Parameterized ellipse arcs, sectors/rings, polygons and stars lower through
   shared paths. Polygon/star circular corners clamp to adjacent edges; open
   arcs retain stroke alignment. Plugin-generated Rust reviewed beside Figma
   ([comparison](primitives-compare.png), mean 0.23/255 at 1×).
   Rounded arc corners, primitive effects/individual borders and inside open-arc
   offsets reaching the minimum curvature radius remain hard errors.
   Invalid geometry, holes and unchanged-canvas uploads have regression tests.
6. **Boolean geometry — done 2026-11-16 (resolved paths).** Union, subtract,
   intersect and exclude export Figma's resulting contours, holes and resolved
   stroke outlines, including nested/empty results. The renderer masks aligned
   outlines against the fill; operands are not painted again. No boolean runtime.
   Generated Rust reviewed beside Figma ([comparison](booleans-compare.png),
   mean 0.22/255 at 1×). Effects/unsupported paints remain hard errors.
   Malformed geometry, overlapping alpha, transforms, bindings and caching tested.
7. **Complete clipping — done 2026-11-17.** Additive, independently transformed
   clips support per-corner rounded rectangles and arbitrary paths on shapes,
   text and supported shadows. Regions union within a clip; every ancestor
   intersects at shared coverage samples. Plugin output retains every frame clip.
   Generated Rust reviewed beside Figma ([comparison](clipping-compare.png),
   mean 1.01/255 at 2×); the circular clip also rendered through the path API.
   Holes, region unions, repeated clips, invalid inputs and unchanged uploads
   have synthetic tests. Resource limits hard-error; mask compositing is covered by task 9.
8. **Corner smoothing — done 2026-11-17 (resolved paths).** Smoothed rectangles,
   frames and rounded paths export Figma contours for fills, individual borders
   and child clips. Vector drop/inner shadows use the existing approximate blur
   model, including spread and stroke-only silhouettes. No parametric squircle
   API; changed smoothing needs re-export. Generated Rust reviewed beside Figma
   ([comparison](smoothing-compare.png), mean 0.78/255 at 2×); shadow edges
   still differ. Missing contours hard-error. Synthetic tests cover holes,
   transforms, invalid input and unchanged uploads.
9. **Masks — done 2026-11-17.** Scoped vector, alpha and luminance masks composite
   content once, with sibling boundaries and nested intersections in plugin output.
   Cached offscreen rendering preserves the host render-pass API and zero uploads
   on unchanged canvases. Mask-layer opacity, supported paints/shadows/text and
   transformed geometry work; vector text/nested-mask sources and blurs
   remain hard errors. Generated Rust reviewed beside Figma
   ([comparison](masks-compare.png), mean 0.20/255 at 2×). Synthetic tests
   cover holes, stroke-only masks, nesting, text, invalid input and caching.
10. **Variable-width and brush strokes — done 2026-11-17 (resolved outlines).**
    All nonuniform presets and custom profiles, HEIST/BLOCKBUSTER stretch brushes
    and dynamic strokes export resolved geometry; no stroke simulation. Centered
    outline rendering excludes unused fill geometry from work limits. Generated
    Rust reviewed beside Figma ([comparison](complex-strokes-compare.png),
    mean 0.12/255 at 1×), including reflection/shear. Tips, holes, opacity, clips,
    invalid outlines and unchanged uploads have synthetic tests. Scatter/custom
    brushes and unverified brush names remain hard errors, never uniform fallbacks.

11. **Multiple and linear-gradient strokes — done 2026-11-17.** Ordered stroke
    paints share geometry across native paths, individual borders and resolved
    outlines. Local gradient axes follow transforms; painted LINE nodes preserve
    resolved placement. Vector masks respect stroke paint visibility. Generated
    Rust reviewed beside Figma ([comparison](stroke-paints-compare.png),
    mean 0.19/255 at 2×). Paint order, invalid inputs and caching are tested.
    Stroke paint stacks remain limited to 32 paints.

12. **Remaining gradients and larger stop counts — done 2026-11-17.**
    Radial, angular and diamond fills/strokes preserve full Figma transforms;
    all four types accept 1–64 stops in a reusable GPU buffer. Plugin output
    compiles and retains hard errors/no filesystem writes. Generated example
    [reviewed beside Figma](gradients-compare.png), mean 0.58/255 at 1×:
    transformed and translucent paints, borders and a 64-stop strip. Synthetic
    tests cover invalid inputs and unchanged uploads.

13. **Image and pattern fills/strokes — done for supported assets 2026-11-17.**
    Host-owned RGBA8 images support Fill/Fit/Crop/Tile and quarter turns. Native
    Canvas pattern sources support rectangular/staggered layouts, scale, fractional
    spacing and alignment. Cached textures preserve zero uploads when unchanged.
    The plugin embeds decoded image pixels and exports source drawing calls, with
    no filesystem writes or raster fallback. Generated Rust [reviewed beside
    Figma](images-compare.png), mean 2.37/255 at 1×; image filtering remains
    softer. Synthetic tests cover asset resolution, invalid inputs and caching.
    Image adjustments/animation, negative spacing and transformed/hidden source
    roots remain hard errors; the plugin UI lifecycle was tested with mocks.

14. **Flattened opacity, blend modes and container effects — done 2026-11-17.**
    Atomic composite canvases retain isolation/pass-through, Figma blend modes,
    paint/effect blending and opacity applied once to completed content. Container
    drop/inner shadows follow its alpha silhouette and ancestor clips. Explicit
    enclosing canvases supply the backdrop; host pixels are never guessed.
    Generated Rust [reviewed beside Figma](compositing-compare.png), mean
    0.72/255 at 1×; shadow filtering remains approximate. Cached scopes and
    failures upload nothing when unchanged. Tests cover saved blend swatches,
    overlap, masks, clipping, validation, compilation and cache behavior.
    Vector masks containing composite scopes
    and requests beyond documented resource/filter limits hard-error.

15. **Layer and background blur — done for normal blur 2026-11-17.**
    Cached premultiplied layer filtering and bounded backdrop filtering preserve
    layer opacity, transformed axes, masks and ancestor clips. Plugin emits native
    composite calls with resolved radii/boundaries; no files or raster fallbacks.
    Generated Rust [reviewed beside Figma](blur-compare.png), mean 1.81/255; Gaussian filtering
    remains approximate. Synthetic tests cover holes, clips, masks, transparency,
    changed backdrops, errors and unchanged uploads. Progressive/duplicate blurs
    and background blur on groups/text/lines remain hard errors. Host pixels are
    not sampled; include the backdrop in the enclosing composite/selection.

For each item: compile generated Rust, test invalid/degenerate geometry and
unchanged-canvas uploads, run an example, and look at its render beside a Figma
export. Until supported end to end, retain layer-specific hard errors and emit
no partial Rust. No plugin filesystem writes, raster fallbacks, or host edits.

Paint and layout gaps are separate from this geometry queue. After geometry,
prioritize: remaining text styling/shaping; responsive layout export
and solver gaps; wide-gamut color and multisampled targets.
These remain unsupported where documented; listing them does not authorize a
file importer, input handling, or a runtime.

## Variable bindings — 2026-11-15

The read-only plugin verifies variables using Figma's consumer-aware resolution
and exports evaluated properties as literals, with binding provenance comments.
Aliases and active/inherited modes use Figma's resolver; no live variable runtime
or theme switching is added. Missing variables and unsupported resulting features
remain hard errors. Synthetic tests cover modes, scalar/text/paint/effect bindings,
lookup failures and stale async results; live Desktop validation remains open.

## Plugin and individual borders — 2026-11-15

Added the read-only [Rust exporter](../plugin/README.md): static selected geometry,
copyable code, all-or-nothing diagnostics, no runtime filesystem/network access.
Generated Rust compiles; synthetic plugin/UI tests pass. Figma Desktop import
and live execution remain unverified (local browser preview was policy-blocked).

`Shape::strokes` supports top/right/bottom/left widths in all three alignments,
with joined rounded contours. The button-bar example uses a real bottom stroke
and was viewed beside its saved Figma export. Unequal rounded cases were rendered
and reviewed, but only against synthetic expectations. GPU tests cover missing
sides, coverage, invalid widths, and unchanged-canvas uploads.

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
design ([docs/switch.png](switch.png)).

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
file) written from Figma's values in [examples/full_range.rs](../examples/full_range.rs)
and rendered beside Figma's export: [docs/full-range-compare.png](full-range-compare.png).
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
(node 143:5040) in [examples/abner_switch.rs](../examples/abner_switch.rs),
beside Figma's 2x export: [docs/abner-switch-compare.png](abner-switch-compare.png).
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
[examples/abner_onoff.rs](../examples/abner_onoff.rs), both states:
[on](abner-onoff-on-compare.png), [off](abner-onoff-off-compare.png).
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
both states: [docs/abner-in-app.png](abner-in-app.png).

What it taught:

- A host with its own ordered display list needs to draw our items between
  its own, so tooltips and overlays still cover them. `Renderer::draw_items`
  draws a range of canvas items; Abner calls it once per shape.
- Abner's surface is sRGB with premultiplied alpha. The renderer's blending
  works there unchanged; colours are converted and blended in linear light,
  like the rest of Abner's UI.
- The host sets its scissor rectangle; we leave it alone.

A control with text followed: Abner's ON/OFF switch and its label
([docs/abner-onoff-in-app.png](abner-onoff-in-app.png)). Abner keeps its
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
[examples/abner_button_bar.rs](../examples/abner_button_bar.rs), beside Figma's
2x export ([docs/abner-button-bar-compare.png](abner-button-bar-compare.png),
mean difference 1.3/255, largest on glyph edges). In Abner the bar itself is
drawn by figleaf, merged into its `main` the same day, seen running in the
crop editor
([docs/abner-button-bar-in-app.png](abner-button-bar-in-app.png)). The
buttons in Figma's bar are placeholders, so Abner keeps its own.

What it taught:

- Figma strokes one side of a frame (the bar's bottom edge). The original
  example used a 1 wide rectangle. Replaced with
  `Shape::strokes` on 2026-11-15; individual borders are now native paint.
- A stroke that takes room in the layout ("strokes included in layout") is
  written as extra padding on that side.
- The bar's fill is translucent. On Abner's sRGB surface it is written as the
  opaque colour it makes on the panel behind it.

Left to do: decide whether hosts depend on figleaf by path or by git, and
whether a host should be able to rasterize figleaf text with its own glyphs.

## 3. Gradients and multiple fills — linear done 2026-11-12

Linear gradients and stacked fills, checked on four pieces of Abner's timeline
from the design system file, in [examples/abner_gradients.rs](../examples/abner_gradients.rs):
a [trim area](abner-trim-area-compare.png) (14:147, largest difference
2/255), a [trim grip](abner-trim-grip-compare.png) (14:148, 4/255), and
markers [4](abner-marker-4-compare.png) and [5](abner-marker-5-compare.png)
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

Remaining: gradients on text and more than 64 stops (reported as a problem). On an sRGB surface a translucent gradient blends in linear
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
[examples/abner_help_text.rs](../examples/abner_help_text.rs): a
[card](abner-help-card-compare.png) whose paragraph wraps in a 304-wide
box (frame 88:183, mean difference 1.3/255) and
[two texts](abner-help-lines-compare.png), one with a line break in it
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
[examples/switchblade_tags.rs](../examples/switchblade_tags.rs), beside Figma's
export of the [whole set](switchblade-tags-compare.png) at 1x (mean
difference 1.4/255) and two pills at 2x:
[person, addable](switchblade-tag-person-addable-compare.png) (2.9/255)
and [heat 1, applied](switchblade-tag-heat1-applied-compare.png)
(2.7/255). Every pill solves to Figma's width; the largest differences are on
glyph edges. The design is set in SF Pro, which the example reads from the
system and does not bundle.

What it taught:

- Icons are vectors with a 1.5 centre stroke and round caps, so
  `Shape::line` was added: one straight segment, drawn as a distance field
  like everything else, with drop shadows. Filled curved paths were added on 2026-11-15; path strokes remain task 3.
- The count carries a drop shadow in two tones, so text drop shadows were
  added (the glyph mask, blurred on the CPU, drawn under the text).
- An inner shadow on a shape paints over its outside stroke, and shows on a
  frame whose fill is hidden, as assumed.
- A minimum width on a hugging frame (the count disc, 18.67) solves as Figma
  solves it.

In Switchblade itself, merged into its `master` on 2026-11-12 (it depends on
figleaf by path, `../figleaf`, with only the `wgpu` feature): the chips of the tag popup draw their
capsule, wells and add icon through figleaf, seen running
([docs/switchblade-in-app.png](switchblade-in-app.png)), and so do the
filter bar's applied pills with their remove well and cross
([docs/switchblade-applied-in-app.png](switchblade-applied-in-app.png)). Switchblade
keeps its own text. Its draw list gained a shape item, and its renderer draws
each shape at its place among its own tiles with `draw_items`. Timed in release builds on an M4 Pro (on battery, window in the background,
so not a reference run): a scripted pass with the tag popup open, forty
cursor moves and a filter applied, twice each on master and the branch. Frame
times are the same within run noise (render p50 8.2 ms and p95 9.1 to 9.4 ms
on both; app time under 0.1 ms on both). A temporary probe put figleaf's
`prepare` at 6 to 12 microseconds a frame for ten shapes, 44 at worst, and
zero bytes uploaded on every frame where the chips did not change.

Applied pills follow the design's tone table since 2026-11-13 (merged into
Switchblade's `master` the same day):
each tone's own well stroke, inner shadow, end padding and cross shadow, and
the fill's own alpha. Seen running with a hot, a cold and a person pill
([docs/switchblade-applied-tones-in-app.png](switchblade-applied-tones-in-app.png)).

Popup chips follow the component's later edit since 2026-11-13 (merged into
Switchblade's `master` on 2026-11-15): on a heat tone the add well is a faint outline
around a faint dark plus with no fill, the count disc is 15 tall, and its
stroke and inner shadow follow the tone, and every ring is the component's
gold. Seen running with Command held
([docs/switchblade-chips-addable-in-app.png](switchblade-chips-addable-in-app.png)).

The component changed again and the app followed it on 2026-11-14 (same
branch): new heat colours, title-case hashtag labels, 12px lead, one count
disc for every heat, and the person icon on people pills
([docs/switchblade-people-in-app.png](switchblade-people-in-app.png)).
The icon is three ellipses; its shoulders are a lens in the design.

Chip widths were wrong until 2026-11-12: Switchblade kept room for the add
well in every count chip, from a hidden layer the Figma component used to
carry. Since the fix (merged into Switchblade's `master` the same day) a chip at rest is the Count form and only takes the Addable form,
20 wider, while ⌘ is held
([docs/switchblade-addable-in-app.png](switchblade-addable-in-app.png)).
Switchblade estimates label widths instead of measuring them, so its chips
match Figma's widths to about a pixel or two, not exactly.

The component's "Person / Count" variant shows the add well at rest in
Figma (95 wide, the same as "Person / Addable"). The owner's decision
(2026-11-12) is no well at rest for a person chip, as for the heat tones, so
the example and Switchblade draw it without one (76 wide) and the export in
`docs/figma` is the earlier one that agrees. That one variant in the Figma
file still differs.

Switchblade's filter panel (Figma "Filter panel", 180:114) draws its
shapes with figleaf (merged into its `master` on 2026-11-13): the glass with its outline and drop shadow as one shape, the field
and picker outlines, the divider, the option cells, the summary capsules,
and the minus, plus and chevron as round-capped lines. Its layout follows
the component too (the owner's call, 2026-11-13): 323 tall expanded, 36px
pickers, an 86px empty capsule, and a minimized bar that hugs its row and
names its rule. All three states were seen running beside the Figma export.
Not seen: hover, a selected option, the "+N" overflow and the size tween.

Left there: shapes in its blurred backdrop pass and a reference timing run (AC power, window in
front, a real library), which was not done before the merge.

Not checked beside Figma: the labels and counts of the review frame (other
strings and widths), hover and pressed states, which the file does not hold.

## Not doing

Importing Figma files, a package format, a scene or panel runtime, input
handling, and pixel-exactness research. Those were figui's direction. The host
owns state and events; the agent reads Figma and writes the calls.


## Mixed-style text runs — 2026-11-17

`Text::rich` and `text::TextRun` preserve explicit fonts, sizes, line heights,
tracking and solid colors. CoreText shapes whole attributed lines; wrapping
crosses style boundaries and retains shared baselines. Native cached glyph
bitmaps retain transforms, clips and text effects. The plugin emits text calls,
verifies contiguous ranges and resolves bindings within runs; no filesystem
writes or text-image export fallback.

Generated Rust [reviewed beside Figma](text-runs-compare.png), mean 1.42/255 at
2×, covering regular/bold, mixed sizes/colors, tracking and wrapped paragraphs.
Synthetic checks cover UTF-8 boundaries, empty lines, missing fonts, malformed
runs, compilation and unchanged uploads. This completes the first text milestone,
not the whole text queue. Automatic line height, broader scripts and remaining
styling are still pending in [TASKS.md](../TASKS.md).


## Automatic line height — 2026-11-18

Plain text and styled runs can select Auto. CoreText resolves the supplied font
instance’s ascent, descent and leading before wrapping, baseline placement and
block alignment; no guessed multiplier or plugin-baked height. The plugin emits
`.auto_line_height()` and retains existing font requirements and hard errors.

Generated Rust [reviewed beside Figma](auto-height-compare.png), mean 2.58/255 at
2×, covering mixed sizes, wrapped lines, empty paragraphs and centered alignment.
Small mixed-size baseline differences remain. Synthetic tests cover resolved
metrics, invalid backends, missing fonts and unchanged uploads. Paragraph spacing
and indentation are next in [TASKS.md](../TASKS.md); live Desktop plugin validation
remains separate.


## Paragraph spacing and first-line indentation — 2026-11-18

Layer defaults and per-run overrides retain paragraph settings. Indentation
reduces only the first line’s wrapping width; continuation lines use the full
box. Paragraph gaps occur at newlines, including empty paragraphs, without a
final trailing gap. Horizontal/vertical alignment uses these layout dimensions.
Plugin output preserves Figma paragraph ranges and validates conflicting styles;
negative/nonfinite settings, exhausted first-line width and list/hanging styles
remain hard errors. No filesystem writes or new dependencies.

Generated Rust [reviewed beside Figma](paragraphs-compare.png), mean 0.57/255 at
2×, covering wrapped text, empty paragraphs, center alignment and per-paragraph
settings. Synthetic tests cover right/bottom alignment, trailing newlines,
validation and unchanged/changed caches. Decorations and case handling are next
in [TASKS.md](../TASKS.md); live plugin validation remains separate.


## Text decorations and case — 2026-11-18

Plain text and runs retain original characters plus explicit case/decoration.
Case resolves before layout, preserving word context across runs and continuation
lines; font-derived underlines and strikeouts follow run colors and layer effects.
Plugin output supports original/upper/lower/title and basic solid automatic
underline/strikethrough. Small caps, custom decoration settings and transformed
text outside current Latin/byte limits are hard errors, with no partial output.

[Generated comparison](decorations-compare.png) was reviewed beside Figma at 2×
(mean channel difference 1.38/255):
underline descenders, strikeouts, accented casing, apostrophes/hyphens, mixed
colors/styles and wrapping. Glyph/antialiasing differences remain. Synthetic tests
cover measurement, no decoration leakage, transformed scope errors and unchanged
uploads. No dependencies or plugin filesystem writes. Truncation/maximum lines
are next; live Desktop plugin validation remains a separate task.
