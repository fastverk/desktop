# Managed local workspaces

`fvd` owns the `fastverk.workspace.v1.WorkspaceService` gRPC service. The
`fv-workspace` CLI uses its generated tonic client over the existing Unix socket.
The desktop Dashboard renders Projects, Workspaces, Workspace hygiene, and
Workspace operations from the same catalog. Existing `fastverk.v1.Fvd` repo and
worktree calls remain legacy-compatible; use the new service for managed tasks.

## Layout and ownership

`Config.repos_root` is the managed volume root (default `/Volumes/Workspace`):

```text
projects/<project-id>/project.pb
repos/<host>/<full-namespace>/<repo>/
workspaces/<workspace-id>/workspace.pb
workspaces/<workspace-id>/src/<host>/<full-namespace>/<repo>/
workspaces/<workspace-id>/run/
workspaces/<workspace-id>/out/
artifacts/<project-id>/<workspace-id>/
```

Register adopts an ordinary primary clone at its existing absolute location.
Clone creates canonical backing storage under `repos`. A project references repo
IDs and can cross organizations. Provision accepts an explicit subset of its
repositories, including none for non-code work. Each base ref resolves to a full
commit ID before any worktree is created. Task branches are
`fv/<workspace-id>/<repository-id>`; changing a label never renames paths.

No compatibility symlinks are required. Path components are validated; symlink
ancestors, ambiguous adoption, existing branch collisions, and unknown workspace
destinations are rejected. Git commands preserve the canonical checkout's branch
and local content. Acquisition of a workspace lease does not sandbox ordinary
editors: fencing protects service lifecycle operations, not arbitrary filesystem
writes. Use separate workspaces for concurrent writers.

The daemon journal is `<FASTVERK_CONFIG_DIR>/workspaces/catalog.pb` (the normal
platform config directory if unset), encoded as `WorkspaceSnapshot`. It is
protected by an OS advisory writer lock and atomically replaced with file and
parent-directory fsync. It includes pinned project specifications, operations,
leases, and removal intent. The writer lock covers lifecycle metadata operations;
independent builds do not take it. Lock contention is retryable. Journal schema
versions and changes to its managed root fail closed.

On-disk `.pb` descriptors are projections for inspection, not additional state
stores. `PutProject` explicitly updates desired state with an expected revision.
Workspace projections record materialization/removal checkpoints; read current
leases and phases through the service. Protobuf binary descriptors avoid a second
serialization implementation. Decode them using `protoc --decode` with the
checked-in schema, or use the CLI. Never edit `catalog.pb` directly.

## Build and try

From `fvkit`:

```sh
cargo build -p fvd --bins
# Or: bazel build //app/daemon:fvd //app/daemon:fv-workspace
```

Run `fvd` with the normal config, or isolate an experiment with
`FASTVERK_CONFIG_DIR`, `FASTVERK_SOCKET`, and a TOML `repos_root` pointing at an
existing scratch directory. Set `FASTVERK_MAINTAIN_INTERVAL_SECS=0` to disable
legacy scheduled maintenance in a scratch daemon. The real daemon and its
clients must use the same socket/config. These commands are examples; the service
does not bulk-import or relocate your current workspace on startup.

```sh
fv-workspace register adopt-desktop desktop github.com fastverk desktop /Volumes/Workspace/fastverk/desktop
fv-workspace project create-console console 0 86400 desktop
fv-workspace provision create-task task-1042 console desktop=origin/main
fv-workspace inspect task-1042
fv-workspace lease own-task task-1042 matt 2 0 3600
fv-workspace context task-1042
fv-workspace snapshot
fv-workspace watch
```

Alternatively, `clone REQUEST ID HOST NAMESPACE NAME CLONE_URL` creates a new
backing clone. URLs must exactly match the supplied identity in canonical HTTPS
or SSH form; credentials come from Git's normal helpers, never embedded tokens.
Interrupted clones remain visible with `ready=false`; retry the identical
request after inspecting a partial clone. The service never deletes a partial
clone to retry it.

