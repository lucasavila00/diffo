# diffo-ui

`diffo-ui` contains small terminal UI primitives shared across Diffo activities.

It provides pane geometry, bounded scroll and scrollbar math, terminal-safe text,
fixed structural and interactive styles, a shared interaction-marker vocabulary,
and semantic layout tokens for widths, heights, insets, gaps, and overlay bounds.
Activity-specific state and rendering remain in their owning crates. Semantic
content, diff content, and syntax colors remain owned by their content renderers.
