# figleaf

figleaf exists so a small native Rust UI can draw what its owner designed in
Figma by calling a library instead of hand-drawing it each time: shapes,
strokes, shadows and text with Figma's meaning, plus Figma's auto layout. An
agent reads the values from Figma and writes figleaf calls into the host.

## Keep it small

- It is a paint library and a layout solver. No importer, no file format, no
  scene or widget runtime, no input handling. The host owns state, events and
  its render pass. If a change needs one of those, stop and ask.
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

Keep README.md and TASKS.md current and short. Do not modify figui, Switchblade,
Abner or other host apps without a separate instruction.

Validate with:

    cargo fmt --all -- --check
    cargo test --all-features --locked
    cargo clippy --all-targets --all-features --locked -- -D warnings
    cargo test --locked
    cargo clippy --all-targets --locked -- -D warnings

`--all-features` needs macOS with clang. The GPU tests need a working adapter.
