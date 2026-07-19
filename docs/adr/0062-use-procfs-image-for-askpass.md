# ADR 0062: Re-enter the running image through procfs for askpass

Status: Accepted

Refined by [ADR 0063](0063-real-loopback-askpass-e2e.md), which verifies this path
through real Git and OpenSSH processes.

Supersedes [ADR 0060](0060-lazy-askpass-image.md) and refines
[ADR 0053](0053-broker-git-interactions.md) and
[ADR 0056](0056-own-deferred-execution-dependencies.md).

## Context

Git and OpenSSH require askpass to be an executable pathname. Diffo ships one binary
and uses an environment marker to make that binary enter a small askpass path instead
of starting the TUI.

The installed pathname is not a stable reference to the launched image. A build or
upgrade can unlink it or replace it with a different Diffo version while the TUI is
still running. ADR 0056 fixed that race by copying the executable, and ADR 0060 moved
the copy from startup to the first prompted network operation. The copy is safe, but
it duplicates the full executable for a helper path that uses only a small part of it.

Diffo targets Linux only. Linux already exposes a lifetime-bearing pathname for the
exact executable image of a running process: `/proc/<pid>/exe`.

## Decision

When constructing the real Git repository source, record
`/proc/<diffo-pid>/exe` as the askpass executable. Give that path to Git and SSH for
every prompted Fetch, Pull, and Push.

Do not use `/proc/self/exe`. Git or SSH resolves the askpass pathname in its own
process, where `self` would identify Git or SSH rather than Diffo. Do not canonicalize
or read the procfs symlink: its value can describe the original mutable pathname or a
deleted file, while the procfs entry itself continues to identify the running image.

The running Diffo process owns the image and remains alive until its Git process group
and askpass children have been terminated and reaped. Replacing or unlinking the
original executable therefore cannot redirect or break a later askpass invocation.

Keep the existing internal environment marker, private Unix socket, prompt validation,
process-group cancellation, and shutdown ordering. Remove the retained executable file
descriptor, private temporary executable, lazy copy, and copy-state mutex.

Procfs availability is a runtime requirement. If Git or SSH cannot execute the procfs
path, report the operation failure at the askpass boundary. Do not fall back to the
installed pathname or silently select different bytes.

## Alternatives

- Keep the lazy full-binary copy. Rejected because Linux already provides a stable
  pathname for the owned running image.
- Embed a small askpass executable and extract it at runtime. Rejected because it adds
  a second build artifact and protocol version to the release pipeline, and Git and SSH
  would still require it to be materialized as an executable pathname.
- Use the installed executable pathname. Rejected because replacement can select a
  different version and unlinking can make the helper unavailable.
- Retain an executable file descriptor and use `/proc/<pid>/fd/<fd>`. Rejected because
  `/proc/<pid>/exe` expresses the required identity directly and needs no descriptor
  bookkeeping.

## Consequences

Diffo continues to ship and execute one binary. Normal startup and the first prompted
network operation perform no executable copy, create no askpass-image directory, and
consume no duplicate disk space. Askpass remains tied to the exact launched version.

The implementation is intentionally Linux-specific. Supporting another operating
system would require a new decision rather than restoring a mutable-path fallback.

## Verification

- A configured real Git source uses `/proc/<diffo-pid>/exe` for askpass.
- Start a copied Diffo binary, unlink and replace its launched pathname, then complete
  a prompted SSH Push. The replacement must not run and the Push must succeed.
- Prompt parsing, socket protocol, cancellation, and secret-handling tests continue to
  pass unchanged.
- `make all` passes.
