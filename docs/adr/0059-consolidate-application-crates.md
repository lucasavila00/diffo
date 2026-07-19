# ADR 0059: Consolidate application crates

Status: Accepted

Refines [ADR 0028](0028-split-diffo-app-model.md),
[ADR 0030](0030-split-diffo-tui-input.md),
[ADR 0039](0039-independent-app-modes.md),
[ADR 0043](0043-shared-text-buffer-view.md),
[ADR 0049](0049-shared-file-picker.md), and
[ADR 0051](0051-crate-documentation.md).

## Problem

Diffo has one product but sixteen workspace packages. Several packages contain one
small terminal component. Other packages split one activity's state from its input
and rendering. Those boundaries require public APIs, manifests, dependency entries,
and crate documentation without creating independent products or system boundaries.

The code still needs strong internal separation. Repository I/O must not enter state
updates. Rendering must consume committed state. Input, rendering, preparation, and
external work must remain independently testable. A crate is not required to enforce
each of these rules; private modules can do so with less workspace overhead.

## Decision

Reduce the workspace from sixteen packages to ten. Keep crates around system
boundaries, independently useful processing, and separate developer programs.
Consolidate product composition into `diffo-app` and shared terminal components into
`diffo-ui`.

| Current crate | Destination |
| --- | --- |
| `diffo-command` | `diffo-ui::command_palette` |
| `diffo-file-picker` | `diffo-ui::file_picker` |
| `diffo-text-view` | `diffo-ui::text_view` |
| `diffo-tui` | `diffo-app::diff` |
| `diffo-explorer` | `diffo-app::explorer` |
| `diffo-workbench` | `diffo-app::workbench` |

Keep these packages:

- `diffo`: terminal startup, event loop, runtime wiring, and shutdown.
- `diffo-app`: activities, application state, rendering, and workbench composition.
- `diffo-ui`: reusable terminal components and fixed visual primitives.
- `diffo-core`: repository data and repository-source interfaces.
- `diffo-git`: real Git and askpass implementation.
- `diffo-repository-service`: serialized repository refresh and command lane.
- `diffo-diff`: patch parsing and diff projections.
- `diffo-highlight`: bounded syntax preparation.
- `diffo-e2e`: reusable pseudo-terminal test support.
- `diffo-measure`: performance measurement program.

Do not keep compatibility shim crates for removed packages. Nothing outside this
workspace is a supported consumer. Move callers to the new module paths, then remove
the old package from the workspace and workspace dependencies.

## Module layout

The trees below define ownership. They do not require one file per leaf. A small
module may stay in one file. Split it into a directory only when that makes the code
easier to navigate.

### `diffo-ui`

`diffo-ui` contains terminal components that are shared by activities. It does not
choose application commands or perform repository work.

```text
diffo-ui
├── theme              fixed semantic colors and styles
├── design             fixed sizes, insets, and layout tokens
├── interaction        shared markers and hit vocabulary
├── terminal_text      terminal-safe text conversion
├── pane               pane split state, layout, and dragging
├── scroll             bounded scroll and scrollbar math
├── command_palette    palette state, input, layout, and rendering
├── file_picker        flat/tree state, input, actions, and rendering
└── text_view          viewport state, scroll mapping, and rendering
```

Each component owns its state transitions and rendering helpers. Component-specific
tests stay beside that component. Shared primitives must not depend on an activity or
the workbench. Callers still own the meaning of command execution, file activation,
and loaded text.

### `diffo-app`

`diffo-app` contains the long-lived activities and the shell that composes them.
Activities remain independent as required by ADR 0039. Moving them into one crate
does not create one shared model.

