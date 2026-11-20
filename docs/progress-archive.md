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


## Ending truncation and maximum lines — 2026-11-18

`Text::text_truncation(Ending)` and `max_lines` use the shared `text::layout_text`
measurement path. Truncation refits the last visible line from the remaining
paragraph, appending an undecorated ellipsis styled from the first omitted
character. Source strings remain intact; disabled truncation ignores stored
line limits. Paragraph gaps, first-line indentation and alignment are preserved.
Invalid limits, missing fonts/glyphs (including hidden text) and exhausted
rendering budgets fail closed. No dependencies or plugin filesystem writes.

[Generated Rust reviewed beside Figma](truncation-compare.png), mean channel
difference 1.31/255 at 2×, covers fixed/partial heights, a box shorter than one
line, auto-height maximum lines, mixed colors/decorations, paragraph spacing,
indentation and exact fits. Figma keeps a line when its midpoint fits the height,
with at least one line; truncation does not implicitly clip overflow. Remaining
glyph-edge differences are visible. Synthetic tests cover disabled limits, UTF-8,
narrow boxes, source budgets, bottom/right alignment, failures and unchanged
uploads. Script-aware shaping/fallback/line breaking is next; live Desktop
plugin validation remains separate.


## Script shaping, explicit fallback and Unicode line breaking — 2026-11-18

CoreText now shapes a documented monochrome BMP profile with resolved paragraph
direction retained across soft wraps. CoreFoundation supplies grapheme boundaries
and Unicode line-break opportunities; emergency wrapping and ellipses never split
clusters. RTL ellipses follow the observed Figma hanging-marker behavior. Explicit
host fallback slots (global or per primary font) resolve whole graphemes and
joining-script words before font-derived auto height. Missing primary fonts,
missing coverage and resource overflows fail closed; no system faces are guessed.

Plugin output retains Unicode and explicit font manifests, and states that Figma’s
implicit fallback chain is unavailable. Emoji/color glyphs, variation selectors,
bidi controls, RTL decoration/tracking, non-Latin case conversion and font changes
inside graphemes remain unsupported. No new library dependencies; two pinned OFL
Noto test/example fonts add 1.49 MB, with hashes/licenses in `fonts/`.

[Generated comparison](scripts-compare.png), reviewed beside Figma at 2×, covers
Greek/Cyrillic, Devanagari conjuncts, combining accents, Arabic with English and
numbers, RTL truncation and mixed fonts. Mean channel difference 1.53/255; glyph
rasterization differs. Tests verify fallback against explicit-font pixels, CJK
punctuation/Thai dictionary boundaries, nonbreaking spaces, joins, invalid inputs,
failed chains and unchanged uploads. Those boundary tests are not CJK/Thai visual
parity or live Desktop-plugin evidence. Richer text fills/strokes are next.

## 2026-11-19 — richer text fills and strokes

- Added explicit text/run paint stacks and layer stroke paints/alignment. CoreText
  extracts the same shaped glyph contours used for measurement, including basic
  decorations; the existing vector renderer draws them with text-box paint
  coordinates. Disjoint contours are partitioned to bound GPU work while keeping
  holes and overlapping glyphs together. No new dependencies.
- Exporter retains native text, emits paint/stroke stacks, and resolves mixed-range
  image assets in memory. Unsupported advanced stroke styles and fill-blend
  combinations remain hard errors, with no partial output or filesystem writes.
- Ran generated `text_paints` beside a live Figma export: 14 editable text nodes,
  JetBrains Mono Regular (wght 400), 2×. [Comparison](text-paints-compare.png)
  covers all gradient types, stacked fills/strokes, all stroke alignments, holes,
  underline, wrapping, mixed ranges, an image fill and translucent overlap. Mean
  channel difference 1.05/255; edge/image-sampling differences remain. Source
  snapshot and generated Rust are checked in; temporary Figma nodes were removed.
- Synthetic tests exercise outline metrics, holes, paint changes, asset failures,
  unsupported combinations and unchanged-canvas zero uploads. This is exporter
  fixture validation, not completion of the live-plugin UI milestone.

## 2026-11-19 — lists and OpenType styling

- Added bulleted/ordered lists, five nesting levels, decimal/alphabetic/Roman
  counters, list spacing and hanging markers. Markers remain separate from source
  text; wrapping, truncation, mixed paints, strokes and caching share existing
  rendering. Live mixed-size/color cases established the initial list marker
  style and baseline, rather than guessing from each item's text.
