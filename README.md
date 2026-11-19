# figleaf

Figma-flavoured paint primitives and auto layout for small native Rust UIs.

You design a control in Figma. An agent (or you) reads the numbers off the
inspector and writes them down as figleaf calls, instead of hand-drawing
rounded rectangles, strokes and shadows again in every app. The host keeps its
own window, event loop, state and render pass; figleaf only answers "where does
this go" and "draw it the way Figma would".

![The switch example](docs/switch.png)

That image is `cargo run --example switch --features wgpu,text-coretext`; its
design is made up for the example.

Three real controls have been checked against Figma so far, all written from
the values Figma reports. In each pair Figma's export is on the left and
figleaf is on the right.

The switch from the Abner design system ([examples/abner_switch.rs](examples/abner_switch.rs)),
enlarged 6x:

![Figma beside figleaf: switch](docs/abner-switch-compare.png)

The ON/OFF switch from the same file ([examples/abner_onoff.rs](examples/abner_onoff.rs)),
enlarged 4x:

![Figma beside figleaf: ON/OFF switch](docs/abner-onoff-on-compare.png)

A segmented control from the Abner Timeline file ([examples/full_range.rs](examples/full_range.rs)),
enlarged 3x:

![Figma beside figleaf: segmented control](docs/full-range-compare.png)

Switchblade's tag pill, all twenty variants
([examples/switchblade_tags.rs](examples/switchblade_tags.rs)), enlarged 2x:

![Figma beside figleaf: tag pills](docs/switchblade-tags-compare.png)

Abner's button bar ([examples/abner_button_bar.rs](examples/abner_button_bar.rs)),
Figma on the left:

![Figma beside figleaf: button bar](docs/abner-button-bar-compare.png)

Fills, strokes, clips and layout land on the same pixels; shadows are within
4/255; curved edges are antialiased slightly differently; glyph edges differ
because Figma does not rasterize text with CoreText. What has and has not
been checked is in [TASKS.md](TASKS.md).

## Figma plugin

[figleaf → Rust](plugin/README.md) displays copyable Rust for a selected hierarchy.
Import [plugin/manifest.json](plugin/manifest.json) in Figma Desktop's Development
menu. Unsupported features block all output. The plugin has no filesystem or
network access; it exports the current geometry, not responsive layout behavior.

## Paint

```rust
use figleaf::{Canvas, Rect, Shadow, Shape, StrokeAlign, Text, hex, hexa};

let mut canvas = Canvas::new();
// Track: 44x24, corner radius 12, fill #34C759.
canvas.shape(Shape::rect(Rect::new(0., 0., 44., 24.)).radius(12.).fill(hex(0x34C759)));
// Knob: 20x20 ellipse, white, drop shadow X0 Y2 blur 4 at #000 20%.
canvas.shape(
    Shape::ellipse(Rect::new(22., 2., 20., 20.))
        .fill(hex(0xFFFFFF))
        .stroke(0.5, hexa(0x0000000A), StrokeAlign::Outside)
        .shadow(Shadow::drop(hexa(0x00000033), [0., 2.], 4.)),
);
canvas.text(Text::new("Sync to grid", Rect::new(56., 4., 120., 16.)).size(13., 16.));
```

| Figma | figleaf |
| --- | --- |
| Rectangle, corner radius, per-corner radii | `Shape::rect(..).radius(..)` / `.radii([tl, tr, br, bl])` |
| Ellipse | `Shape::ellipse(..)` |
| Fill (solid) | `.fill(hex(0xRRGGBB))`, `hexa(0xRRGGBBAA)` |
| Fill (linear gradient), paint opacity | `.gradient(LinearGradient::transform([a, b, tx]).stop(0., color).stop(1., color).opacity(0.5))` |
| Several fills | call `.fill(..)` / `.gradient(..)` once per fill, bottom first |
| Stroke, inside / center / outside | `.stroke(width, color, StrokeAlign::..)` |
| Individual frame borders, top / right / bottom / left | `.strokes([top, right, bottom, left], color, StrokeAlign::..)` |
| Drop shadow X, Y, blur, spread | `.shadow(Shadow::drop(color, [x, y], blur).spread(s))` |
| "Show behind transparent areas" | `.show_behind()` |
| Inner shadow | `.shadow(Shadow::inner(color, [x, y], blur))` |
| A vector's straight stroke with round caps (icons) | `Shape::line(from, to, width).fill(color)` |
| Clip content on a frame | `.intersect_clip(Clip::rounded_rect(rect, radii))` |
| Text, size / line height, alignment | `Text::new(..).size(13., 16.).align(..)` |
| Letter spacing (pixels) | `.letter_spacing(0.7)` |
| Fixed-width text box, line breaks | `.wrap()`; `text::wrap(..)` gives the lines, for the height |
| Inner shadow on text | `.inner_shadow(color, [x, y], blur)` |
| Drop shadow on text | `.drop_shadow(color, [x, y], blur)` |

figleaf draws a text field's look but does not edit text. For the host's
caret and selection, `text::caret_offsets(backend, string, style)` gives the
distance from the start of the line to each place a caret can stand.

Colours are straight-alpha encoded sRGB, the numbers Figma shows.

Individual borders use one continuous ring with elliptical corner contours,
including zero-width sides. `cargo run --example frame_borders --features wgpu`
renders synthetic rounded cases. Abner's button-bar example now uses a real
bottom border and was reviewed beside its saved Figma export; unequal rounded
border widths have synthetic validation only. No new dependencies were added.

## Affine transforms

