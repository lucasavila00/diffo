# ADR 0056: Own dependencies used after deferred execution

Status: Proposed

Refines [ADR 0053](0053-broker-git-interactions.md).

## Problem

Diffo can keep running after the executable from which it started has been replaced. This
is normal on Unix: the process retains the old executable inode while a build, install, or
update removes its directory entry and publishes a new file at the same path.

This happened in the development workspace. The running process reported
`target/debug/diffo` as its executable, `/proc/<pid>/exe` identified that image as
`(deleted)`, and the file currently at `target/debug/diffo` was a different inode.

Every Fetch, Pull, and Push currently resolves the running executable only when the
operation starts. It calls `current_exe()`, canonicalizes the returned path, and gives the
result to Git as `GIT_ASKPASS` and `SSH_ASKPASS`. After the original directory entry is
removed, canonicalization fails with `ENOENT`, displayed as `No such file or directory`.
If a replacement already exists, lookup can instead select a different version of Diffo
and create an askpass protocol mismatch. The latter is less visible and more dangerous
than a missing-file error.

The Git remote and local hooks are not the cause. Push exposes the problem because it
always prepares the askpass bridge, even when that particular remote does not ultimately
ask a question.

This is one instance of a broader time-of-check/time-of-use error:

```text
remember mutable locator -> resource is replaced or removed -> resolve locator later
```

A path names whatever occupies that path at lookup time. It neither retains the resource
that was originally found there nor proves that a later lookup has the same identity.
Existence checks and repeated canonicalization narrow no meaningful race.

## Decision

### Prepare an owned askpass image at startup

Acquire the running executable while startup still owns a valid reference to it. Copy
from that opened reference into a unique, mode-0700 runtime directory and publish a
mode-0700 private askpass image. The copy, not the install or build path, is the executable
given to Git and SSH.

Keep the runtime-directory guard alive until every Git command and askpass child has been
terminated and reaped. Do not rediscover, recanonicalize, or replace the image during a
network operation. Startup fails with dependency-specific context if the image cannot be
opened, materialized, protected, or executed; do not defer a generic operating-system
error until Push.

Copy rather than hard-link. A hard link survives rename and unlink, but it still shares
an inode that another process can modify in place. A private copy fixes both the bytes and
the pathname for the lifetime in which Git may invoke it.

This is a runtime copy of the one shipped Diffo binary, not a second installed program or
a user-configurable helper. It preserves ADR 0053's single-binary boundary.

### Treat locators and owned dependencies as different types

For any work that can be queued, delayed, retried, or run by a child process, distinguish:

- a locator, such as a path, command name, environment lookup, current directory, ref
  name, or socket address; and
- an owned dependency, such as an open file, immutable byte snapshot, temporary-directory
  guard, bound listener, child handle, or other lifetime-bearing lease.

Resolve mutable locators before the deferred boundary and put the owned dependency in the
operation context. Retain it until the last possible consumer has completed. When an
external API accepts only a pathname, materialize a private immutable resource and retain
the guard that keeps that pathname valid.

Do not use `exists()`, canonicalization, or preflight validation as a substitute for
ownership. Those checks can improve an error message but cannot guarantee later identity
or availability.

Apply this rule only where identity must remain stable across time. Repository paths and
Git refs that are intentionally meant to observe the latest state remain locators and
must not be snapshotted accidentally.

### Make failures identify the broken boundary

Errors must name the dependency and phase, for example:

- `failed to open the running Diffo executable`;
- `failed to prepare the private askpass executable`; or
- `Git could not execute the prepared askpass executable`.

Retain the underlying operating-system error as context. Do not expose credentials,
environment contents, or raw askpass prompts while adding that context.

## Verification

- Start Diffo, replace and unlink its original executable, then Push through an askpass
  transport. The prompt and Push must still complete through the owned image.
- Repeat with a different executable published at the original path. The replacement must
  never receive the askpass marker or socket path.
- Delay Git before its first askpass invocation and prove the private image remains present
  until Git and all helpers exit.
- Cover preparation failures, a non-executable runtime location, cancellation, and normal
  shutdown. Each failure must restore the terminal and report the failed dependency phase.
- Keep these as compiled-process tests. Unit tests of path strings cannot establish inode,
  replacement, execution, or lifetime behavior.
- For future deferred resources, add a deterministic test that invalidates or replaces the
  original locator between acquisition and use.

## Cost

Diffo copies its executable once per process and retains that private copy while running.
This adds startup I/O and temporary disk usage approximately equal to the binary size.
The copy is deliberately paid once so every later network operation has stable bytes,
identity, permissions, and lifetime.