- Added boolean OpenType overrides and real small caps (`smcp`/`c2sc`), validated
  against supplied fonts before shaping. Feature-aware font instances are cached;
  missing features and backends that do not opt in fail closed. Preserved default
  kerning at zero tracking by removing the old kerning-disabling zero attribute.
- Exporter emits list settings and sorted feature tags, still read-only and
  all-or-nothing. RTL lists, vertical/random features, and explicit kerning with
  nonzero tracking remain unsupported. Source Serif 4 is a licensed test asset,
  not a library dependency or runtime download.
- Ran generated `lists_opentype` at 2× beside Figma: 12 editable text cases in
  JetBrains Mono and Source Serif 4; [comparison](lists-opentype-compare.png), mean
  channel difference 0.56/255. Small caps use the supplied font's real glyphs;
  edge/position differences remain. Saved source snapshot/generated Rust and
  removed temporary nodes. Explicit feature toggles are synthetic validation:
  the Figma plugin API exposes that map read-only. This does not complete the
  separate live development-plugin UI milestone.

## 2026-11-19 — custom underlines

- Added solid/dotted/wavy underline styles, Auto/Pixels/Percent thickness and
  offsets, solid color overrides, and enabled/disabled skip-ink to Text/TextRun.
  Separate contours preserve color and overflow behind glyphs; actual glyph
  intersections determine skip gaps. Zero thickness hides the decoration.
- Plugin emits the inspector settings and verifies decoration color bindings.
  Unsupported/custom strikethrough settings still suppress all Rust. The plugin
  remains read-only. CoreGraphics path operations require macOS 13+ for custom
  skip-ink; unsupported backends fail explicitly. No dependencies added.
- Captured 12 live Figma cases and rendered generated Rust at 2×; see the
  [comparison](custom-decorations-compare.png). Glyph edges and small skip-gap
  differences remain. PNG exports preserve custom color whereas the observed SVG
  exports used the text fill; validation uses PNG. Synthetic checks cover color,
  percentages, zero thickness, overflow, invalid values and unchanged uploads.
  This does not complete the separate live development-plugin UI milestone.

## 2026-11-20 — RTL lists, decorations and tracking

- Removed RTL style exclusions in CoreText and the exporter. Decorations follow
  shaped visual runs rather than logical endpoints; tracking trims the visual
  line end. RTL auto underlines use outline skip-ink. Lists mirror markers and
  use observed leading-edge gutters, with wrapped/truncated lines and alignment.
- Saved 18 Arabic/Hebrew Figma cases, their node snapshot and generated Rust;
  [comparison](rtl-text-compare.png) covers tracking of both signs, mixed bidi,
  underlines/strike/waves, ordered/bulleted lists, nesting/hanging and alignment.
  Font/edge differences remain, including unexpected extra Figma counter glyphs.
  The Hebrew reference requires explicit wght=400/wdth=100 axes. Its pinned OFL
  font adds 112,640 test/example bytes, no dependency or default-build cost.
- Automatic decoration colors across mixed fills differed semantically; kept
  that combination as a hard exporter error and used explicit underline colors
  in the mixed-color reference. Explicit bidi controls remain unsupported.
- Synthetic regression coverage checks disjoint decoration spans, marker
  direction, continuation insets, wrapping/truncation and unchanged uploads.
  The plugin remains read-only and all-or-nothing. This is not live validation
  of the development-plugin UI.

## Text completeness — 2026-11-20

Completed the horizontal-text milestone: mixed runs, automatic line height,
paragraph spacing/indentation, case/decorations, truncation, script-aware shaping,
explicit fallback, lists/OpenType, richer paints, custom underlines and RTL handling.
Earlier comparison links are retained below.