```text
diffo-app
├── diff
│   ├── model          pure Diff state, messages, effects, and updates
│   ├── input          fixed keyboard and mouse event mapping
│   ├── prepare        background buffers, syntax coverage, and atomic commits
│   └── view           diff, file panel, overlays, geometry, and styles
├── explorer
│   ├── model          tree, selection, and committed file state
│   ├── input          Explorer keyboard and mouse routing
│   ├── worker         file loading and bounded syntax preparation
│   └── view           tree and file rendering
└── workbench
    ├── activity_bar   activity selection and layout
    ├── command_queue  FIFO command lifecycle and cancellation
    ├── prompt         operation prompt state, input, and rendering
    ├── pending_scroll frame-bounded Diff scroll coalescing
    └── routing        global input, activity dispatch, progress, and results
```

The Diff model remains independent of terminal types and repository I/O. Its tests
stay under `diff::model`. Preparation can use background workers, but results enter
visible state only during frame preparation. Views read committed state only.

Explorer keeps its own model, worker, input, and view. Workbench owns activity
selection and cross-activity command lifecycle. Activities do not reach into each
other's state. Cross-module items are private by default and `pub(crate)` when one
application module must use them. `diffo-app` exposes only the small runtime-facing
surface needed by the executable.

### Unchanged crates

The surviving non-UI crates keep their current internal module boundaries. Do not
move Git execution, repository serialization, diff parsing, or syntax engines into
`diffo-app`. These are real process, concurrency, or processing boundaries and are
useful without terminal screen ownership.

The intended dependency direction is:

```text
diffo -> diffo-app -> diffo-ui
  |          |          |----> diffo-highlight ----> diffo-diff
  |          |          `----> diffo-core
  |          |----> diffo-diff
  |          |----> diffo-highlight
  |          `----> diffo-core
  |----> diffo-git -----------------> diffo-core
  `----> diffo-repository-service --> diffo-core
```

No dependency may point from `diffo-core`, `diffo-git`, or
`diffo-repository-service` into the application or UI crates.

## Migration

Make the change in behavior-neutral stages:

1. Move command palette, file picker, and text view into `diffo-ui`. Preserve their
   tests and public behavior. Remove the three old crates.
2. Move the existing `diffo-app` model and `diffo-tui` implementation under
   `diffo-app::diff`. Preserve the pure model boundary and atomic preparation tests.
   Remove `diffo-tui`.
3. Move Explorer and Workbench under `diffo-app`. Preserve activity ownership and
   command routing. Remove `diffo-explorer` and `diffo-workbench`.
4. Remove obsolete workspace dependency entries. Update `ARCH.md`, root and crate
   READMEs, package metadata, rustdoc includes, and ADR links that name a removed
   crate as a current owner.

Run `make all` after every stage. It covers unit, real-Git, pseudo-terminal,
formatting, and lint checks. Also run `cargo doc --workspace --no-deps` after the
crate documentation changes required by ADR 0051.

## Consequences

- The workspace has fewer manifests, crate READMEs, dependency edges, and public
  APIs.
- A change to one activity can update its state and rendering without coordinated
  crate releases or re-exports.
- Private and `pub(crate)` APIs replace interfaces that were public only because of
  crate boundaries.
- Module ownership and tests preserve separation between state, input, rendering,
  preparation, and I/O.
- `diffo-app` becomes a larger crate. Its top-level activity modules make that size
  explicit and keep unrelated state separate.
- A change inside a consolidated crate may compile more code. This is accepted in
  exchange for a simpler dependency graph and easier navigation.

## Alternatives

- Keep all sixteen packages. Rejected because the smallest boundaries add more
  maintenance than isolation.
- Merge everything into the binary. Rejected because Git, repository concurrency,
  parsing, syntax work, and developer programs are meaningful independent
  boundaries.
- Merge only the three smallest crates. Rejected because the artificial split
  between Diff state, Diff rendering, Explorer, and their workbench would remain.
- Create a generic activity framework while merging. Rejected because the merge is
  packaging work. Existing concrete activity routing is sufficient.

## Acceptance

- The workspace contains the ten packages listed in this ADR.
- Removed crate names appear only in history, migration documentation, and ADRs.
- Diff and Explorer retain separate model, input, worker or preparation, and view
  modules.
- Repository I/O remains outside application state and rendering modules.
- Existing fixed controls and observable behavior do not change.
- `make all` passes after the final merge.
