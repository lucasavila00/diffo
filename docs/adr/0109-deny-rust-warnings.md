# ADR 0109: Deny Rust warnings

## Context

The workspace's Clippy step rejects warnings, but ordinary Cargo builds can
still succeed with compiler warnings. As a result, `make diffo` can report a
warning that CI's earlier build and test steps allow.

Warnings are easiest to fix when introduced. Allowing them to accumulate also
makes new diagnostics harder to notice and lets local and CI behavior diverge.

## Decision

Set the Rust `warnings` lint to `deny` in the workspace lint configuration.
Every workspace package inherits that configuration, so compiler warnings fail
ordinary builds, tests, documentation builds, Clippy, `make diffo`, and the
Cargo commands run by CI.

Keep lint exceptions narrow and explicit when a warning is intentional. Do not
use a workspace-wide allowance to bypass the gate.

## Consequences

Any first-party Rust warning is a build failure locally and in CI. Developers
must fix a warning or justify it at the smallest relevant scope before the
workspace can pass validation. Dependency warnings remain governed by Cargo's
dependency lint capping and are outside this workspace policy.
