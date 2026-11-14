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
`Canvas`. It captures the current dimensions and explicit values, with the
selected root at `(0, 0)`. Auto layout and constraints are already resolved by
Figma: this is a static paint export, not a responsive layout generator. Ancestor
clipping/effects outside the selected root, interactions, component state, and
variable bindings are not exported. Bindings produce errors; hidden layers,
paints, and effects are intentionally excluded.

Every visible descendant is validated. Any error suppresses **all** Rust and
identifies its layer path. Selection or page changes clear old output. No image
fallbacks, vector flattening, border rectangles, or partial code are emitted.
An agent can consume the displayed code just as a person can; this plugin does
not install an agent connection or write to a consumer's project.

Supported: translation-only rectangles, frames/components/instances/groups,
full ellipses, filled vector paths, round-capped straight lines, solid and linear-gradient fills
(up to eight stops), solid uniform/individual borders, and shape shadows.
Rectangular clips can intersect; one rounded clip is supported for shapes.
Simple Latin text needs one solid fill, one style, explicit line height, and
caller-supplied font indices. Code comments identify each required family/style;
the host must supply the exact font and variable-font configuration. Normal
figleaf text limits still apply (including a maximum 2048-unit raster width).

Vector output preserves M/L/Q/C/Z commands, subpaths, nonzero/even-odd holes,
and explicitly unfilled branches. It reads Figma’s resolved fill geometry, with
no topology reconstruction. Regions share the node’s paints and are unioned
before filling; differing per-region paints are rejected. The limits are 256
regions and 4,096 commands; scale-dependent renderer limits also apply.

Errors include vector strokes/effects/paint overrides, images, masks, layer opacity, blend modes, corner
smoothing, rotation/scale/skew (except rotation of a line), unsupported gradients,
multiple/dashed/gradient strokes, non-miter joins, blur, container effects,
variable bindings, Display P3 documents, mixed/automatic-height/decorated/list
text, rounded text clipping, and intersecting rounded clips. Unequal corner radii
are supported on shapes, but not clipping frames. The size limit is 5,000 nodes
and 128 levels. New Figma features may need additional validation rules.

Generated code targets figleaf's supported semantics, not pixel identity.
Existing antialiasing, shadow, text, and sRGB-target blending differences remain.
Use an encoded-sRGB `Rgba8Unorm`/`Bgra8Unorm` target for the examples' blending.

## Validation

```
node plugin/build.js --check
node --test plugin/exporter.test.js
```

Tests compile and execute generated Rust, check diagnostics, clipping, escaping,
font contracts, read-only plugin messages, stale-result clearing, and UI states.
Tests include synthetic Plugin API objects and a real Figma person-icon fixture.
The exporter was also exercised through the Figma Plugin API: the original
variable-bound vector failed as intended, and an in-memory fixture with its
explicit resolved paint generated Rust. The complete panel has not yet been
exercised inside Figma Desktop. A browser policy blocked the local HTML visual preview.
