Keep Diff and History as two activities with distinct data, controls, and
actions. They should share a core renderer that supports both unified hunk and
rich file projections.

Diff continues to review and stage mutable working-tree and index changes.
History remains read-only, with commits in the upper part of its leading pane
and the selected commit's changed files below. Both activities can show either
their full hunk review or a selected file through the shared renderer.
