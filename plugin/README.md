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
`Canvas`. It captures the current dimensions and resolved values, with the
selected root at `(0, 0)`. Auto layout and constraints are already resolved by
Figma: this is a static paint export, not a responsive layout generator. Ancestor
clipping/effects outside the selected root, interactions, component state, and
live variable bindings are not exported. Hidden layers,
paints, and effects are intentionally excluded.

Every visible descendant is validated. Any error suppresses **all** Rust and
identifies its layer path. Selection or page changes clear old output. No image
fallbacks, vector flattening, border rectangles, or partial code are emitted.
An agent can consume the displayed code just as a person can; this plugin does
not install an agent connection or write to a consumer's project.

Supported: affine-transformed rectangles, frames/components/instances/groups,
full ellipses, filled vector paths, stroked straight lines, solid and linear-gradient fills
(up to eight stops), solid uniform/individual borders, and shape shadows.
Nested transforms compose; the root translation is omitted but its linear matrix is retained.
All ancestor frame clips are retained, including unequal corner radii and crossing
transforms. The same clip stack applies to shapes and text.
Simple Latin text needs one solid fill, one style, explicit line height, and
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

Errors include per-region paint overrides, images, masks, layer opacity, blend modes,
singular transforms, unsupported gradients,
multiple/gradient strokes, non-miter undashed rectangle/whole-ellipse borders, blur, container effects,
unavailable/unresolvable variables, Display P3 documents, mixed/automatic-height/decorated/list
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
does not enable Figma mask nodes, which still hard-error.
