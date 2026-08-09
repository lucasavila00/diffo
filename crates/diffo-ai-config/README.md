# diffo-ai-config

`diffo-ai-config` is the single source of truth for Diffo's compile-time AI
policy. It defines the supported provider, Codex executable names, selected
model, request and Review context limits, prompts, and response schemas for AI
commits and Review.

Its `codex-mock` feature selects the fixed offline test executable; production
builds select the real Codex executable.

The crate contains policy constants only. It does not invoke Codex, parse
responses, expose runtime configuration, or own application behavior. Change
`AI_MODEL` in `src/lib.rs`; production, unit tests, and `codex-mock` consume the
same value.
