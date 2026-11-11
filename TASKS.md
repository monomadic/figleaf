# Tasks

figleaf is useful when a real app draws a real Figma-designed control through
it and the result looks right beside the Figma export. Work on the first unmet
item unless the owner says otherwise.

## Where things stand (2026-11-12)

Works: rectangles, rounded rectangles, ellipses, solid fills, strokes in all
three alignments, drop and inner shadows, rectangular clips, single-line text
through CoreText, the auto layout solver with a builder API, and a wgpu
renderer that draws inside the host's pass. Checked by unit tests, GPU pixel
tests and one reviewed render of a made-up design ([docs/switch.png](docs/switch.png)).

Not checked: anything against Figma. These are assumptions until task 1:

- a shadow's Gaussian sigma is half of Figma's blur value;
- shadows follow the fill and stroke together, and inner shadows paint over the stroke;
- growing a shape (outside strokes, spread) keeps square corners square and
  adds to round ones;
- shadow blur is computed from the distance to the edge, which is exact for
  straight edges and approximate at corners and for shapes smaller than the blur.

## 1. One real control beside its Figma export

Take a control the owner designed (a switch with its shadows and label is the
obvious one), write it as figleaf calls from the inspector values, render it,
and look at it beside the Figma export. Fix what is visibly wrong and correct
the assumptions above. One screenshot and a short note.

## 2. Use it in a real host

Replace one hand-drawn control in one of the owner's apps (they are wgpu 30
hosts) with figleaf calls. Needs the owner's go-ahead for that repository. This
is where the API's rough edges show: surface formats, multisampled passes,
scale factors, when to rebuild the canvas.

## 3. Gradients and multiple fills

Linear gradients first, with Figma's handle positions and stops.

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