`Shape::transform` and `Text::transform` take Figma's two matrix rows:
`Transform::new([[a, c, tx], [b, d, ty]])`. Geometry, borders, gradients and
supported effects stay in local coordinates. `parent.compose(child)` applies
the child first, then the parent. Rotation, reflection, nonuniform scale and
skew work on primitives, filled paths and text; singular/non-finite matrices
are reported in `Renderer::problems`.

Plain `.clip(...)` remains in canvas coordinates. Single-clip helpers remain
available, including `clip_transformed`. For nested clips, both `Shape` and
`Text` accept repeated `.intersect_clip(Clip::rounded_rect(rect, radii).transform(m))`
calls. Radii are top-left, top-right, bottom-right, bottom-left. Clips have
independent transforms and are intersected without replacing any ancestor.
`Clip::paths(paths)` supports arbitrary vector regions and winding-rule holes;
regions union within a clip, while separate clips intersect. An empty path clip
clips everything out. The plugin preserves every frame clip inside the selection;
Figma masks use scoped compositing, described below.

Clip stacks apply to fills, strokes, gradients, text and their supported shadows.
They share coverage samples, so duplicate clips do not compound edge opacity.
Limits are 128 clips and 8,192 flattened edges per item, plus a coverage-work and
GPU-buffer budget; invalid or excessive clips report `Renderer::problems` and
suppress the whole item. No new dependencies.

`cargo run --example clipping --all-features` renders generated code beside
[Figma](docs/clipping-compare.png): unequal corners, crossing rotated rounded
clips and clipped text (mean 1.01/255 at 2×, largest 121/255 on text).
A second render uses a circular path clip for the same reference. Path holes,
region unions, invalid geometry, shadows and caching have synthetic tests.

Run `cargo run --example affine --features wgpu` for rotation/reflection/skew
beside Figma ([comparison](docs/affine-compare.png)): mean difference 0.23/255,
maximum 78/255 at an edge. Nonuniform scale, transformed text/effects, nested
composition and cache behavior have synthetic tests. Text rasterizes at the
maximum magnification, within the existing backend limits (including 4× scale),
and samples its transformed mask with bilinear filtering. No new dependencies;
GPU storage adds 64 bytes per shape and 64 per text instance.

## Vector paths

`Shape::vector(rect, paths)` paints local-coordinate `VectorPath` regions with
solid/gradient fills and the existing rectangular/rounded clips. Paths
have `move_to`, `line_to`, `quadratic_to`, `cubic_to`, and `close` builders.
`WindingRule::NonZero` and `EvenOdd` preserve holes; `None` preserves explicitly
unfilled network branches. Open filled contours close implicitly. Regions are
unioned before each fill, so overlaps do not accumulate opacity.

