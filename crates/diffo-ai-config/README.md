# diffo-ai-config

`diffo-ai-config` is the single source of truth for Diffo's compile-time AI
policy. It defines the supported provider, Codex executable names, selected
model, request limits, prompt, and response schema.

The crate contains policy constants only. It does not invoke Codex, parse
responses, expose runtime configuration, or own application behavior. Change
the model in `src/lib.rs`; production, unit tests, and `codex-mock` consume the
same value.
