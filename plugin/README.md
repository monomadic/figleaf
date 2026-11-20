# figleaf → Rust

A read-only Figma Design plugin. Select one hierarchy, click **Generate Rust**,
then inspect or copy the code. It never writes files, changes the design, uses
network access, or integrates code into an application. There are no third-party plugin
runtime or build dependencies.

## Run

In Figma Desktop, choose **Plugins → Development → Import plugin from manifest**
and select this directory's `manifest.json`. Run **figleaf — Rust** from the
Development menu. The committed `dist/code.js` is ready to import.

After editing the plugin sources, run `node plugin/build.js`. That development
build writes the bundle; the running Figma plugin does not write files.

## Contract

Output is a `draw_selection` Rust function appending shapes/text to a caller's
`Canvas`, with the selected root at `(0, 0)`. Hierarchies containing auto layout
also emit `selection_layout(size)` and take `size: Option<[f32; 2]>` in
`draw_selection`, returning a `Result`. `Some(size)` resizes the root within its
min/max bounds; `None` uses authored/hug sizing. Measured text instead uses
independent axis options and a backend, as described below. Other hierarchies retain
static paint output. Values/variable modes remain resolved at export time. Ancestor
clipping/effects outside the selected root, interactions, component state, and
live variable bindings are not exported. Hidden layers,
paints, and effects are intentionally excluded.

Every visible descendant is validated. Any error suppresses **all** Rust and
identifies its layer path. Selection or page changes clear old output. No image
fallbacks, vector flattening, border rectangles, or partial code are emitted.
An agent can consume the displayed code just as a person can; this plugin does
not install an agent connection or write to a consumer's project.

Supported: affine-transformed rectangles, frames/components/instances/groups,
full ellipses, filled vector paths, stroked straight lines, solid and linear/radial/angular/diamond gradient fills
(up to 64 stops), solid uniform/individual borders, and shape shadows.
Nested transforms compose; the root translation is omitted but its linear matrix is retained.
All ancestor frame clips are retained, including unequal corner radii and crossing
transforms. The same clip stack applies to shapes and text.
Supported Unicode text supports mixed runs with solid/gradient/image/pattern paint stacks,
layer strokes (inside/center/outside, including multiple and gradient paints),
explicit or automatic line heights, and
caller-supplied font indices. Code comments identify each required family/style;
the host must supply the exact font and variable-font configuration. Normal
figleaf text limits still apply (including a maximum 2048-unit raster width).

Vector output preserves M/L/Q/C/Z commands, subpaths, nonzero/even-odd holes,
and explicitly unfilled branches. It reads Figma’s resolved fill geometry, with
no topology reconstruction. Regions share the node’s paints and are unioned
before filling; differing per-region paints are rejected. The limits are 256
regions and 4,096 commands; scale-dependent renderer limits also apply.

Smoothed corners export Figma’s resolved fill/stroke contours and path clips,
including individual frame borders. The smoothing value is recorded in a comment;
geometry must be re-exported after changing it. Drop/inner path shadows use
figleaf’s existing approximate blur model. Missing contours are hard errors.

Errors include per-region paint overrides, image adjustments/animation, unknown blend modes,
singular transforms, unsupported gradients,
non-miter undashed rectangle/whole-ellipse borders, progressive blur,
unavailable/unresolvable variables, Display P3 documents, decorated/list
text. The size limit is 5,000 nodes and 128 levels; the renderer also bounds
clip edges and coverage work. New Figma features may need additional validation rules.

Generated code targets figleaf's supported semantics, not pixel identity.
Existing antialiasing, shadow, text, and sRGB-target blending differences remain.
Use an encoded-sRGB `Rgba8Unorm`/`Bgra8Unorm` target for the examples' blending.

## Path strokes

Solid vector and line strokes export butt/square/round caps, miter/bevel/round
joins, SVG miter limits, and inside/center/outside alignment. Open contours are
centered. The exporter traces the original non-branching vector network instead
of treating resolved fill boundaries as stroke centerlines. Curves stay curves;
no outlined-stroke or raster fallback is used. Branches, isolated vertices,
per-vertex joins and rounded vertices still produce errors.
The saved stroke fixture exports and compiles to Rust and was rendered beside
its Figma export; see `examples/path_strokes.rs` and `docs/path-strokes-compare.png`.

## Dashes and endpoints

