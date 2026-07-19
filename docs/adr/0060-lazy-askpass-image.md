# ADR 0060: Materialize the owned askpass image lazily

Status: Superseded by [ADR 0062](0062-use-procfs-image-for-askpass.md)

Refines [ADR 0056](0056-own-deferred-execution-dependencies.md).

## Problem

ADR 0056 made askpass safe across binary replacement by copying the running Diffo
executable into a private directory at startup. The copy is about 57 MB in a debug
build. Startup also called `fsync` before showing the first frame.

This makes every launch pay for a helper that is used only by a prompted Fetch, Pull,
or Push. Concurrent pseudo-terminal tests multiply the cost: each launched process
copies and syncs the full binary before drawing. Filesystem journal waits can exceed
the five-second readiness contract even though application state is ready.

Resolving the executable path later is still unsafe. The installed or built binary
may be replaced while Diffo is running. A delayed lookup could select different
bytes, fail because the old path was removed, or run a helper with an incompatible
protocol.

## Decision

Open the running executable at startup and retain that file descriptor in
`GitRepositorySource`. Opening the file acquires the original inode without copying
its bytes. Path replacement or unlink after startup does not change what the open
descriptor reads.

Do not create a private directory or copy bytes during normal startup. On the first
network operation that has an askpass prompt context:

1. Lock the owned askpass state.
2. Rewind the retained executable.
3. Copy it into a unique mode-0700 temporary directory.
4. Close the new file and rename it from a temporary name to the final mode-0700
   askpass path.
5. Retain the directory guard and reuse the path for later operations.

The repository command lane is already outside terminal input and rendering. The
first network operation may wait for this one-time copy without delaying the first
frame or blocking terminal input.

Do not call `fsync` for the private image. Diffo needs a complete file visible to the
current operating system, not crash recovery after power loss. Direct file writes,
close, and rename provide that process-lifetime publication. Git is started only
after the rename succeeds.

Keep the state behind a mutex. Concurrent requests must produce at most one retained
image. If materialization fails, keep the captured startup file so a later operation
can retry. Report the failure as an askpass preparation failure for that network
operation.

## Security boundary

This changes when bytes are copied, not which bytes are trusted. The copy always
comes from the file descriptor opened at startup. It never reopens `current_exe`, the
installed path, or a replacement binary.

The private directory and executable remain mode 0700. The image remains alive until
the repository source is dropped, after Git, SSH, and the askpass bridge have stopped.
No helper path or behavior becomes configurable.

## Consequences

- Normal startup performs one executable open instead of a full copy and sync.
- Launches that never run a prompted network operation create no askpass image.
- The first prompted network operation pays the one-time copy cost on the background
  repository lane.
- Later network operations reuse the prepared image.
- Startup no longer fails because a temporary runtime directory cannot be created.
  That error is reported when the feature is first needed.
- The retained file descriptor remains open for the process lifetime when askpass is
  enabled.

## Alternatives

- Keep eager copying. Rejected because it delays every launch for an optional
  operation.
- Resolve `current_exe` when a network command starts. Rejected because it loses the
  startup image identity guaranteed by ADR 0056.
- Copy in an untracked background thread after startup. Rejected because it adds
  lifecycle and shutdown work even when askpass is never used.
- Ship a second small helper binary. Rejected because Diffo intentionally keeps one
  shipped executable and one private protocol entry point.
- Keep `fsync` during lazy publication. Rejected because crash durability is not a
  requirement for a process-owned temporary executable.

## Acceptance

- Constructing the real Git source opens the running executable but creates no
  `diffo-askpass-image-*` directory.
- A normal launch can draw or dump its first repository state without copying the
  executable.
- The first prompted network operation creates one private executable; later
  operations reuse it.
- Replacing or unlinking the launched binary before the first Push cannot redirect or
  break askpass. The prompt and Push still use the startup image.
- Preparation failures identify the askpass boundary and do not expose credentials.
- `make all` passes, including the compiled-process binary-replacement regression.
