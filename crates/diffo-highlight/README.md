# diffo-highlight

`diffo-highlight` provides bounded syntax highlighting for Diffo.

It highlights visible code windows with bundled syntax definitions and exposes token
text with Monokai Extended foreground colors. Theme backgrounds and font attributes
are intentionally excluded so every code view has the same syntax style. Fixed line,
look-behind, and byte limits keep syntax work off the full-file critical path.