Dash patterns export on vector/line strokes and uniform rectangle/frame/ellipse
borders. Rounded rectangles are supported. Dashes fit each segment and keep
caps at dash ends and joins at vertices. Unequal dashed borders and dashed effects
remain errors. Patterns are limited to 32 finite, nonnegative entries; renderer
geometry limits still apply.

The five Figma decorations (`ARROW_LINES`, `ARROW_EQUILATERAL`, `TRIANGLE_FILLED`,
`DIAMOND_FILLED`, `CIRCLE_FILLED`) map to native `StrokeCap` values. Endpoint
vertex overrides export independently as `VectorPath::start_cap/end_cap`, even
when the node-wide cap is mixed. Unavailable mixed caps on a LINE node still
fail closed: the API must expose the endpoint values. No outlined-stroke fallback
is emitted. Generated code was rendered beside the saved Figma dash/endpoint
fixture; the complete panel still needs live Figma Desktop validation.

## Variable bindings

Bindings resolve in each consuming node's active/inherited Figma mode, including
aliases across collections, using `Variable.resolveForConsumer`. Figma's evaluated
node/paint properties supply geometry and styling; the plugin never substitutes
a collection's default mode or writes values back to the design. Numeric, color,
boolean and string bindings are accepted where the resulting feature is supported.
Comments record variable names, IDs, resolved literals and mode IDs when available.
Output remains a snapshot: it does not install live bindings or export every theme.
Unavailable variables, unresolved aliases and unsupported resulting features
suppress all Rust. A selection/page change or newer generation discards pending
lookup results. Generate again after editing variables to refresh the snapshot.

## Validation

```
node plugin/build.js --check
node --test plugin/exporter.test.js
```

Tests compile and execute generated Rust, check diagnostics, clipping, escaping,
font contracts, read-only plugin messages, stale-result clearing, and UI states.
Tests include synthetic Plugin API objects and a real Figma person-icon fixture.
Earlier live validation exercised the vector with an explicitly resolved paint.
The new variable resolver has synthetic tests for per-consumer modes, lookup
failures, scalar/text/effect/gradient bindings and asynchronous invalidation;
live variable-bound export in Figma Desktop remains unverified. The complete panel has not yet been
exercised inside Figma Desktop. A browser policy blocked the local HTML visual preview.

Ellipse arcs/sectors/rings, polygons and stars export inspector parameters through
`Shape::ellipse_arc`, `Shape::polygon` and `Shape::star`. Polygon/star corner radii
include clamping at adjacent edges. Rounded arc corners and effects on these
path primitives hard-error; counts are limited to 3–256. Open-arc inside offsets
that reach the ellipse's minimum curvature radius are also rejected. The saved
`primitive-reference.json` exports code compiled and rendered in `examples/primitives.rs`,
reviewed beside its [Figma export](../docs/primitives-compare.png).

Boolean union/subtract/intersect/exclude nodes use Figma's resolved `fillGeometry`
and `strokeGeometry`, including nested and empty results. Operand geometry is
already baked into these paths; children are not drawn separately. Their variable
bindings are still verified and included as provenance comments. Stroke outlines
retain alignment, caps, joins and dashes; effects/unsupported paints remain errors.
Missing geometry hard-errors. This exports a static result, not live boolean
operations. See `examples/booleans.rs` and the [comparison](../docs/booleans-compare.png).

`clipping-reference.json` exercises per-corner and nested transformed frame clips
on shapes and text; generated code runs in `examples/clipping.rs`. The host API
also accepts arbitrary `Clip::paths`, with unions and winding-rule holes. This
is independent of the scoped mask compositing described below.


## Masks

Vector, alpha and luminance masks emit explicit source/content canvases and
`Canvas::masked` calls. A mask applies to following siblings until the next mask,
a clipping frame/component or the parent boundary; nested scopes intersect.
Mask-layer opacity is supported. The source is consumed as a mask, never painted
again. Alpha/luminance sources can use supported shapes, gradients, shadows and
text. Vector sources require shapes; text and nested masks in a vector source
hard-error. Unsupported paints/effects still suppress all output.

Limits are eight nested mask scopes and 32 scopes; the renderer additionally
bounds offscreen target pixels and reports errors for unsupported content.
[Generated Rust beside Figma](../docs/masks-compare.png) covers all three types,
primary-color luminance, zero-alpha vector paints, gradients and scope breaks.
The plugin still only shows Rust and writes no files.


