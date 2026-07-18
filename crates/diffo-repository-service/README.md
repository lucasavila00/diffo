# diffo-repository-service

`diffo-repository-service` provides Diffo's serialized background repository lane.

It converts optional filesystem notifications into refresh requests, collects full
snapshots, and executes the one identified command dispatched by the workbench. One
worker serializes refreshes and mutations so snapshots cannot race repository
commands. Typed askpass prompts carry the active command identifier and answers bypass
the blocked worker lane. Queue lifecycle, presentation, and committed repository state
stay outside this crate.
