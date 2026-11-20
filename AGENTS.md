# figleaf

figleaf exists so a small native Rust UI can draw what its owner designed in
Figma by calling a library instead of hand-drawing it each time: shapes,
strokes, shadows and text with Figma's meaning, plus Figma's auto layout. The
Figma plugin in `plugin/` shows Rust for a selection; an agent or the owner
pastes that into the host, or writes the same calls by hand from Figma's
values. README.md has the steps under "Using figleaf in an app".

## Keep it small

- It is a paint library, a layout solver, and a read-only Figma plugin that
  displays Rust calling them. The plugin is in scope: extend it together with
  the library. It only shows code; it writes no files and has no network
  access.
- No file format, no scene or widget runtime, no input handling. The host owns
  state, events and its render pass. If a change needs one of those, stop and
  ask.
- When putting a design into a host: use the exporter's output where it
  accepts the selection, and write calls by hand for small controls or for a
  piece it refuses. An agent cannot click the plugin in Figma Desktop; run the
  same exporter through `node plugin/cli.js` on a snapshot taken with Figma's
  MCP `use_figma` tool (plugin/README.md, "Exporting without the plugin"),
  or ask the owner to run the plugin and paste the result.
- `plugin/snapshot.js` (runs inside Figma) and `plugin/cli.js` share the
  plugin's exporter: never fork its logic into them. When the exporter starts
  reading a new node property, add it to `FIGLEAF_SNAPSHOT_PROPS`;
  `snapshot.test.js` fails until you do.
- Read TASKS.md and work on its first unmet task unless the user changes it.
- New API should read like Figma's inspector: same names, same numbers.
- Prefer the general mechanism over one that only fits the example.
- Default builds have no dependencies. Keep new ones optional and note their
  size cost in the README.

## Keep it honest

- "Looks right beside the Figma export" is the bar. Do not start pixel-exact
  or renderer research unless the user asks.
- Do not call something Figma-accurate unless it was looked at beside a Figma
  export. Tests built from documented behaviour are synthetic; say so.
- Anything that can't be drawn must show up in `Renderer::problems`. Never
  drop content silently.
- When a change is visual, run the example and look at the image.
- An unchanged canvas must upload nothing and report `changed: false`.

## Rules

Keep README.md and TASKS.md current and short. README.md is the guide to using
figleaf; per-feature detail goes in docs/REFERENCE.md and plugin/README.md. Do not modify figui, Switchblade,
Abner or other host apps without a separate instruction.

Validate with:

    cargo fmt --all -- --check
    cargo test --all-features --locked
    cargo clippy --all-targets --all-features --locked -- -D warnings
    cargo test --locked
    cargo clippy --all-targets --locked -- -D warnings
    node plugin/build.js --check
    node --test plugin/exporter.test.js plugin/snapshot.test.js

`--all-features` needs macOS with clang. The GPU tests need a working adapter.
