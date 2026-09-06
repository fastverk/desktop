# fastverk/desktop

This repository is a **git / CI / release vehicle** for the fastverk
desktop/runtime Bazel modules (`fvkit`, `fastverk-app`). It is **not** a
Bazel module.

**Git repo ≠ Bazel module.** Each subdirectory is its own module, with its own
`MODULE.bazel`, its own version, and its own tests. Consumers keep writing:

```python
bazel_dep(name = "fvkit", version = "0.0.8")
bazel_dep(name = "fastverk-app", version = "0.0.6")
```

Module names and versions are **not** lockstepped. A change that ships
`fvkit` 0.0.9 does not bump `fastverk-app`. There is no repo-wide `0.0.1`.

Published module identity lives in each subdirectory's `MODULE.bazel`
(`module(name = ..., version = ...)`). This git repo only groups those trees,
runs path-filtered CI, and is the place tags are cut from.

Gateway/adapter implementations live in
[`fastverk/platform`](https://github.com/fastverk/platform). Public gRPC
contracts live in [`fastverk/contracts`](https://github.com/fastverk/contracts)
(`fastverk_contracts`). Desktop/runtime stays here.

## Layout

```
desktop/
  README.md                 # this file — vehicle, not a module
  LEDGER.md                 # every include / optional / absorb / exclude row
  RECONCILE.md              # macOS meta-repo vs this vehicle (file inventory)
  .github/workflows/ci.yml  # one path-filtered workflow
  .github/workflows/release.yml  # macOS .dmg on `fastverk-app/v*` tags
  tools/ci/                 # affected-module detection + ledger check
  tools/ledger-check.sh     # CI entrypoint: LEDGER ↔ dirs ↔ MODULE.bazel
  tools/changed-modules.sh  # CI entrypoint: path → imported modules
  fvkit/                    # module(name = "fvkit", version = "0.0.8")
  fastverk-app/             # module(name = "fastverk-app", version = "0.0.6")
```

One subdirectory per module. Each imported tree keeps the source repo's
`MODULE.bazel` pins, license, and tests. See [LEDGER.md](LEDGER.md) for which
modules are in the tree today and which are follow-up imports.

Cluster 1 is imported (`fvkit`, `fastverk-app`), and **both source repos are
now retired: this vehicle is the edit surface.** They are not deleted or
archived — they keep their history and tags so published registry versions
and `git_override` pins keep resolving. See
[Consolidation](https://docs.fastverk.com/consolidation.html).

## Tags

Tags are **per module**, never repo-wide:

```
<module>/vX.Y.Z
```

Examples: `fvkit/v0.0.9`, `fastverk-app/v0.0.6`.

Do not tag `v0.0.1` (or any other version) at the repository root. That would
imply a lockstep bump of every module.

GitHub's archive for a slash tag on this repo unpacks as
`desktop-<module>-vX.Y.Z/<module>/`. That directory is the module root a
registry release must `strip_prefix` to.

## How to cut a release for one module

1. Change only that module's subdirectory. Leave other `MODULE.bazel` versions
   alone.
2. Bump **that** module's `module(version = ...)` and its `CHANGELOG.md` when
   the source tree has one.
3. Merge to this repo's default branch.
4. Tag the merge commit:

   ```sh
   git tag fvkit/v0.0.9
   git push origin fvkit/v0.0.9
   ```

5. Publish the registry entry from a bazel-registry checkout (same `rels`
   `--workspaces-root` / `--tag-prefix '<module>/v'` / `--strip-prefix`
   pattern as [tomato-bazel/rules](https://github.com/tomato-bazel/rules)).
   Existing published versions keep resolving to the historical per-repo tags
   (`fastverk/fvkit` `v0.0.8`, etc.). Only **new** versions use this vehicle's
   tags.

## How to cut a macOS .dmg

The installable app is a `fastverk-app` release, not a repo-wide tag.

1. Bump `fastverk-app/MODULE.bazel` `module(version = ...)` and the matching
   [LEDGER.md](LEDGER.md) row. Keep `fvkit` alone unless that module changed.
2. Merge to this repo's default branch.
3. Tag the merge commit and push:

   ```sh
   git tag fastverk-app/v0.0.6
   git push origin fastverk-app/v0.0.6
   ```

4. [`.github/workflows/release.yml`](.github/workflows/release.yml) builds
   `//tools/macos:fastverk_app` on `macos-latest`, attaches
   `fastverk-vX.Y.Z.dmg` to a GitHub Release, and best-effort publishes to
   `https://static.fastverk.com/apps/`.
5. Install:

   ```sh
   bash fastverk-app/tools/macos/install.sh
   # or: bash fastverk-app/tools/macos/install.sh fastverk-app/v0.0.6
   ```

Do not tag `v0.0.6` at the repository root. The nested
`fastverk-app/.github/workflows/release.yml` does not run from this vehicle.

## Path-filtered CI

There is one workflow: [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

A change under `fvkit/` runs that module's tests, not the whole tree. The
detector is [`tools/ci/affected.py`](tools/ci/affected.py) (also
[`tools/changed-modules.sh`](tools/changed-modules.sh)): it diffs against the
PR base (or the push before-SHA) and maps paths to immediate children that
contain `MODULE.bazel`.

| Change | What runs |
| --- | --- |
| `fvkit/**` | `fvkit` only |
| `fvkit/**` and `fastverk-app/**` | those two modules |
| `.github/workflows/ci.yml` or `tools/ci/**` | every imported module |
| `README.md` / `LEDGER.md` only | ledger check, no module test matrix |

Per-module commands reuse each source repo's existing Bazel/Rust gate rather
than inventing a new stack. Overrides live in
[`tools/ci/modules.json`](tools/ci/modules.json):

| Module | Linux | macOS | Extra |
| --- | --- | --- | --- |
| `fvkit` | `bazel test //...` | `bazel test //...` | source `connection.proto` schema parity with tomato-bazel/cred-helper on Linux |
| `fastverk-app` | — (source does not; tao / tray-icon / eframe link macOS frameworks) | `bazel test //...` | source Swift renderer: `//app/dashboard:fastverk-dashboard` and `//app/ios:FastverkConsole` |

A module with no test targets (`bazel test` exit 4) falls back to
`bazel build //...`. Buildifier runs as a warning so this PR does not rewrite
imported Starlark.

[`tools/ledger-check.sh`](tools/ledger-check.sh) fails CI if `LEDGER.md`,
on-disk module directories, and each `MODULE.bazel` `name`/`version` disagree.

## Provenance

Imports use `git subtree add` (no `--squash`) from each source repo's default
branch. Source history is not rewritten. Source repos are not deleted or
archived here.

[LEDGER.md](LEDGER.md) records, for every include, optional, absorb, and
exclude row: source repo, default-branch commit SHA, `module(name=...)`,
`module(version=...)`, and whether the tree is imported.

## What this repo is not

- Not a single Bazel module and not a root `MODULE.bazel`.
- Not a lockstep version for the constellation.
- Not [fastverk/platform](https://github.com/fastverk/platform) (gateway /
  adapter vehicle: forge, tracker, service-finder, wave).
- Not [fastverk/contracts](https://github.com/fastverk/contracts) (public
  protos; `fastverk_contracts`).
- Not [fastverk/botnoc](https://github.com/fastverk/botnoc) (private shell /
  control plane).
- Not `fastverk/plugin-shell` (console plugins are a different vehicle).
- Not a replacement for the existing implementation GitHub repos in this PR.
  Those remotes stay; this tree is an additional git/CI/release surface.
