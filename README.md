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
curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/lucasavila00/diffo/main/install.sh | sudo sh
```

The [installer](install.sh) verifies the latest release against its published
SHA-256 checksum and installs it as `/usr/local/bin/diffo`. Run `diffo` from
inside a Git repository.