**Earlier completed work.** Mixed-style runs with explicit fonts,
   colors, sizes and tracking are done (2026-11-17); generated output was
   [reviewed beside Figma](text-runs-compare.png). Automatic line height is
   done (2026-11-18), with [font-derived rendering and plugin output](auto-height-compare.png).
   Paragraph spacing and first-line indentation are done (2026-11-18),
   [including per-paragraph settings](paragraphs-compare.png).
   Basic underline/strikethrough and upper/lower/title case are done (2026-11-18),
   [including mixed styles](decorations-compare.png).
   Ending truncation and maximum line counts are done (2026-11-18),
   [including partial heights and mixed styles](truncation-compare.png).
   Script-aware shaping, explicit fallback and Unicode line breaking are done
   for the [documented BMP profile](REFERENCE.md#script-aware-text-and-explicit-fallback)
   (2026-11-18), with [representative script comparisons](scripts-compare.png).
   Richer text fills and aligned/stacked strokes are done (2026-11-19),
   with [mixed paints and silhouette comparisons](text-paints-compare.png).
   Lists, OpenType overrides and real small caps are done (2026-11-19), with
   [list and small-caps comparisons](lists-opentype-compare.png).
   Custom underlines are done (2026-11-19), with
   [styles, dimensions, colors and skip-ink comparisons](custom-decorations-compare.png).
   RTL lists, decorations and tracking are done (2026-11-20), with
   [Arabic/Hebrew and mixed-direction comparisons](rtl-text-compare.png).

The final increment adds Unicode scalars beyond the BMP, complete-grapheme font
coverage, automatic decoration paint inheritance, mixed text paint blends and
advanced strokes (dashes, cap/join/miter, resolved variable-width/dynamic/brush
geometry). The exporter remains read-only and all-or-nothing.

The [18-case comparison](text-completeness-compare.png) was reviewed against a live
Figma export at 2×. Mean channel difference: 0.32/255; glyph-edge differences remain.
Independent opaque cells are rendered separately within existing compositing budgets.
Math/music, monochrome emoji clusters, decorations, blends and variable/dynamic text
strokes have visual evidence; brush geometry and fallback/truncation/cache cases also
have synthetic tests. Copied/generated Rust compiles; whole-plugin UI validation has
not been claimed.

Boundaries: 512 UTF-8 bytes/128 runs and existing geometry/GPU budgets; host-owned
exact fonts/fallback. Color fonts, keycap emoji (CoreText separates digit/enclosure),
soft hyphens, explicit bidi/layout controls, noncharacters, non-Latin casing,
vertical/random features and enabled kerning with tracking remain hard errors.
Resolved styled stroke geometry is fixed to its exported text/font/layout. These
are explicit scope limits, not approximations or universal Unicode-parity claims.
Responsive layout is now the first unmet task.

## Responsive layout export, first stage — 2026-11-20

Auto-layout selections generate a `layout::Tree` calculation and draw calls driven
by solved bounds. Nested horizontal/vertical fixed/hug/fill sizing, min/max, padding,
nonnegative gaps, alignment, hidden children and left/top absolute offsets are
supported. Hosts supply root sizes; root min/max apply. No importer or scene runtime.
Text measurement and the remaining responsive semantics stay on the active queue.

Fixed fill allocation when lower and upper constraints compete, and counter-axis
hug sizing with a fill child's minimum. Invalid dimensions, bounds, padding and
root sizes fail instead of producing invalid rectangles. A live Figma probe
confirmed the hug/minimum dependency. The same 17-node generated hierarchy matches
all Figma rectangles at 280×300, 480×360 and 720×440. Solid-color comparisons were
visually reviewed at 2× ([narrow](responsive-layout-280-compare.png),
[medium](responsive-layout-480-compare.png), [wide](responsive-layout-720-compare.png));
this does not claim general pixel parity. The renderer reports no problems and
uploads nothing on unchanged output. Generated code compiles without dependencies;
unsupported responsive descendants suppress all Rust. Development-plugin UI use
has not been claimed. Temporary Figma fixtures were removed after validation.

## Width-dependent responsive text — 2026-11-20

Added host-owned intrinsic measurement callbacks to `Tree::solve_measured`.
Hug widths are measured before width allocation; hug heights use final widths
before nested parent/sibling height layout. Root axes can independently be supplied
or natural. `Text::measure` and rendering share text-source validation and paragraph
layout, retaining mixed fonts/styles, explicit fallback, lists and line limits.
Hard paragraph breaks also work during unwrapped intrinsic-width measurement.

The plugin emits measured text calls and accepts native auto-height/hug text.
Missing fonts and invalid measurements return errors without partially appending
drawing commands. No dependencies, font discovery, filesystem writes or scene
runtime were added. Stroke-inclusive responsive layout remains a hard error.

All ten rectangles in one generated hierarchy match live Figma at widths
320/480/640 (heights 616/496/424). The comparisons were visually reviewed at 2×:
[narrow](responsive-text-320-compare.png), [medium](responsive-text-480-compare.png),
[wide](responsive-text-640-compare.png). Normal font rasterization differences
remain; this is not pixel parity. Tests cover callbacks, nested dependencies,
line limits, failures, compilation and zero unchanged-canvas uploads. Temporary
Figma fixtures were removed; running development-plugin UI validation is still open.

## Responsive layout completion — 2026-11-21

Part 2 is complete within the supported scope, rather than a claim that every
Figma layout combination works. The plugin remains read-only and shows Rust;
the host owns events, state and supplied sizes. Default builds gained no dependencies.

2. **Responsive layout export and solver gaps.** Export Figma auto layout as
   calls to the existing solver instead of only fixed drawing coordinates.
   **First stage done (2026-11-20):** nested horizontal/vertical fixed, hug and fill,
   min/max, padding, nonnegative gaps, alignment and left/top absolute children;
   [one export reviewed at three sizes](docs/responsive-layout-480-compare.png).
   **Measured text done (2026-11-20):** intrinsic width and width-dependent height,
   nested hug sizing and native plugin calls, [reviewed at three widths](docs/responsive-text-480-compare.png).
   **Stroke-inclusive layout done (2026-11-20):** visible inside borders, individual
   widths, hug/fill and measured content; [reviewed fixture](docs/responsive-strokes-240-compare.png).
   **Horizontal wrapping done (2026-11-20):** row breaking, row gaps, hug heights,
   fill-track stretching and space-between distribution; [reviewed at three widths](docs/responsive-wrap-360-compare.png).
   **Resize constraints done (2026-11-20):** plain/absolute min, center, max, stretch
   and scale; [reviewed at three sizes](docs/responsive-constraints-300-compare.png).
   **Baseline alignment done (2026-11-20):** horizontal/wrapped rows, nested first
   baselines, fixed/hug text and native measurement; [reviewed at two widths](docs/responsive-baseline-400-compare.png).
   Fill-height cycles and vertically resizing baseline text still hard-error.
   **Vertical wrapping done (2026-11-20):** columns, hug-width propagation, fill
   stretching and bounded text remeasurement; [reviewed at three heights](docs/responsive-vertical-wrap-360-compare.png).
   **Implicit frame minima done (2026-11-21):** padding/inside-border expansion,
   fill redistribution and responsive border paint order; [reviewed undersized frame](docs/responsive-border-min-2-compare.png).
   **Final stage completed below:** rotated children and grid layout.
   **Done when:** the same generated panel lays out at several supplied sizes
   without re-export, agrees with corresponding Figma layouts, and reports
   unsupported layout semantics. Test text reflow and nested sizing dependencies.

**Final stage:** explicit grid columns/rows with fixed, fractional and hug tracks,
cell anchors/spans, start/center/end alignment, padding/borders, nested stacks and
measured text. Fixed-size rotated nodes reserve their rotated envelope and compose
ancestor transforms for painting and clipping. Their children can still use fill,
hug, wrapping and constraints in the unrotated local box.

One shared-exporter snapshot produces Rust checked against all nine node boxes
at three root sizes, without re-exporting. The `responsive_grid` example was run
and its [280](responsive-grid-280-compare.png), [360](responsive-grid-360-compare.png)
and [480](responsive-grid-480-compare.png) comparisons were visually inspected beside
Figma PNGs. Mean channel differences are 0.18, 0.16 and 0.15/255; this is fixture
agreement, not pixel parity. Repeat preparation reported `changed: false`, zero
uploads and zero created textures. Grid hug/span allocation was also probed in
Figma; width-dependent grid text measurement has synthetic solver coverage.

Boundaries remain hard errors: rotated hug/fill sizing, rotated non-min constraints
and baseline coupling; spans mixing hug and fractional tracks; fractional tracks
on a hugging axis; fill-height baseline cycles, nonconvergent wrapping dependencies,
scaled/skewed responsive nodes, groups, resizing resolved geometry and patterns
sourced from inside the responsive selection. Grid placement exports the current
hierarchy's explicit cells, including cells Figma populated automatically; changing
the set of children remains host-owned. Live plugin UI validation is part 3.
