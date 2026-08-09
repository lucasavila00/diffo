# codex-mock

`codex-mock` is a deterministic, offline stand-in for the narrow `codex exec`
contract used by Diffo's end-to-end tests. It validates the pinned model, read-only
sandbox, output-schema argument, fixed prompt, and supplied context before returning a
fixed structured commit subject, review map, or diff answer.

This binary is test infrastructure. Production Diffo resolves the real `codex` CLI;
E2E and stress builds enable Diffo's `codex-mock` Cargo feature so it resolves this
binary name instead.
