# ADR 0111: Overlap snapshot metadata

Status: Accepted

Refines [ADR 0077](0077-visible-and-bounded-repository-startup.md).

## Context

After Git status, Diffo reads changed files and then reads commit and operation
metadata. These reads are independent, so the second group extends startup.

## Decision

Read files on the existing bounded pool while one additional worker reads metadata.
Join both before publishing the snapshot. Do not run watch-path discovery beside Git
status; measurements showed that made one-file startup much slower.

## Consequences

Snapshot startup has one more fixed worker, never another unbounded pool. Fast and
large repositories avoid serial metadata time while snapshot contents stay atomic.

## Verification

A deterministic test proves the two read groups overlap. Compare
`make measure-startup` before and after, then run `make all`.
