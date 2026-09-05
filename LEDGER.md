# Ledger

Provenance for every fastverk desktop/runtime include, every optional later
row, every absorb, and every explicit exclude. This file is the source of
truth for what belongs in this git vehicle.

**Git repo ≠ Bazel module.** Importing a tree does not rename `module(name=...)`
and does not rewrite `module(version=...)`. SHAs below are the source default
branch (`main`) at ledger write / import time.

Status:

- `imported` — subdirectory present; SHA is the subtree-imported commit.
- `pending` — listed for a follow-up PR; SHA is source `main` HEAD when this
  row was written (or `—` when the source is not readable from this token).
  Do not pretend these are in the tree.
- `absorb` — must not appear as a module directory here; residue belongs with
  another imported module.
- `excluded` — must not appear as a module directory here.

Imported via `git subtree add` (no squash) from each source `main` SHA:

- Cluster 1: `fvkit`, `fastverk-app`.

Gateway/adapter implementations live in
[`fastverk/platform`](https://github.com/fastverk/platform). Public protos live
in [`fastverk/contracts`](https://github.com/fastverk/contracts)
(`fastverk_contracts`). This vehicle keeps the desktop/runtime trees (`fvkit`
library + `fvd` daemon, macOS app + credential helper). Do **not** rewrite
`module(name=...)` or `module(version=...)` in this PR.

## Includes (public desktop / runtime modules)

| Module | Status | Source repo | Source SHA | `module(name)` | `module(version)` | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| fvkit | imported | [fastverk/fvkit](https://github.com/fastverk/fvkit) | `ca638b99462a0c9433bfc9845832568044829d31` | fvkit | 0.0.8 | cluster 1; source CI is `bazel test //...` on Linux and macOS plus `connection.proto` schema parity with tomato-bazel/cred-helper; tags `v0.0.1`–`v0.0.9`; MODULE.bazel on HEAD is `0.0.8` (kept); registry.tbzl.dev has `fvkit` 0.0.1–0.0.9 |
| fastverk-app | imported | [fastverk/fastverk-app](https://github.com/fastverk/fastverk-app) | `985a7a0feee08a95e92331418a0c0f2eb28b6ed6` | fastverk-app | 0.0.2 | cluster 1; source CI is `bazel test //...` on macOS only (tao / tray-icon / eframe link macOS frameworks) plus Swift renderer builds (`//app/dashboard:fastverk-dashboard`, `//app/ios:FastverkConsole`); tags `v0.0.1`–`v0.0.5` and `ios-v0.0.1`–`ios-v0.0.7`; MODULE.bazel on HEAD is `0.0.2` (kept); registry.tbzl.dev has `fastverk-app` 0.0.1–0.0.2 |

### Both source repos are retired

Each source repo's default-branch HEAD equalled its Source SHA above at
retirement, so the imported trees and the sources were identical and no work
was stranded. Both now carry a README banner and a `retired` workflow that
fails a pull request touching anything but that banner. **This vehicle is the
edit surface.**

The remotes keep their history and every tag: published registry versions
resolve through the per-repo `vX.Y.Z` tags, and `git_override` pins still
point at them. They are deliberately **not** archived — that waits until this
vehicle publishes a release from its own `<module>/vX.Y.Z` tag, so no live
consumer resolves through a source remote. See
[Consolidation](https://docs.fastverk.com/consolidation.html).

⚠️ A third copy of `app/desktop`, `app/settings`, `tools/credhelper`, and
`tools/macos` still lives in `fastverk/fastverk`. Do **not** delete it on
sight and do **not** copy it here wholesale: that repo's `app/settings` is
*newer* than this vehicle's (it dropped the BuildBuddy provider on
2026-08-13; the copy here still offers it), while this vehicle's
`app/desktop` is far ahead of the meta-repo's. Reconciling them is a
macOS-verified merge, tracked in the consolidation runbook.

## Optional later

Not imported in this PR. No additional desktop/runtime modules are queued.

## Absorb

Never subtree these as a module directory in this vehicle. Residue belongs
with an imported module.

None. No residue from another imported module belongs here.

## Follow-up (not this PR)

- [ ] Publish new versions from this vehicle's `<module>/vX.Y.Z` tags via
      tomato-bazel/bazel-registry `rels`. Existing published versions keep
      resolving to the historical per-repo tags (`fastverk/fvkit` `v0.0.8`,
      `fastverk/fastverk-app` `v0.0.2`, etc.). Only **new** versions use this
      vehicle's tags.
- [ ] Source repos are not deleted or archived by this work.

## Follow-up import checklist

Unchecked rows are **not** in this tree. Import with `git subtree add` (no
squash) from the source default branch, then flip the row to `imported` and
set the SHA to the commit that landed.

- [x] fvkit
- [x] fastverk-app

## Excludes

Do not create these directories. Do not import them into this vehicle.

### Platform / gateways / adapters

| Name | Status | Why excluded |
| --- | --- | --- |
| platform | excluded | [fastverk/platform](https://github.com/fastverk/platform) is the gateway/adapter vehicle |
| forge | excluded | [fastverk/forge](https://github.com/fastverk/forge) — platform vehicle, not this repo |
| tracker | excluded | [fastverk/tracker](https://github.com/fastverk/tracker) — platform vehicle, not this repo |
| service-finder | excluded | [fastverk/service-finder](https://github.com/fastverk/service-finder) — platform vehicle, not this repo |
| wave | excluded | [fastverk/wave](https://github.com/fastverk/wave) — platform vehicle, not this repo |
| geetch | excluded | forge.v1 adapter; platform vehicle (pending there), not this repo |

### Control plane / shell / agents

| Name | Status | Why excluded |
| --- | --- | --- |
| botnoc | excluded | botnoc shell / control plane; out of this vehicle |
| deploy | excluded | deploy repo; out of this vehicle |
| agents | excluded | private agent fleet; out of this vehicle |
| agent | excluded | not a desktop/runtime module for this vehicle |

### Plugins

| Name | Status | Why excluded |
| --- | --- | --- |
| plugin-shell | excluded | console-plugin vehicle; not this repo |
| plugins | excluded | plugin collection; not this repo |
| plugin-planning | excluded | absorb into `wave` on the platform vehicle; not this repo |

### Contracts / spec

| Name | Status | Why excluded |
| --- | --- | --- |
| contracts | excluded | public protos live in [fastverk/contracts](https://github.com/fastverk/contracts) (`fastverk_contracts`) |
| spec | excluded | [fastverk/spec](https://github.com/fastverk/spec) is a separate corpus |

### Engines

| Name | Status | Why excluded |
| --- | --- | --- |
| mycelium | excluded | engine; not a desktop/runtime module |
| polyglot | excluded | engine; not a desktop/runtime module |
| agora | excluded | engine; not a desktop/runtime module |
| crank | excluded | engine; not a desktop/runtime module |

## Import method

For each imported row:

```sh
git subtree add --prefix=<dir> https://github.com/fastverk/<dir>.git main
```

No `--squash`. Source history is merged under the prefix; source remotes are
not rewritten. After the add, confirm `<dir>/MODULE.bazel` still declares
the same `name` and `version` as the source default branch (do not reset
versions to a vehicle-wide number). Directory names match the GitHub repo
(`fastverk-app`); Bazel `module(name)` matches in this vehicle (`fvkit`,
`fastverk-app`).

## Drift audit

[`drift.yml`](.github/workflows/drift.yml) runs daily (report-only, off the
PR path; `workflow_dispatch` available) and compares each imported row's
Source SHA to the source repo's default-branch HEAD. A drifted row is
fixed with a merge-commit subtree pull, then by updating that row's
Source SHA:

```sh
git subtree pull --prefix=<dir> https://github.com/fastverk/<dir>.git main
```

No `--squash` — squashing destroys the merge base for future subtree
pulls. Private/unreachable sources are warnings only.

**Now that both sources are retired, a drift report means something
different.** It used to mean *this vehicle is stale, pull from source*. It
now means *someone committed to a retired repo* — the `retired` check was
bypassed. Replay that commit here, then revert it at source. The audit is
unchanged; only the reading is. Treating a post-retirement drift report as a
routine subtree pull would make the retired repo authoritative again.