The plugin reads Figma's resolved `fillGeometry` plus unfilled `vectorPaths`.
Per-region paint overrides remain hard errors. Solid path
strokes are supported as described below. Run `cargo run --example vector_paths --features wgpu` for the person
icon from [Switchblade's design](https://www.figma.com/design/p01oYypZrTOjxe4Pi2Ytxe?node-id=336-1230).
Its bound color was explicitly resolved for the saved fixture. The plugin now resolves bindings in the consumer’s active
mode and records their provenance in comments. Figma's 4× export is left, generated Rust's render right:

![Person vector comparison](docs/vector-person-compare.png)

Reviewed at 4×: mean channel difference 0.82/255, maximum 29/255 at an edge.
Other winding/curve cases have synthetic tests. No new dependencies: the CPU
subdivides curves to 0.125 physical-pixel tolerance and the GPU uses 4×4 coverage
samples. This is intended for small UI vectors, not large illustrations. Limits
are 256 regions, 4,096 commands, 8,192 subdivided edges per shape, and 32 MiB of
scene edge storage (or the device limit). A shape exceeding 512 million estimated
edge/sample checks is reported in `Renderer::problems`, as are invalid paths
and unsupported styles. Unchanged geometry uploads nothing. `Layer` now borrows
its source vector paths; hosts consuming layers must retain the source shapes.

## Path strokes

Vectors accept `.stroke(width, color, StrokeAlign::Inside/Center/Outside)`,
`.stroke_cap(StrokeCap::Butt/Square/Round)`,
`.stroke_join(StrokeJoin::Miter/Bevel/Round)`, and `.stroke_miter_limit(ratio)`
(the SVG ratio, default 4, minimum 1). Closed contours use their winding rules
for inside/outside alignment, including holes; open contours use centered strokes.
Strokes are formed in local coordinates before affine transformation. Overlaps
within one stroke are unioned before applying opacity. Fill and stroke remain
separate paints. `stroke_paths(paths)` supplies independent centerlines for a
network whose resolved filled regions differ from its stroked edges.

The plugin traces non-branching vector networks exactly once and exports native
stroke calls, including curved segments. Branches, isolated network vertices,
per-vertex join overrides and rounded vector vertices
remain hard errors in the native centerline export. Resolved styled outlines and
path shadows are supported as described below.

`cargo run --example path_strokes --features wgpu` renders generated plugin code
beside Figma's 2× export ([comparison](docs/path-strokes-compare.png)): curved caps,
acute joins, thick aligned strokes and holes; mean channel difference 0.07/255,
maximum 72/255 at an edge. Miter-limit changes, zero-length round/square caps,
transforms, clipping, alpha and caching also have synthetic GPU tests.

The optional `wgpu` feature now includes `tiny-skia-path` 0.11.4 for curve
stroking, adding three crates (`tiny-skia-path`, `strict-num`, `arrayref`), about
280 KiB of installed Rust source. Linked size depends on the host's build.
Default builds still have no dependencies. Stroke expansion shares the existing
4,096-command / 8,192-edge and GPU work limits; exceeding them reports a problem
rather than omitting part of a shape.

## Dashes and endpoints

`.dash_pattern(vec![12., 8.])` accepts Figma's alternating dash/gap lengths.
Patterns fit each segment with a dash centered on its vertices; adjacent pieces
join across corners. Odd sequences repeat; zero entries follow Figma's evaluated
pattern behavior, and empty/all-zero patterns draw solid strokes. Curves remain
curves throughout dash splitting. Rectangles (including rounded corners), ellipses,
and vector paths support dashed uniform strokes. Unequal dashed borders, dashed
effects, and dashes on the legacy `Shape::line` helper produce errors; use a
stroked `Shape::vector` for dashed lines, as the plugin does.

`StrokeCap` also has `ArrowLines`, `ArrowEquilateral`, `TriangleFilled`,
`DiamondFilled`, and `CircleFilled`. Set a node-wide cap with `.stroke_cap(...)`,
or override each open path's endpoints with `VectorPath::start_cap(...)` and
`end_cap(...)`. Decorations follow endpoint tangents and stroke width before
transforms; triangle shafts are shortened, and decorations are unioned with the
stroke before opacity. Closed contours have joins instead of decorations.

The plugin emits these calls, including different caps at a vector's two ends.
Unsupported/missing endpoint styles, mixed joins and branching networks
still fail closed in native centerline export. A decorated zero-length path reports that it has no
tangent. Patterns allow at most 32 finite, nonnegative entries; dash subdivision
and stroke outlines retain explicit command, region, edge and GPU work limits.

`cargo run --example dashed_endpoints --features wgpu` renders generated code
beside Figma ([comparison](docs/dashed-endpoints-compare.png)): curved dash caps,
aligned closed paths, all five decorations, mixed endpoints and primitive borders.
Mean channel difference is 0.12/255; the largest local difference is 232/255 near
a curved decorated endpoint. Synthetic tests also cover reflected arrows, shaft
trimming, dash phase, opacity, malformed patterns and unchanged-canvas uploads.
No additional dependencies were added for this task.

## Layout

```rust
use figleaf::layout::{Align, Node, Sizing, Tree};

// A row: 12 padding, label left, switch right, centred vertically.
let mut tree = Tree::new(Node::row().padding(12.).align(Align::SpaceBetween, Align::Center));
let label = tree.add(tree.root(), Node::fixed([120., 16.]));
let switch = tree.add(tree.root(), Node::fixed([44., 24.]));

let solved = tree.solve([320., 48.])?;   // on every resize
let where_the_switch_goes = solved[switch];
```

Covers horizontal and vertical auto layout, wrapping (`.wrap(gap_between_lines)`),
padding, gap, alignment including space-between, fixed / hug / fill sizing, min
and max sizes, and absolutely positioned children. `solve_natural` sizes a hugging root to its content. Text
does not measure itself: ask the text backend for the advance and set the node.

## Drawing

With the `wgpu` feature the renderer draws inside the host's own render pass:

```rust
let mut renderer = figleaf::render::Renderer::new(&device, surface_format);
// when the UI changes:
let report = renderer.prepare(&device, &queue, &canvas, [width_px, height_px], scale, &mut text)?;
// in the host's pass:
renderer.draw(&mut pass);
```

A host that keeps its own ordered list of things to draw can call
`renderer.draw_items(&mut pass, range)` to put its drawing between ours.

An unchanged canvas uploads nothing and reports `changed: false`. Anything
that could not be drawn is listed by `renderer.problems()`; nothing is dropped
silently. A host on another renderer can call `Shape::layers` and draw the
coverage layers itself.

Text needs a backend. `text-coretext` (macOS, needs clang) rasterizes with
CoreText from font bytes you supply; `text::NoText` is for hosts with none.

## Features and dependencies

| Feature | Adds | Dependencies |
| --- | --- | --- |
| none | layout, display list, text types | none |
| `wgpu` | the renderer | wgpu 30 |
| `text-coretext` | CoreText text backend | system frameworks; clang and ar at build time |

## Not supported yet

More than 64 gradient stops, image adjustments/animation, progressive blurs, multisampled targets, layout
grid, baseline alignment and space-between for wrapped lines, and for text: hyphenation,
emoji, explicit bidi controls, script-specific case conversion,
and any backend other than CoreText.

## Where it came from

figleaf is a sister of [figui](../figui), which tried to import whole Figma
frames into a runtime. figleaf keeps the parts that turned out to matter: the
auto layout solver and the CoreText text backend are borrowed from there; the
shape renderer is new.

### Ellipse arcs, polygons and stars

`Shape::ellipse_arc(rect, ArcData::new(start, end, inner_radius))` retains
Figma's angles in radians (clockwise from +x) and inner-radius ratio 0–1.
The signed sweep is `end - start`: zero is empty, one full turn makes a ring,
and an inner radius of 1 leaves only the arc stroke. Open arc strokes retain
inside/center/outside alignment and caps. `Shape::polygon(rect, point_count)`
and `Shape::star(rect, point_count, inner_radius)` retain the inspector's
ellipse-inscribed vertices; `.radius(r)` rounds convex and concave corners,
clamping where adjacent corners meet. Counts are limited to 3–256.

These parameters lower to shared vector geometry for fills, gradients, strokes,
dashes, transforms and clipping, with no new dependencies. Rounded arc corners,
primitive effects and individual borders remain errors. Inside open-arc strokes
whose offset reaches the ellipse's minimum curvature radius also fail explicitly.
`Primitive::paths` exposes the generated paths in rectangle coordinates.

`cargo run --example primitives --features wgpu` renders plugin-generated code
beside [Figma's reference](docs/primitives-compare.png): sectors, partial/full
rings, open-arc alignment, polygons, rounded/clamped stars and dashed borders.
Mean difference 0.23/255 at 1×, largest 224/255 near rounded-star dashes.
Dense strokes remain subject to the renderer’s coverage-work budget;
other edge cases have synthetic tests.

### Resolved boolean geometry

The plugin exports union, subtract, intersect and exclude nodes as their resolved
`Shape::vector` contours, including holes, nested operations and empty results.
Operands supply geometry; they are not painted again. Their variable bindings
are still verified and recorded. This is a static snapshot of Figma's result,
not a boolean-operation engine or scene runtime.

`Shape::stroke_geometry(paths)` supplies resolved outlines alongside `.stroke(...)`.
The outlines already contain caps, joins and dashes. Like Figma's `strokeGeometry`,
inside/outside outlines must be expanded to twice the nominal width; the renderer
masks them against the vector fill, including its holes. Regions paint once even
where outlines overlap. Mixing resolved outlines with centerline styles is an error.
Unsupported effects and paints still hard-error; missing geometry never falls
back to operands or a bounding box. No new dependencies.

`cargo run --example booleans --features wgpu` renders plugin-generated code
beside [Figma's export](docs/booleans-compare.png): all four operations, aligned
and dashed outlines, a nested reflected gradient and an empty intersection.
Mean difference 0.22/255 at 1×; other validation and caching cases are synthetic.


## Corner smoothing

The plugin preserves Figma's smoothed rectangles, frames and rounded paths as
resolved `VectorPath` contours, with resolved stroke outlines for borders and
`Clip::paths` for clipped children. Smoothing values remain in code comments;
changing a radius or smoothing value requires exporting the geometry again.
There is no parametric squircle approximation or new dependency.

Vector paths now support drop and inner shadows, including spread, transformed
outlines and stroke-only silhouettes. They use the existing signed-distance
blur approximation, not an exact Gaussian convolution; overlapping contours and
narrow features can differ from Figma. Container shadows use explicit composite canvases.
Missing fill/stroke geometry and unsupported effects block the entire export.

Run `cargo run --example smoothing --features wgpu` for the generated fixture.
[Figma beside figleaf](docs/smoothing-compare.png) covers smoothing 0/0.6/1,
unequal frame borders and clipping, rounded stars/polygons, and shadows at 2×.
Mean channel difference is 0.78/255; visible shadow-edge differences remain.
Synthetic tests cover holes, reflected stroke-only shadows, invalid inputs and
unchanged-canvas uploads.


## Masks

`canvas.masked(Mask::new(MaskType::Alpha, mask_canvas), content_canvas)` adds
one atomic paint operation. `Vector` uses filled/stroked shape outlines,
ignoring nonzero paint alpha; `Alpha` uses rendered opacity; `Luminance` uses
perceived brightness in encoded sRGB. `Mask::opacity(value)` applies mask-layer
opacity. Both canvases use their containing canvas’s coordinates. Nested scopes
intersect, and overlapping content is composited before masking once.

`prepare` renders changed scopes into two offscreen GPU textures and submits
that work before the host’s render pass. `draw` and `draw_items` retain their
existing API; a scope is one item. Unchanged scopes reuse their textures and
upload nothing. No new dependencies; default builds remain dependency-free.
Limits: eight nested scopes, 32 scopes and 32 million target pixels summed over
scopes (two RGBA textures per scope), plus device texture limits. Invalid content
suppresses its entire scope and appears in `Renderer::problems`.

The plugin preserves sibling scope, mask replacements, clipping-frame breaks,
and nested intersections. Supported shapes, gradients, shadows and text work
in alpha/luminance masks. Vector masks containing text or other masks still
hard-error; normal blurs use composite canvases.
Run `cargo run --example masks --features wgpu` for the generated fixture:
[Figma beside figleaf](docs/masks-compare.png), mean 0.20/255 at 2×. The comparison
covers rounded/elliptical boundaries, primary-color luminance, mask opacity,
transparent vector paints, gradients, nesting and clipping-frame scope breaks.
Synthetic tests also cover holes, strokes, reflected geometry, text and caching.


## Variable-width and brush strokes

The plugin exports WEDGE, TAPER, QUARTER_TAPER, EYE, MIRRORED_TAPER and CUSTOM
width profiles through Figma’s resolved `strokeGeometry`. HEIST and BLOCKBUSTER
stretch brushes (either direction) and dynamic strokes use the same mechanism.
Profile/brush parameters stay in comments; editing them requires re-exporting.
There is no brush simulation, texture fallback or new dependency.

`Shape::stroke_geometry(paths)` already draws these filled outlines, preserving
holes, overlap opacity, transforms, clips and supported shadows. The nominal
stroke width does not widen the baked outline again. Centered outlines now omit
the unused fill domain from coverage work; inside/outside outlines retain it.

Scatter/custom brushes and other brush names remain hard errors. Positive-width
styled strokes require nonempty resolved outlines; aligned open/mixed networks
remain unsupported. Missing contours never fall
back to constant width. Existing paint, path-size and renderer work limits apply.
`cargo run --example complex_strokes --features wgpu` renders generated Rust
[beside Figma](docs/complex-strokes-compare.png): every nonuniform preset, a custom
profile, both supported brushes, dynamic geometry, reflection, shear and opacity.
Mean difference is 0.12/255 at 1×; synthetic tests cover tips, holes, clipping,
nominal-width independence, invalid data and unchanged uploads.


## Multiple and gradient strokes

`Shape::stroke_paints(Vec<Paint>)` replaces the stroke color with up to 32 ordered
solid/gradient paints, bottom to top. They share the shape’s stroke width,
alignment, individual border weights and path styling, as in Figma. Call it after
`.stroke(...)` or `.strokes(...)`; an empty list uses the legacy stroke color.
`stroke_gradient(width, gradient, align)` sets a single gradient stroke.

Gradient coordinates use the shape’s local box and follow its affine transform.
An axis along a zero-height line works; an axis spanning a zero-sized dimension
is rejected. The plugin uses resolved outlines for gradient/multi-paint LINE
nodes, preserving Figma’s supplied placement. Native vector paths, resolved
outlines, individual borders, dashes and masks retain stroke paint stacks.
Gradients allow 1–64 stops; image and pattern strokes use the same geometry.
No new dependencies or shader variants.

`cargo run --example stroke_paints --features wgpu` renders generated Rust
[beside Figma](docs/stroke-paints-compare.png), mean 0.19/255 at 2×: solid/gradient
stacks, alignments, an ellipse, unequal borders, a line, transformed dashes and
variable-width/smoothed outlines. Synthetic tests cover paint order, gradient
coordinates, invalid stacks, vector masks and unchanged-canvas uploads.

## Radial, angular and diamond gradients

`Gradient::radial(matrix)`, `Gradient::angular(matrix)` and
`Gradient::diamond(matrix)` take the full two-row Figma `gradientTransform`.
Use `.stop(position, color)` and `.opacity(value)`, then `.gradient(..)` for
fills or `.stroke_gradient(width, gradient, align)` for borders. Explicit paint
stacks use `Paint::Gradient(..)`. `LinearGradient` remains a compatible alias.
All four types accept 1–64 stops, including repeated positions for hard edges.
Nonlinear gradients require a nonzero box and an invertible transform.

Stops live in a reusable GPU storage buffer; unchanged canvases upload nothing.
Linear hard edges retain pixel-width filtering; nonlinear gradients use 4×4
samples per pixel. Figma dithering is not reproduced.
`cargo run --example gradients --features wgpu` renders plugin-generated Rust
[beside Figma](docs/gradients-compare.png): identity and sheared transforms,
translucent stops, borders and a 64-stop strip. Visually checked at 1×; mean
channel difference 0.58/255. Synthetic tests cover rejection and upload caching.

## Image and pattern paints

`Image::new([width, height], rgba_bytes)` holds immutable, shared straight-alpha
sRGB RGBA8 pixels supplied by the host; no decoder or I/O dependency is added.
Use `.paint(Paint::Image(ImagePaint::new(image).scale_mode(ImageScaleMode::Fit)))`.
Modes are `Fill` (default), `Fit`, `Crop(Transform)` and `Tile(scaling_factor)`;
`.rotation(degrees)` accepts quarter turns outside Crop. Crop takes Figma’s
full image transform. Paint opacity and image alpha are preserved.

`Pattern::new(source_canvas, [width, height])` repeats native figleaf drawing.
It supports `.scaling_factor(..)`, `.spacing([x, y])` (fractions of tile size),
`.tile_type(TileType::Rectangular/HorizontalHexagonal/VerticalHexagonal)`,
horizontal/vertical alignment and opacity. Use `Paint::Pattern(..)` in fills
or stroke paint stacks. The renderer caches the native source in an offscreen
tile at the requested drawing scale; no Figma raster fallback is involved.

Images and pattern tiles share shape/path coverage, clips, transforms and masks.
Unchanged sources upload nothing; invalid sources report problems and suppress
the whole shape. Limits: 16 million pixels per texture, 32 million active pixels
per renderer, plus device limits and the existing eight-level/32-scope budget.
Negative pattern spacing is unsupported.

`cargo run --example images --features wgpu` renders plugin-generated Rust
[beside Figma](docs/images-compare.png): placement modes, quarter turns, crop
transparency, rounded clips, image borders and all three pattern layouts. Mean
channel difference is 2.37/255 at 1×; bilinear image edges are softer than this
Figma export. Synthetic tests cover errors, placement, staggering and caching.

## Flattened opacity, blending and container shadows

`canvas.composite(Composite::new(content).opacity(0.5))` paints an atomic canvas,
then applies opacity once. Overlapping children do not become darker from
repeated opacity. `Shape::opacity` also flattens that shape’s paints/effects in
the renderer. This is a paint operation, with no scene or widget runtime.

`Composite::blend_mode(BlendMode::Multiply)` and the other Figma blend modes
blend against earlier content inside an enclosing composite. `Normal` isolates
children; `PassThrough` exposes the enclosing backdrop, including when opacity
is reduced. Shape paints use `.fill_blend(mode)` after adding a fill and
`.stroke_blend_modes(vec![..])` after configuring the stroke paints.

The outer composite is isolated and draws normally into the host render pass.
It cannot read existing host pixels; a non-normal root mode reports a problem.
Include the intended backdrop in the enclosing canvas. `draw_items` treats a
composite as one atomic item. Internal blending uses encoded sRGB; presentation
converts to the host’s target format. No new dependencies.

Container `.shadow(..)` effects use the completed content’s alpha silhouette,
including holes and clipped children. Drop/inner shadows, spread, transformed
offsets, effect blend modes, and ancestor clips are supported. Gaussian shadow
filtering is approximate.

Cached scopes upload nothing when unchanged. Invalid scopes report a problem
and draw no partial content. Limits: eight nesting levels, 128 composite scopes (including paint scopes),
64 million intermediate pixels, 256 million filter samples, and device limits.
Shadow spread is limited to 32 physical pixels and the blur kernel to a
128-pixel radius, after the effect transform and render scale.

`cargo run --example compositing --features wgpu` renders generated Rust
[beside Figma](docs/compositing-compare.png): all blend modes, opacity, isolation,
pass-through against a colored sibling backdrop, paint blending and container
shadows. Mean channel difference: 0.72/255 at 1×; shadow edges still differ.
The saved Figma blend swatches and synthetic overlap/mask/cache cases are tested.


## Layer and background blur

`Composite::layer_blur(radius)` filters completed premultiplied content, including
its effects, before layer opacity and ancestor clipping. A blurred Pass Through
scope isolates its content. `Composite::background_blur(radius, boundary)` filters
earlier content inside an enclosing composite, then paints the foreground sharply.
The boundary is a canvas-space `Clip` (rounded rectangle or paths, including holes);
it is independent of fill alpha. Background sampling clamps to its local bounds.
Use `.effect_transform(..)` for local blur axes and scale, and transform the
boundary separately like other clips. Root background blur reports a problem:
include the intended backdrop in an enclosing canvas, not in the host render target.

Both filters use cached separable Gaussian passes; no dependencies were added.
The existing composite resource limits apply. Invalid/negative/nonfinite radii,
excessive filter work and invalid boundaries report problems without partial output.
The plugin exports normal blur radii and resolved boundaries, including bindings;
zero-opacity fills do not activate background blur. Progressive/duplicate blurs,
and background blur on groups, text or lines still hard-error.

`cargo run --example blur --features wgpu` renders plugin-generated Rust
[beside Figma](docs/blur-compare.png), mean 1.81/255 at 1×. Filtering is approximate;
background boundary edges differ by up to 38/255 in this fixture. Tests cover transformed kernels, premultiplied transparency,
clipping, masks, holes, backdrop changes, invalid input and zero unchanged uploads.


## Mixed-style text

Use `Text::rich(vec![..], rect)` with `text::TextRun::new("text")` runs, each
with `.font(index)`, `.size(size, line_height)`, `.letter_spacing(pixels)` and
`.color(color)`. Layer alignment, wrapping, transforms, clips and shadows still
apply; the plain-text style setters do not override explicit run styles.
`text::layout_runs` returns measured lines for hosts that need text dimensions.

CoreText shapes each attributed line as a whole, preserving shared baselines
and shaping across style boundaries. Fonts and variable axes remain explicitly
supplied by the host; missing glyphs/fonts reject the whole layer. Custom text
backends opt into `measure_runs`/`raster_runs`; their defaults report unsupported
styled text. Cached colored glyph bitmaps use the existing image/composite
renderer, with no new dependencies or runtime asset loading.

Limits remain 512 UTF-8 bytes, 128 runs, the Unicode profile below and
explicit or automatic line heights. Scripts
outside the supported BMP profile remain unsupported. The plugin verifies contiguous Figma ranges and resolves
bindings inside runs; it emits native text calls, never exported text images.

`cargo run --example text_runs --all-features` renders generated Rust
[beside Figma](docs/text-runs-compare.png): regular/bold fonts, colors, mixed sizes,
tracking and wrapping. Mean channel difference is 1.42/255 at 2×; glyph edges
still differ. Synthetic tests cover UTF-8 boundaries, empty paragraphs, font
failures, invalid styles, shared metrics and unchanged uploads.


## Automatic line height

`Text::auto_line_height()` and `text::TextRun::auto_line_height()` select Auto
(stored as zero in `Style::line_height`). CoreText resolves it from the supplied
font instance’s ascent, descent and leading, including the selected size/axes.
There is no guessed size multiplier. Runs resolve before wrapping; each line
uses its participating runs’ heights, and empty paragraphs retain their font’s
height. Alignment and caching use the resolved block height.

Hosts can use `text::resolve_style`/`resolve_runs` before measuring or validating
styles, or `layout_runs` for measured paragraphs. Plain `Text::origin` helpers
require an explicit/resolved height. Custom backends must implement
`natural_line_height` and the styled-text rendering methods for this renderer
path; unsupported or invalid metrics and missing fonts report problems.
The plugin emits `.auto_line_height()` for Figma Auto instead of baking a height.

`cargo run --example auto_height --all-features` renders generated Rust
[beside Figma](docs/auto-height-compare.png): plain/mixed fonts and sizes, wrapping,
blank paragraphs and centered alignment. Mean channel difference: 2.58/255 at
2×; small mixed-size baseline differences remain. Synthetic tests cover font
metrics, empty lines, invalid backends, missing fonts and unchanged uploads.


## Paragraph spacing and indentation

`Text::paragraph_spacing(pixels)` adds space between newline-delimited paragraphs,
including empty ones, without adding space after soft wraps or after the final
paragraph. `Text::paragraph_indent(pixels)` insets only each paragraph’s first
line; wrapping uses the reduced first-line width and full continuation widths.
Center/right alignment uses that line’s available width. Vertical alignment
includes paragraph gaps in the block height.

Styled runs can override either setting with the same builders; unset values
inherit the text layer. Runs within one paragraph, including its terminating
newline, must agree on paragraph settings. Conflicts fail closed. Spacing and
indentation must be finite and nonnegative, and indentation must leave positive
line width. Lists have their own spacing and nesting settings below; hanging
punctuation remains unsupported.

`text::layout_paragraphs(.., ParagraphStyle { spacing, indent })` exposes the same
layout to hosts. `RunLine` carries `indent`, `available_width` and `spacing_after`;
sum `height + spacing_after` for the block height. `layout_runs` uses zero layer
defaults while retaining run overrides. Cached rendering uploads nothing when
unchanged, and updates when paragraph settings change. No new dependencies.

`cargo run --example paragraphs --all-features` renders plugin-generated Rust
[beside Figma](docs/paragraphs-compare.png), mean 0.57/255 at 2×. The fixture covers
wrapping, empty paragraphs, centered alignment and different settings per
paragraph. Glyph edges still differ. Synthetic tests cover right/bottom alignment,
trailing breaks, conflicts, invalid values and cache invalidation.


## Text decorations and case

`Text` and `text::TextRun` accept `.text_decoration(TextDecoration::Underline)`
or `Strikethrough`, and `.text_case(TextCase::Upper)`, `Lower`, or `Title`.
Defaults are `None` and `Original`. On rich text, set these on each run.
Case resolves before measurement and wrapping without changing the source string;
title case capitalizes word initials while preserving remaining letters, with
word context retained across run boundaries. Simple Latin casing preserves `ß`
in uppercase; transformations outside the supported character/byte limits fail.

CoreText uses font metrics for solid, automatic-color/offset/thickness decorations,
matching the continuous automatic lines in the reference. Decorations share run color, transforms, clipping and
shadows. Custom underlines also support solid, dotted and wavy styles, pixel or
font-size percentage thickness/offset, solid color overrides, and skip-ink.
Use `.text_decoration_style(TextDecorationStyle::Wavy)`,
`.text_decoration_thickness(DecorationValue::Pixels(2.))`,
`.text_decoration_offset(DecorationValue::Percent(20.))`,
`.text_decoration_color(color)` and `.text_decoration_skip_ink(false)`.
These setters are on both `Text` and `TextRun`; types live in `text`. Automatic
color follows the run paints. Zero thickness hides the underline. Custom settings
require underline; custom strikethrough remains an error.

Custom underlines use separate contours behind the glyphs, so offsets can extend
outside the line box (explicit clipping still applies). Skip-ink subtracts gaps
where glyph ink intersects the decoration, using CoreGraphics on macOS 13+.
Older systems report unsupported skip-ink. Thickness is bounded to 128 logical
units and offsets to ±2048; excess geometry reports a renderer problem. Custom
backends must opt in with `validate_decoration` and return decoration contours
from `outline_runs`; bitmap-only calls reject custom decorations.
[Custom underline comparison](docs/custom-decorations-compare.png) comes from
`cargo run --example custom_decorations --all-features`, with 12 live Figma cases.
Small edge and skip-gap differences remain. No new dependencies. `cargo run --example decorations --all-features`
renders [generated code beside Figma](docs/decorations-compare.png); glyph and
antialiasing differences remain. Tests cover style boundaries, measurement,
wrapping, invalid transformed text and unchanged-canvas uploads.


## Ending truncation and maximum lines

Use `.text_truncation(text::TextTruncation::Ending)` on `Text`, optionally with
`.max_lines(n)`. Disabled truncation is the default; a stored line limit has no
effect until Ending is enabled. Limits must be positive integers up to 10 million.
The plugin emits these settings for fixed-height and maximum-line text layers,
including Figma’s legacy `TRUNCATE` auto-resize value, while retaining the source.

The final visible line is refitted from its remaining paragraph with `…`. The
ellipsis inherits the first omitted character’s font/color, without decoration.
Wrapping, paragraph gaps/indents, case and alignment use the resulting line
metrics. Height visibility follows line midpoints and always retains a first
line, matching the checked Figma cases; a tiny box can therefore overflow its
height or ellipsis width. Use an explicit clip when the host needs clipping.
`text::layout_text` exposes the same measured lines for host sizing.

[Generated Rust beside Figma](docs/truncation-compare.png) covers partial heights,
auto-height maximum lines, mixed styles, paragraph gaps and untruncated exact fits
(mean difference 1.31/255 at 2×; glyph edges still differ). Tests also cover UTF-8,
invalid limits, hidden missing fonts and unchanged uploads. Missing ellipsis glyphs
or exhausted text budgets report errors; there is no three-dot or bitmap fallback.


## Text fills and strokes

`Text::paints(vec![Paint::...])` replaces the default solid color with a paint
stack; `TextRun::paints` does the same for a rich-text range. Paints use the
whole text box, so gradients and images retain their coordinates across runs
and wrapped lines. Solid, linear/radial/angular/diamond gradient, image and
pattern paints reuse the shape renderer. An empty stack means no fill.

Use `.stroke(width, color, StrokeAlign::Inside/Center/Outside)` on the text
layer, optionally followed by `.stroke_paints(vec![...])` and
`.stroke_blend_modes(vec![...])`. Strokes cover the combined glyph silhouette
and follow fills. The host still supplies text and fonts, not exported outlines.
CoreText provides shaped contours through `Backend::outline_runs`; custom
backends must opt in or report unsupported. Call `resolve_runs` before requesting
outlines directly. Colored `raster_runs` rejects explicit paint stacks.

Basic decorations can share the paints. Existing shaping, fallback, wrapping,
truncation, transforms, clips and text shadows still apply. Outlines are split
only where coverage is disjoint; holes and translucent overlaps stay together.
There are no new dependencies. Native extraction is bounded at 65,536 commands
per line; each connected group retains the existing vector and GPU work limits.
An excessive group reports a renderer problem rather than approximating it.

The plugin emits these native text calls and reads mixed-range image assets
into memory. Dashed/brush text strokes, custom text stroke joins/miter limits,
blended fill stacks/ranges, and blended text fills combined with strokes remain
hard errors. The existing single-fill blend case remains supported.

`cargo run --example text_paints --all-features --locked` renders generated Rust
[beside Figma](docs/text-paints-compare.png): 14 editable text cases covering
holes, fill stacks, aligned/gradient/stacked strokes, underline, wrapped and
mixed-run gradients, image fills, all gradient types and translucent overlaps.
The 2× comparison was visually reviewed; mean channel difference is 1.05/255,
with differences at glyph edges and image sampling. This is not pixel parity.
Pattern paints and failure/cache behavior also use synthetic tests.

## Script-aware text and explicit fallback

CoreText shapes supported BMP scripts, including Greek/Cyrillic, Hebrew/Arabic,
Indic, Southeast Asian and CJK text. CoreFoundation supplies grapheme boundaries
and Unicode line-break opportunities, including dictionary breaks; emergency
wrapping and truncation keep clusters intact. Paragraph direction survives soft
wraps and mixed Latin/RTL lines. RTL ending ellipses retain the final wrapped word
and can hang beyond the box, as in the checked Figma export.

Use `CoreText::new(&sources)?.with_fallback_fonts(&[1, 2])?` for an ordered list of
host-supplied slots, or `.with_fallback_for(primary, &[...])?` for a face-specific
chain. Resolution keeps complete graphemes and joining-script words in one font,
before automatic line-height measurement. A missing primary font, exhausted chain
or run budget is an error. No system-font lookup, guessed weight or new dependency.
Custom backends opt into `text_boundaries` and `resolve_fonts`; Latin defaults
remain available. `layout_text` exposes the same shaped lines used by rendering.

The exporter retains Unicode and explicit font/style slots. Figma does not expose
its implicit glyph fallback chain, so generated comments require host provisioning
rather than inventing fallback faces. Emoji/color glyphs, variation selectors,
explicit bidi controls, non-Latin case conversion, and
font changes inside a grapheme remain hard errors. Scalar `caret_offsets` stays
Latin-only; this adds no input runtime. Existing byte/run/bitmap budgets still apply.

[Generated comparison](docs/scripts-compare.png) was reviewed beside Figma: Greek,
Cyrillic, Devanagari conjuncts, combining accents, Arabic with English/numbers,
RTL truncation and explicit mixed fonts. Mean difference is 1.53/255 at 2×; font
rasterization differs. CJK punctuation and Thai dictionary boundaries have tests,
not visual-parity evidence. Fallback matches explicitly assigned font pixels in
tests, including unchanged uploads and failed-chain caching. Pinned OFL Noto
reference fonts add 1.60 MB to tests/examples only (see `fonts/Noto.NOTICE.txt`);
default builds remain dependency-free and do not embed these fonts.


## Lists and OpenType styling

Use `.list_options(text::ListOptions::Unordered)` or `Ordered`, `.indentation(1..=5)`,
`.list_spacing(pixels)` and `.hanging_list(true)` on `Text` or each `TextRun`,
with `.wrap()` on the text layer. List options default to level 1. Paragraphs
retain their source strings; the layout result carries separate `ListMarker`s
and baseline offsets. Wrapped continuation lines have no marker. Ordered counters
cycle through decimal, alphabetic and Roman numbering by nesting level, resume
when returning to a parent, and restart after ordinary paragraphs. List spacing
replaces paragraph spacing between list items. Marker style and baseline come
from the list's initial style, matching the mixed-style reference. As in Figma,
plain paragraphs retain stored indentation levels without a visible list inset.

`.open_type_features(vec![text::OpenTypeFeature::new(*b"liga", false)])` sets
explicit boolean overrides; tags are lowercase OpenType tags. Features follow
runs through fallback, measurement, wrapping, truncation, bitmap and outline
rendering. `TextCase::SmallCaps` and `SmallCapsForced` request real `smcp` and
`smcp` + `c2sc` features. CoreText checks the supplied font's GSUB/GPOS (or kern)
tables; missing features report errors instead of substituting scaled capitals.
Custom backends must opt into `validate_open_type_features`.

Limits are 64 unique overrides and the existing 64 cached font-instance budget,
now including feature combinations. Vertical/random features and
explicitly enabled kerning with nonzero tracking remain hard errors. Default
kerning is retained at zero tracking; the previous zero-valued kern attribute
incorrectly disabled it. Other text/resource limits still apply.

`cargo run --example lists_opentype --all-features --locked` renders the
[12-case Figma comparison](docs/lists-opentype-compare.png): bullets, all five
numbering levels, hanging lists, spacing, double digits, mixed styles, truncation
and Source Serif 4 small caps. Reviewed at 2×; mean channel difference 0.56/255,
with edge and pixel-position differences remaining. Explicit feature on/off
coverage is synthetic (ligatures, metrics, outlines, cache invalidation and
unsupported fonts); Figma's plugin feature map is read-only. No new dependencies.
The bundled Source Serif 4 font adds 1,209,508 bytes to test/example assets only;
it is not embedded in the library (see `fonts/SourceSerif4.NOTICE.txt`).

## RTL lists, decoration and tracking

Arabic/Hebrew paragraphs retain CoreText bidi ordering, joining and tracking,
including mixed Latin/numeral runs. Tracking is removed at the visual line end;
decorations use shaped visual spans, so one logical style can decorate disjoint
pieces without drawing across intervening text. RTL underlines use outlines for
skip-ink (macOS 13+); bitmap-only callers must use `outline_runs` instead.

Lists mirror markers to the leading right edge and preserve RTL nesting, hanging
markers, paragraph indentation, wrapping and truncation. Alignment remains the
physical left/center/right setting. Fonts and variable axes are host-supplied;
the Hebrew example explicitly selects weight 400 and width 100.

`cargo run --example rtl_text --all-features --locked` renders the
[18-case Figma comparison](docs/rtl-text-compare.png). Arabic/Hebrew tracking,
underline/strike/waves, mixed styles, wrapped lists, nesting, hanging markers and
left alignment were inspected. Glyph/edge differences remain, including unexpected
extra counter glyphs in the Figma export; native counters use the supplied font.
Automatic decoration-color inheritance across mixed fills is not represented:
the plugin rejects that combination and requires explicit decoration colors.
Synthetic tests cover disjoint spans, retained insets, truncation and zero-upload
caching. The pinned OFL Hebrew font adds 112,640 test/example bytes; no dependency
or default-build size increase. Explicit bidi controls remain unsupported.
