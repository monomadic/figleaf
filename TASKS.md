# Tasks

figleaf is a small paint library and layout solver for native Rust interfaces
built from Figma designs. Work on the first unmet item below unless the owner
changes priorities. Extend the library/renderer and plugin together where applicable.
Completed implementation notes and comparisons are in the
[progress archive](docs/progress-archive.md); dated claims there are historical.

## Ordered queue — 2026-11-17

1. **Text completeness — in progress.** Mixed-style runs with explicit fonts,
   colors, sizes and tracking are done (2026-11-17); generated output was
   [reviewed beside Figma](docs/text-runs-compare.png). Automatic line height is
   done (2026-11-18), with [font-derived rendering and plugin output](docs/auto-height-compare.png).
   Paragraph spacing and first-line indentation are done (2026-11-18),
   [including per-paragraph settings](docs/paragraphs-compare.png).
   Basic underline/strikethrough and upper/lower/title case are done (2026-11-18),
   [including mixed styles](docs/decorations-compare.png).
   Ending truncation and maximum line counts are done (2026-11-18),
   [including partial heights and mixed styles](docs/truncation-compare.png).
   Script-aware shaping, explicit fallback and Unicode line breaking are done
   for the [documented BMP profile](README.md#script-aware-text-and-explicit-fallback)
   (2026-11-18), with [representative script comparisons](docs/scripts-compare.png).
   Richer text fills and aligned/stacked strokes are done (2026-11-19),
   with [mixed paints and silhouette comparisons](docs/text-paints-compare.png).
   Lists, OpenType overrides and real small caps are done (2026-11-19), with
   [list and small-caps comparisons](docs/lists-opentype-compare.png).
   **Next: custom decorations**, then RTL lists/decoration/tracking, broader
   Unicode coverage, text paint blends and advanced stroke styles. Vertical/random
   font features and explicit kerning with tracking remain unsupported.
   Preserve hard errors until each case is implemented.
   **Done when:** generated Rust retains supported run styles and paragraph
   behavior; measured and rendered text agree; representative labels, paragraphs,
   mixed runs and non-Latin samples are reviewed beside Figma. Test wrapping,
   truncation, missing fonts, invalid runs, and unchanged-canvas uploads.

2. **Responsive layout export and solver gaps.** Export Figma auto layout as
   calls to the existing solver instead of only fixed drawing coordinates.
   Start with nested horizontal/vertical fixed, hug and fill sizing, min/max,
   padding, gaps, and absolute children. Connect text measurement to width-dependent
   layout, then cover wrapping, constraints, baseline alignment, wrapped-line
   distribution, rotated children and grid layout. Keep state and resize events
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
  Support has explicit exceptions and resource limits; see [README](README.md).
- **Text/layout:** supported BMP scripts with explicit styled runs/fallback through CoreText, wrapping
  and text shadows;
  fixed/hug/fill auto layout with horizontal wrapping. Text measurement is
  supplied by the caller. Remaining text styling and layout semantics are above.
- **Plugin:** static selected-hierarchy Rust output, resolved variable values
  and embedded image data. It does not yet export responsive solver calls,
  live bindings, interactions or component state. See [plugin limits](plugin/README.md).
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
