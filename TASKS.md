# Tasks

figleaf is a small paint library and layout solver for native Rust interfaces
built from Figma designs. Work on the first unmet item below unless the owner
changes priorities. Extend the library/renderer and plugin together where applicable.
Completed implementation notes and comparisons are in the
[progress archive](docs/progress-archive.md); dated claims there are historical.

Exporting without the plugin (2026-11-20): `plugin/snapshot.js` snapshots a
hierarchy through Figma's MCP `use_figma` and `node plugin/cli.js export` runs
the plugin's exporter on it, so an agent can export without Figma Desktop.
Checked on Switchblade's Tag pill; image fills still need the plugin
([steps and limits](plugin/README.md#exporting-without-the-plugin)).

Placing an exported piece (2026-11-20): `Canvas::append_at(piece, offset)`
moves a whole canvas, clips, effects and masks included; checked pixel for
pixel against writing the same piece at the offset by hand, not against Figma.

## Ordered queue — 2026-11-20

1. **Text completeness — completed 2026-11-20 within the documented scope.**
   Unicode, mixed text, paragraphs, lists/OpenType, decorations, paints and strokes
   now have renderer and plugin output. See the [completion evidence and boundaries](docs/progress-archive.md#text-completeness--2026-11-20)
   and [final comparison](docs/text-completeness-compare.png). Unsupported color fonts,
   keycap sequences, layout controls and advanced font options still hard-error.

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
   **Next:** implicit border
   minimum-size expansion, rotated children and grid layout. Keep state and resize events
   in the host; add no scene runtime.
   **Done when:** the same generated panel lays out at several supplied sizes
   without re-export, agrees with corresponding Figma layouts, and reports
   unsupported layout semantics. Test text reflow and nested sizing dependencies.

3. **Live plugin validation.** Run the development plugin inside Figma Desktop
   against representative complete panels, including text, variables in different
   modes, images/patterns, masks, compositing and blur. Exercise selection/page
   changes, edits during generation, asset failures, diagnostics and copying code.
   **Done when:** copied Rust compiles and renders beside the selected designs;
   unsupported descendants produce layer-specific errors and no partial output;
   stale results clear correctly. Record actual live evidence separately from
   mocked UI tests. The plugin must still only display Rust and write no files.

4. **Renderer performance and host integration.** Measure realistic panels at
   multiple sizes/scales, both unchanged and changing text, geometry and effects.
   Profile full-target composite intermediates and nested filters before choosing
   optimizations. Document resource limits, backdrop ownership, font provisioning,
   render-pass integration and dependency/version usage. Add repeatable CI checks
   where the required platform/adapter is available.
   **Done when:** reproducible timings and allocation/upload counts establish
   practical limits, unchanged canvases upload nothing, and a small standalone
   host example demonstrates the integration. Changes to other apps need separate
   authorization; do not turn this into pixel-exactness research.

5. **Remaining paint and geometry exceptions.** Prioritize cases blocking real
   designs: branching/isolated stroke networks, rounded vector vertices,
   per-vertex joins and per-region paints; unequal dashed borders and remaining
   primitive effects; progressive/duplicate blurs and background blur on
   groups/text/lines; vector-mask sources requiring resolved outlines; image
   adjustments, pattern limitations and additional brush types.
   **Done when:** each accepted case has native rendering, plugin output, invalid
   input tests and a reviewed Figma comparison. Track completion by case rather
   than declaring all geometry finished. Keep resource caps unless evidence
   justifies changing them; do not replace unsupported content with bitmaps.

6. **Portability and output formats.** Add an optional non-CoreText text backend,
   wide-gamut/Display P3 handling and multisampled render targets as concrete
   consumers require them.
   **Done when:** each supported backend/format has integration tests and visual
   evidence; dependency and size costs are documented. Default builds stay
   dependency-free.

## Current support and boundaries

- **Paint API/renderer:** primitives, paths, transforms, clipping, masks,
  resolved smoothed/complex stroke geometry, multiple paints, gradients,
  images/patterns, flattened opacity, blend modes, shadows and normal blur.
  Support has explicit exceptions and resource limits; see the [reference](docs/REFERENCE.md).
- **Text/layout:** supported BMP scripts with explicit styled runs/fallback through CoreText, wrapping
  and text shadows;
  fixed/hug/fill auto layout with horizontal and vertical wrapping. Text measurement is
  supplied by the caller. Remaining text styling and layout semantics are above.
- **Plugin:** static selected-hierarchy Rust output, resolved variable values
  and embedded image data. Responsive selections include solver calls. Live bindings, interactions and
  component state remain unsupported. See [plugin limits](plugin/README.md).
- **Compositing:** the enclosing exported canvas supplies the backdrop. Existing
  host pixels and ancestors outside the selection are not implicitly included.
  Filtering/antialiasing remain approximate; supported does not mean pixel-identical.

## Validation still to close

Feature fixtures have generated-code compilation, synthetic tests and saved
Figma comparisons. These do not establish whole-panel coverage or live plugin
behavior. Live Desktop execution, variable-mode resolution and the full image/UI
lifecycle remain explicit checks under item 3. Unequal rounded borders and some
layout combinations still lack direct Figma comparisons. Interactive performance
and broader host configurations remain unmeasured under item 4.

For each implementation: compile generated Rust, exercise malformed/degenerate
inputs and unchanged uploads, run the visual example and inspect it beside Figma.
Anything undrawable must appear in `Renderer::problems`; the plugin emits no
partial Rust. Run the checks in [AGENTS.md](AGENTS.md), plus plugin tests and
bundle freshness checks when plugin code changes. Keep synthetic, Figma-reference
and live-app evidence clearly distinguished.

## Out of scope

No Figma file importer, package/file format, scene/widget runtime, input handling,
or live variable/theme runtime. The host owns state, events and its render pass.
No plugin filesystem writes, raster fallbacks or unrelated host-app changes.