## Variable-width and brush strokes

Nonuniform preset and custom width profiles export Figma’s resolved stroke
outlines. HEIST/BLOCKBUSTER stretch brushes and dynamic strokes are also supported.
The code records their parameters in comments and emits `.stroke_geometry(...)`;
changing the profile or brush requires re-exporting. The consumer retains solid
paint, alpha, transforms, clips and supported shadows as normal figleaf calls.

Scatter brushes, custom brushes, other brush names, aligned open/mixed networks,
invalid profiles and missing outlines hard-error. No constant-width approximation, brush simulation, raster
fallback or Figma mutation is used. Existing geometry/work limits still apply.
[Generated output beside Figma](../docs/complex-strokes-compare.png) includes all
nonuniform presets, custom widths, both brushes and transformed outlines.


## Multiple and gradient strokes

Visible solid and linear/radial/angular/diamond stroke paints export in Figma’s paint order,
with paint opacity and stop alpha preserved. They share the node’s stroke
geometry: widths/alignment, individual borders, dashes and resolved outlines.
Gradient/multi-paint LINE nodes use Figma’s resolved stroke outlines.

Limits: 32 visible stroke paints and 1–64 stops per gradient, plus existing geometry
work limits. Gradients spanning a zero-sized dimension, missing line outlines,
unknown blend modes hard-error.
The plugin still emits no partial Rust and never writes files.
[Generated output beside Figma](../docs/stroke-paints-compare.png) covers paint
stacks, borders, lines, affine transforms and resolved paths.

Nonlinear gradients emit `Gradient::radial`, `Gradient::angular` or `Gradient::diamond` with the full
Figma transform; zero-sized boxes and singular transforms hard-error.
[Gradient comparison](../docs/gradients-compare.png) includes exported fills,
borders, transformed gradients, translucent stops and a 64-stop strip.

## Image and pattern fills/strokes

Static PNG/JPEG/WebP images are read through Figma’s image API, decoded in an
in-memory UI canvas and embedded once per hash as RGBA8 Rust literals. Fill,
Fit, Crop, Tile, quarter-turn rotation and paint opacity are supported. The
plugin still only displays code: no downloads, filesystem writes or project edits.
Limits are one million pixels per image, two million total and 16 MB per encoded
asset. Missing/undecodable images, animation, adjustments, invalid transforms
and unknown blending hard-error.

Patterns resolve `sourceNodeId`, validate its hierarchy and bindings, and emit
a native `Canvas` source. Rectangular and both staggered tile types, fractional
spacing, scale, horizontal/vertical alignment and opacity are retained. Sources
must be visible, nonzero and have an untransformed root; use a containing frame
for rotated content. Negative spacing, cycles, unsupported source content and
more than eight nested/32 distinct sources hard-error. No source is replaced
by an exported bitmap. Pattern-source fonts join the generated font list.

[Comparison](../docs/images-compare.png) covers generated image and pattern paints.
The asset/decoder lifecycle, stale replies, missing sources and cyclic references
have synthetic tests; the complete UI has not been manually run inside Figma.

## Compositing

Layer opacity, all Figma blend modes and drop/inner container shadows export
as explicit `Composite` canvases. Normal containers isolate their children;
Pass Through uses the enclosing backdrop. Paint and shadow blend modes are
preserved, as are ancestor clips around completed effects. The selected
hierarchy is isolated: external document/host pixels are not its backdrop.
Include the intended background in the selection.

Unknown modes, unsupported effects, invalid opacity and
excessive nesting still suppress all Rust. Vector masks containing composite
scopes remain unsupported; alpha/luminance masks can contain them. Renderer
filter/resource limits apply at the consumer’s render scale. No filesystem
writes or raster fallbacks are added.

[Compositing comparison](../docs/compositing-compare.png) covers the generated
output; regression tests cover compilation, validation, bindings and nesting.


## Blur

Normal layer/background blur exports `Composite::layer_blur(radius)` and
`.background_blur(radius, boundary)`. Layer blur flattens content/effects before
opacity; background blur samples earlier content inside the exported hierarchy,
with a rounded or resolved path boundary. Include the backdrop in the selection.
Ancestor clips apply after layer blur. Zero-opacity fills do not activate background
blur. Radii preserve resolved variable bindings and effect transforms.

