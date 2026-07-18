# diffo-app

`diffo-app` contains Diffo's application state and pure update logic.

The crate defines the model, messages, and effects used by the Diff activity. State
transitions stay independent of terminal rendering and repository I/O so they can be
tested deterministically.
