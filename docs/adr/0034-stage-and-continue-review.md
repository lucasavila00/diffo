# ADR 0034: Stage and continue review

## Problem

The review loop is: inspect an unstaged diff, stage it, inspect another unstaged
diff. The loop breaks when staging completes without opening another file to
review.

This behavior belongs to the file action, not to the Space key. Space is only
the current binding for staging the open file.

## Options

### Reconcile selection after every refresh

Choose a nearby row after the repository changes. This is generic, but can open
a staged file and end the review loop.

### Keep the staged file open

This clearly shows what just moved, but leaves no unstaged diff ready for
review.

### Model a stage-and-review action

When staging the open unstaged file, remember another unstaged review target.
Open that target only after staging succeeds. Keep the current file when staging
fails.

This matches the review workflow, so we choose it.

## Decision

When Diffo opens a repository, open the first unstaged file. Open a staged file
only when no unstaged files exist.

Use an explicit file-action state:

```text
Idle
  └─ StageFileAction → StagingFile(next review target)

StagingFile
  ├─ success → open another unstaged file → Idle
  └─ failure → keep the current file → Idle
```

As long as unstaged files remain, a successful `StageFileAction` leaves another
unstaged diff open. The exact key binding and target-selection bookkeeping are
not part of this decision.

## Acceptance

Repeatedly run the stage action for the open file. Each success opens another
unstaged diff until none remain. A failed action does not advance.

Opening a repository with both staged and unstaged files starts on an unstaged
diff.