Progressive or unknown blur types, duplicate blurs of either type, invalid/excessive
radii, unavailable boundaries, and background blur on groups/text/lines hard-error.
Renderer filter limits also apply at the consumer’s scale. The plugin still only
shows Rust and writes no files. [Generated comparison](../docs/blur-compare.png)
was reviewed beside Figma; Gaussian filtering remains approximate.


## Mixed-style text runs

Styled segments export `Text::rich` and explicit `text::TextRun` calls, preserving
font slots, size, pixel/percentage spacing resolved to pixels, color/paint opacity
and explicit or automatic line heights. Runs wrap together inside one aligned layer. Each
font/style is listed in the generated font manifest; the host supplies the exact
font bytes/axes. Run-level variable bindings are verified against the text node.

Ranges must cover the text exactly; missing fonts, unsupported run paints or
blend modes, malformed ranges and unsupported list/decorative styles suppress
all output. Limits are 128 runs and 512 UTF-8 bytes per layer. Remaining list/decorative styles and scripts outside the supported Unicode profile
are future work. The
[generated comparison](../docs/text-runs-compare.png) was reviewed beside Figma;
live plugin execution remains a separate roadmap item.


Automatic line height exports `.auto_line_height()` for plain text and individual
runs. The supplied font resolves the height at render time; no guessed pixels
are emitted. Missing fonts and backends without natural-height/styled-text support
fail closed. [Automatic-height comparison](../docs/auto-height-compare.png) was
reviewed beside Figma; the plugin still only displays Rust and writes no files.


Paragraph spacing and first-line indentation export `.paragraph_spacing(..)` and
`.paragraph_indent(..)` on plain text or individual styled runs. Figma’s paragraph
ranges are preserved: a newline belongs to the preceding paragraph. Conflicting
settings within one paragraph, negative/nonfinite values, an indent consuming
the full width, and hanging punctuation suppress all Rust. The
[paragraph comparison](../docs/paragraphs-compare.png) covers generated output;
no filesystem writes, partial output or live variable runtime are introduced.


Text decorations and case export `.text_decoration(..)` and `.text_case(..)` on
plain text or individual runs, retaining original characters. Supported values
are underline/strikethrough and original/upper/lower/title case. Automatic solid
decorations use the supplied font. Custom underlines export solid/wavy/dotted
styles, pixel/percentage offsets and thickness, solid colors (including verified
variable bindings), and skip-ink settings. Custom strikethrough and invalid values
still suppress all Rust. Transformed text
must also fit the Latin/512-byte limits. The [comparison](../docs/decorations-compare.png)
uses generated Rust; this does not replace live Desktop plugin validation.


Ending truncation exports `.text_truncation(TextTruncation::Ending)` with an
optional `.max_lines(n)`, preserving full source text and styles. The renderer
resolves the visible lines and ellipsis from the supplied fonts and box. Disabled
truncation leaves any stored line limit inactive. Invalid modes or line counts
suppress all output. [Generated reference](../docs/truncation-compare.png) covers
fixed/partial heights, auto-height line limits, mixed styles and paragraph gaps.
The plugin still only displays Rust; live Desktop validation remains separate.


Script-aware output preserves Unicode scalar text, combining marks and explicit
font/style ranges. The host uses CoreText with exact font bytes and optionally
explicit fallback slots; generated comments explain this requirement. The plugin
cannot recover Figma’s implicit glyph fallback chain and does not guess it.
Supplementary symbols, monochrome emoji, variation selectors and ZWJ sequences
retain their source text. Keycap emoji, noncharacters, unsupported controls and
non-Latin case conversion suppress all Rust. Color fonts remain renderer errors. Missing glyph coverage and font
changes inside a grapheme are diagnosed by the renderer. The
[script comparison](../docs/scripts-compare.png) covers generated output;
whole-plugin Desktop validation remains an open task.

Text paint coordinates cover the whole text box, including mixed ranges and
wrapped lines. Empty fills support stroke-only text. Mixed-range image assets
are read and decoded in memory. Text dashes, cap/join/miter settings and paint blends
export native settings. Supported variable-width, dynamic and brush strokes export
resolved stroke geometry; regenerate this geometry when text/font/layout changes. See the [text comparison](../docs/text-paints-compare.png).
The fixture validates exporter output; live development-plugin UI validation
remains a separate task.


