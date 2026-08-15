# Diffo

Diffo is a terminal UI for understanding and changing the state of your current
Git repository.

It brings working-tree changes, staged changes, recent commits, branches, and
common Git actions into one keyboard-driven interface.

Use it to review what changed, prepare commits, and move through everyday Git
work without leaving the terminal.

## Install on Ubuntu 24.04

Diffo currently publishes an x86_64 GNU/Linux executable. With `curl` installed,
run:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/lucasavila00/diffo/main/install.sh | sh
```

The [installer](install.sh) verifies the latest release against its published
SHA-256 checksum and installs it as `/usr/local/bin/diffo`. Rerun the same
command to replace an existing installation with the latest release. Run `diffo`
from inside a Git repository.

## AI commits

Diffo can generate and create a commit from staged changes with Codex. See
[AI commits](docs/architecture/ai-commits.md) for the supported provider and
model, data boundaries, safety checks, and offline testing contract.

## Architecture

The [architecture documentation](docs/architecture/) describes how Diffo works
now. The [architecture decision log](docs/adr/) preserves the context and
tradeoffs behind consequential decisions.