Every mutation has a caller-chosen request ID. Identical completed retries return
the original result. Changed payloads under the same ID are rejected. Running or
blocked operations persist and can resume with the same request after a restart.
`Apply` is unary and waits for the attempt; `Watch` exposes intermediate journal
revisions while it runs. The operation list includes blocked details. Watch sends
a current snapshot on connection and replacements when the journal revision
changes; it is not an event-log replay. Consumers replace their view by stable ID.

For machine clients, import `proto/fastverk/workspace/v1/workspace.proto`.
`fv-workspace apply command.pb` accepts an encoded `WorkspaceCommand`; append
`--protobuf` to unary commands for an encoded response. Default CLI output is
human-readable diagnostic text. Dashboard JSON is a read-only presentation
projection following the existing app shim convention.

## Release and collection

A lease is acquired or renewed with an expected workspace revision, owner, and
fencing generation. TTL is 1–3600 seconds. An expired lease may be taken over with
the current revision/generation, which increments the generation. It never
silently releases the workspace or authorizes deletion.

Release requires a live lease and either explicit abandonment or an integration
target ref for every checkout. The service resolves target commits and verifies
that each workspace HEAD is an ancestor. This proves Git containment, not a remote
release or successful CI; squash/rebase merges without ancestry need explicit
human disposition. Branches are retained even after collection.

```sh
# Example after the task branch has been merged and origin/main fetched:
fv-workspace release ship-task task-1042 matt 3 1 desktop=origin/main
# Or explicitly abandon its committed work:
fv-workspace abandon abandon-task task-1042 matt 3 1 'experiment concluded'
# Inspect current revision and retention reasons, then collect when eligible:
fv-workspace inspect task-1042
fv-workspace collect collect-task task-1042 4
```

Collection requires explicit release, expired retention, current integration or
abandonment evidence, clean tracked/untracked content, no ignored files, no Git
worktree lock, and no active consumers observed by `lsof`. Missing/inconclusive
process observation blocks collection (install `lsof` on Linux). Runtime logs,
artifacts, unknown files, or links outside the registered checkouts are retained
for review. No generated-file allowlist is assumed. This is deliberately a
conservative first policy; it does not promise race-free protection from an
uncooperative external writer.

Each removal is journaled before `git worktree remove`, rechecked immediately
before execution, and checkpointed afterward. A crash after Git succeeds can
resume without treating missing owned content as an unmanaged deletion. There
is no `--force`, branch deletion, recursive directory removal, or broad pruning
of another task's registrations. Small descriptors/empty scaffolding remain as
tombstones. The legacy metadata-prune task is separate from source collection.

## Build and integration boundary

`ResolveBuildContext` returns stable project/workspace/repository IDs, the pinned
project-spec revision, observed source commits and dirty flags, validation
targets, a workspace-specific mutable Bazel output-base address under the cache
root, and an artifact address. It always sets `requires_input_snapshot=true`.
This observation is not an immutable snapshot, build run, or cache key. Concurrent
runs must allocate distinct execution contexts/output bases. The adapter must
snapshot declared inputs and account for toolchains/configuration before using
Tomato Bazel remote execution or content-based reuse.

This implementation provides adoption/cloning, project cataloging, multi-repo
provisioning, ownership, inspection, build-context resolution, verified release,
and conservative collection. Automatic PR submission/merging, build execution
and input snapshots, runtime port/database/container allocation, background
collection, and bulk path migration remain explicit future adapters. They are
not hidden side effects of registration, inspection, or daemon startup.

## Verification

Core tests use real temporary Git repositories for explicit base pins,
namespace collisions, dirty input reporting, idempotency, lease fencing,
interrupted multi-repo provisioning/removal, live process protection, ignored
content, integration ancestry, retention, and corrupt/locked journals. A daemon
test exercises the generated client through the real gateway, including
snapshot/watch/reconnect and durable mutation. Run:

```sh
cargo test -p fvkit-core -p fvd
bazel test //...
```