Lists export native `.list_options(..)`, `.indentation(..)`, `.list_spacing(..)`
and `.hanging_list(..)` settings per paragraph. Numbering and marker positions
are computed by the layout solver, leaving the source text intact. OpenType's
uppercase Figma tags become sorted lowercase `OpenTypeFeature::new(*b"liga", ..)`
overrides. Small caps and forced small caps retain their text-case settings;
the supplied font must provide the real features. The renderer diagnoses absent
features rather than silently ignoring them.

Invalid/conflicting paragraph settings, indentation outside 0–5 (1–5 for lists),
malformed/vertical/random features, and enabled kerning with nonzero
tracking suppress all output. [Lists and small caps](../docs/lists-opentype-compare.png)
were reviewed against a live Figma export; explicit feature toggles have synthetic
shaping/export tests because the plugin API exposes them read-only. The exporter
still only displays Rust and performs no filesystem writes.

RTL lists, decoration and tracking now export through the same native text API;
[Arabic/Hebrew comparison](../docs/rtl-text-compare.png) includes mixed-direction
styles, nesting, hanging markers and wrapping. Automatic decoration paints across
mixed fills are retained and resolved by the renderer; explicit colors still override
them. The [text completeness comparison](../docs/text-completeness-compare.png) covers
Unicode, inherited decorations, blends and advanced strokes. Source snapshots and
generated-code compilation are fixture evidence, not live development-plugin UI validation.

## Responsive layout — first stage

Nested horizontal/vertical stacks export ordinary `layout::Tree` calls: fixed,
hug and fill axes, min/max, padding, nonnegative gaps, alignment and left/top-pinned
absolute children. Plain frames preserve child offsets. Hidden layers do not
participate. Draw calls use solved bounds and translations, including clips;
layout errors return before appending paint. The plugin still only displays code.

Vertical wrapping/grid layout, baseline alignment, negative gaps, non-left/top constraints,
rotated/scaled/skewed nodes, groups,
pattern sources inside the responsive selection and
responsive resizing of resolved geometry hard-error.
Fixed-height text can wrap in its solved width; auto-height text uses the measured
layout path below to resize surrounding rows.
Selecting a fill-sized root uses its authored size for `None`, since its parent is
outside the export. The host supplies subsequent root sizes.

The same generated Rust was checked against all node rectangles and renders in
three live Figma sizes; see the [comparison](../docs/responsive-layout-480-compare.png).
Whole-plugin UI validation remains a separate task.

### Measured responsive text

Hug-width and auto-height text export native measurement calls. When present,
`selection_layout` and `draw_selection` take `[Option<f32>; 2]`, font slots and a
host-supplied text backend. For example, `[Some(320.), None]` fixes width and hugs
height; `[None, None]` uses the root's authored/hug sizing. Intrinsic width is
measured before fill allocation, then text height at the final width. Nested hug
frames and subsequent siblings follow the result. No system fonts are guessed,
and font/measurement errors return before drawing changes the caller's canvas.

[Three-width Figma comparison](../docs/responsive-text-480-compare.png) covers mixed
styles, paragraph breaks/spacing, indentation and a hug-width label with max width.
Generated code compiles without dependencies; actual font measurement uses the
host's backend. This is export-fixture evidence, not validation of a running plugin
window. `node plugin/build.js --check` checks the local bundle; close/reopen the
development plugin to load a rebuilt bundle. Stroke-inclusive layout still errors.

Stroke-inclusive responsive frames now export their visible stroke widths and
alignment to the solver. Inside strokes add to padding; center/outside strokes
and child strokes do not reserve space. Hidden/absent paints contribute nothing,
zero-opacity visible paints count, and multiple paints count the widths once.
Individual side widths, nested hug/fill and width-dependent text measurement use
the same content area; absolute children retain their offsets.
The generated layout matches ten live Figma rectangles at two widths; see the
[border comparison](../docs/responsive-strokes-240-compare.png). Reopen the local
development plugin to load the rebuilt bundle.

Horizontal wrapped auto layout now exports row gaps and automatic/space-between
track distribution. Fixed/hug/fill sizing, min/max, inside borders, absolute
children and measured text retain the responsive layout path. Invalid row gaps,
unknown distribution, vertical wrapping and baseline alignment produce layer
errors and no Rust. The [three-width fixture](../docs/responsive-wrap-360-compare.png)
checks row breaking, hug height, fill-height stretching and row distribution.
