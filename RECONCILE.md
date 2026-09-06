# macOS meta-repo reconcile inventory

Actionable file inventory for reconciling the duplicated macOS / settings /
credhelper surface between the private meta-repo
[`fastverk/fastverk`](https://github.com/fastverk/fastverk) and this vehicle.

This document does **not** archive or delete the meta-repo, does **not** mint
or rewrite Apple / WorkOS / Cirrus secrets, and does **not** touch iOS AuthKit
(`fastverk-app/app/ios/**`, owned separately).

Tree-level finding first recorded in
[docs#2](https://github.com/fastverk/docs/pull/2) /
[consolidation](https://docs.fastverk.com/consolidation.html). This page is
the file-level inventory that page asked for before any copy or delete.

## How to read the classes

| Class | Meaning | Action |
| --- | --- | --- |
| **(A)** | Meta-repo ahead | Copy the named delta into this vehicle (checklist below). Do not `cp -r` the tree. |
| **(B)** | This vehicle ahead | Leave the meta-repo copy as archive / notice. Do not copy meta → vehicle. |
| **(C)** | Divergent, or the delta does not apply cleanly | Matthew decides. Do not port in either direction until then. |
| **(D)** | Already identical (or the named delta is already satisfied here) | No code motion. |

Do not sync by copying a whole tree. [Consolidation](https://docs.fastverk.com/consolidation.html)
already showed that "which side is ahead" is per-tree, sometimes per-file.

## Compared revisions

| Side | Repo | Revision used here |
| --- | --- | --- |
| Vehicle | [fastverk/desktop](https://github.com/fastverk/desktop) | this branch, imported trees `fvkit/` @ `ca638b99` and `fastverk-app/` @ `985a7a0f` (LEDGER SHAs; both sources retired) |
| Meta | [fastverk/fastverk](https://github.com/fastverk/fastverk) (private) | [docs#2](https://github.com/fastverk/docs/pull/2) live-tree measurement on 2026-09-05. **This agent could not clone the meta-repo** (org token 404s a private remote). Meta-side dates and line counts below are that measurement, not a new byte-diff. |

A follow-up agent with read access to `fastverk/fastverk` should attach
`sha256` of each meta file next to the vehicle hashes in the file tables
and re-classify any row still marked **(C) pending byte-diff**.

## Tree-level classification

| Meta path | Vehicle dest | Class | Ahead | Evidence | Runner for any port |
| --- | --- | --- | --- | --- | --- |
| `app/desktop/` | `fastverk-app/app/desktop/` | **(B)** | vehicle | Vehicle `src/main.rs` is 426 lines; docs#2 measured meta at 230. Identity RPCs (`Login` / `Logout` / `WhoAmI`) and the busy-pulse tray exist only here. | n/a — do not port |
| `app/settings/` | `fastverk-app/app/settings/` | **(A)** | meta | Meta last touched 2026-08-13 (*Remove BuildBuddy from the settings connect UI*, fastverk#38). Vehicle `src/main.rs` last meaningful edit 2026-06-28 and still offers `buildbuddy` + an API-key field. | **macOS** (`eframe` / `winit` / glow) |
| `tools/credhelper/` | `fastverk-app/tools/credhelper/` | **(C)** then likely **(D)** | date says meta; delta likely superseded | Meta 2026-07-08 vs vehicle 2026-06-25 — docs#2 named the delta as *fetch the `fvkit` git-dep over https, not ssh*. This vehicle does not use a git-dep: Cargo is `path = "../fvkit/app/core"`, Bazel is `bazel_dep(name = "fvkit", …)`. Confirm `cred_helper.rs` is byte-identical before treating as (A). | Linux-safe **if** the leftover is only a Cargo git URL; **macOS** if `cred_helper.rs` itself changed |
| `tools/macos/` | `fastverk-app/tools/macos/` | **(B)** | vehicle | Vehicle also packages the Swift dashboard + `fvd-json`, and `fastverk-app/.github/workflows/release.yml` Developer-ID notarizes and pushes the CDN. Meta path is the older ad-hoc-signed packaging. | n/a — do not port. Signing/notarize stays on **macOS** runners; Cirrus owns the secrets. |
| `proto/fastverk/v1/connection.proto` | `fvkit/proto/fastverk/v1/connection.proto` | **(D)** | tie | docs#2: byte-identical. | n/a |
| `proto/fastverk/v1/repos.proto` | `fvkit/proto/fastverk/v1/repos.proto` | **(D)** | tie | docs#2: byte-identical. | n/a |
| `proto/fastverk/v1/fvd.proto` | `fvkit/proto/fastverk/v1/fvd.proto` | **(B)** | `fvkit` | Vehicle 338 lines; reserved tag 9 (`api_key`) on 2026-08-14 (`ca638b99`). Meta still declared `string api_key = 9` (324 lines). | n/a — do not copy meta proto here |
| `proto/fastverk/v1/maintenance.proto` | `fvkit/proto/fastverk/v1/maintenance.proto` | **(B)** | `fvkit` | Vehicle 160 lines vs meta 104. | n/a |
| `proto/fastverk/build/v1/graph.proto` | — | stay on meta | unduplicated | Not a desktop vehicle file. Keep on the meta-repo. | n/a |

`graph.proto`, `cli/fv`, the devcontainer, and the org design corpus are
**not** desktop-vehicle trees. They stay on the meta-repo.

The 2026-08-13 settings change and the 2026-08-14 `fvd.proto` reserve are
two halves of one change, landed in two repos. Neither side has both.
That is why this is a merge, not a delete.

## Vehicle file inventory (concrete dests)

Hashes are SHA-256 of this vehicle HEAD. Meta hashes: attach after a
private-repo clone.

### `app/desktop` — class (B)

| Meta source | Vehicle dest | Lines | sha256 (vehicle) | Class | Notes |
| --- | --- | --- | --- | --- | --- |
| `app/desktop/src/main.rs` | `fastverk-app/app/desktop/src/main.rs` | 426 | `f18d39e6…7161ba54` | **(B)** | Identity client + `Busy` pulse. Meta 230 lines, neither feature. |
| `app/desktop/BUILD.bazel` | `fastverk-app/app/desktop/BUILD.bazel` | — | `0d3c59a0…1a2b59eb` | **(B)** | `tao` / `tray-icon` / `tokio` + `@fvkit`. |
| `app/desktop/Cargo.toml` | `fastverk-app/app/desktop/Cargo.toml` | — | `a6686dd3…dd8c1236` | **(B)** | Cargo/IDE only; Bazel is primary. |

### `app/settings` — class (A)

| Meta source | Vehicle dest | Lines | sha256 (vehicle) | Class | Notes |
| --- | --- | --- | --- | --- | --- |
| `app/settings/src/main.rs` | `fastverk-app/app/settings/src/main.rs` | 530 | `a9020654…6b069288` | **(A)** | Still lists `buildbuddy` (combo @ L155), API-key field (L167–171), and sends `ConnectParams.api_key` (L416–434). Port the 2026-08-13 UI removal here — see checklist. |
| `app/settings/BUILD.bazel` | `fastverk-app/app/settings/BUILD.bazel` | — | `43ac2978…9cec4e5` | **(D)** likely | No provider list. Confirm against meta; expected identical. |
| `app/settings/Cargo.toml` | `fastverk-app/app/settings/Cargo.toml` | — | `d078d3b8…d572de7a` | **(D)** likely | Same. |

In-process `fvkit::connections::connect` (not the gRPC
`ConnectProviderRequest`) is how settings still passes `api_key`. The
reserved proto tag 9 does **not** by itself compile-break this crate.

### `tools/credhelper` — class (C), expected (D) after byte-diff

| Meta source | Vehicle dest | Lines | sha256 (vehicle) | Class | Notes |
| --- | --- | --- | --- | --- | --- |
| `tools/credhelper/cred_helper.rs` | `fastverk-app/tools/credhelper/cred_helper.rs` | 120 | `8782d53e…1c3128c7` | **(C)** pending byte-diff | Protocol + `diagnose`. No git URL. If meta only changed the workspace git-dep, this file is **(D)**. |
| `tools/credhelper/BUILD.bazel` | `fastverk-app/tools/credhelper/BUILD.bazel` | — | `0d6c5145…d222018` | **(B)** / **(D)** | `@fvkit//app/core:fvkit` module dep + `cred_helper_layer` tar. No ssh git. |
| `tools/credhelper/Cargo.toml` | `fastverk-app/tools/credhelper/Cargo.toml` | — | `72a4b3f4…ed20c7d2` | **(D)** for this vehicle | `fvkit.workspace = true` → path dep, not `git@` / `ssh://`. **Do not** port a meta-repo `https://github.com/fastverk/fvkit` git-dep into this file. |

### `tools/macos` — class (B)

| Meta source | Vehicle dest | sha256 (vehicle) | Class | Notes |
| --- | --- | --- | --- | --- |
| `tools/macos/BUILD.bazel` | `fastverk-app/tools/macos/BUILD.bazel` | `b13df187…f57020d01` | **(B)** | Bundles `fastverk-dashboard` + `fvd-json` + cred-helper + settings + `fvd`. |
| `tools/macos/defs.bzl` | `fastverk-app/tools/macos/defs.bzl` | `8e2f1c9b…2fc8` | **(B)** | `macos_app_bundle` / `macos_dmg`. |
| `tools/macos/sign_notarize.sh` | `fastverk-app/tools/macos/sign_notarize.sh` | `3ae10dcc…c0ca26` | **(B)** | `codesign` + `notarytool` + staple. **macOS-only. Do not edit secrets.** |
| `tools/macos/entitlements.plist` | `fastverk-app/tools/macos/entitlements.plist` | `d8a545cd…b2a542` | **(B)** | Hardened-runtime entitlements. |
| `tools/macos/Info.plist` | `fastverk-app/tools/macos/Info.plist` | `8dc373df…3459214` | **(B)** | Bundle plist. |
| `tools/macos/install.sh` | `fastverk-app/tools/macos/install.sh` | `d39053cc…e85ff2` | **(C)** | Vehicle file still sets `REPO="fastverk/fastverk"`. Self-updater + `release.yml` already publish `fastverk-app` (and the in-app updater watches that repo). Repoint vs leave: Matthew. **Not** a meta→vehicle copy. |

### Sibling trees that belong to this vehicle (not a meta delete target)

These exist on the vehicle (or only here) and are **not** the four
duplicated meta trees. Listed so a later pass does not "reconcile" them
away.

| Vehicle path | Class | Notes |
| --- | --- | --- |
| `fastverk-app/app/dashboard/**` | **(B)** / vehicle-only | SwiftUI meridian renderer. Bundled by `tools/macos`. **macOS / Xcode.** |
| `fastverk-app/tools/fvdjson/**` | **(B)** / vehicle-only | JSON shim for the dashboard. |
| `fastverk-app/proto/fastverk/ui/v1/**` | vehicle-only | Dashboard panel bundle. |
| `fastverk-app/deploy/cdn/index.html` | vehicle | CDN landing page. Do not fold into the meta-repo. |
| `fastverk-app/deploy/cfn/app-distribution.yaml` | vehicle | OIDC role for CDN push. No secret values in-tree. |
| `fastverk-app/.github/workflows/release.yml` | vehicle **(B)** | Notarize + GitHub Release + CDN. **Mentions** `DEVELOPER_ID_*` / `AC_API_*` / `KEYCHAIN_PASSWORD` by name only. **Do not add, rotate, or invent those secrets. Cirrus owns them.** |
| `fastverk-app/.github/workflows/ios-release.yml` | out of scope | iOS AuthKit / ASC. Do not touch. |
| `fastverk-app/app/ios/**` | out of scope | iOS console. Do not touch. |
| `fastverk-app/MODULE.bazel`, `Cargo.toml`, `BUILD.bazel` | vehicle wiring | Path/module deps already replace the meta-repo ssh git-dep. |
| `fvkit/crates/fvkit-core/src/connections.rs` | **(C)** | Daemon still has a `buildbuddy` preset (`x-buildbuddy-api-key`, `BUILDBUDDY_API_KEY`). Settings UI (A) can drop the provider without deleting this preset. Dropping the preset is a separate Matthew call. |
| `fvkit/proto/fastverk/v1/fvd.proto` | **(B)** | Tag 9 reserved. Keep. |
| meta `cli/fv`, `tools/ci/bootstrap-cred-helper.sh` | stay on meta | Consolidation sequencing (delete / repoint) is **after** a green macOS settings build. Not this PR. |

## (A) follow-up checklist — exact source → dest

No (A) row is compiled or copied in this PR. Settings links `eframe` /
`winit` and cannot be verified on a Linux Cloud Agent.

### A1. Settings: drop BuildBuddy from the Connect panel

| | |
| --- | --- |
| Meta source | `fastverk/fastverk` `app/settings/src/main.rs` (fastverk#38, 2026-08-13) |
| Vehicle dest | `fastverk-app/app/settings/src/main.rs` |
| Also consider | dest `BUILD.bazel` / `Cargo.toml` only if the meta PR touched them (expected no) |
| Do **not** dest | `fvkit/crates/fvkit-core/src/connections.rs` (unless Matthew chooses C1) |
| Do **not** dest | `fvkit/proto/fastverk/v1/fvd.proto` (already reserved here) |
| Secrets | none. Do not add API keys, ASC, or WorkOS values. |
| Verify | **macOS runner**: `cd fastverk-app && bazel test //...` and `bazel build //app/settings:fastverk-settings`. Linux CI will not link this crate. Vehicle `tools/ci/modules.json` already runs `fastverk-app` on `macos-latest` only. |

Port by diff, not `cp`. Expected vehicle hunks (confirm against the meta
file before applying):

1. Provider combo: `["github", "gitlab", "buildbuddy"]` → `["github", "gitlab"]`.
2. Remove the BuildBuddy-only API-key field and the `!= "buildbuddy"` host
   special case.
3. Stop sending a user-typed `api_key` from the UI. Empty-string
   `ConnectParams.api_key` is enough if the struct still requires the field.

### A2. Credhelper https git-dep — **do not copy blindly**

| | |
| --- | --- |
| Meta source | `fastverk/fastverk` `tools/credhelper/**` and/or the meta workspace `Cargo.toml` (2026-07-08) |
| Vehicle dest | **none**, unless `cred_helper.rs` itself differs |
| Why | Vehicle already fetches `fvkit` over the Bazel registry / sibling path, not ssh. Copying an https git-dep would be a regression. |

Follow-up agent with meta read access:

```text
diff -u \
  <(meta) app/settings/src/main.rs \
  fastverk-app/app/settings/src/main.rs
diff -u \
  <(meta) tools/credhelper/cred_helper.rs \
  fastverk-app/tools/credhelper/cred_helper.rs
```

If `cred_helper.rs` is identical → flip the credhelper row to **(D)** and
leave the meta tree as notice. If it is not, attach the hunk and re-class.

## (C) Matthew decisions

1. **C1 — BuildBuddy in `fvkit`.** Drop UI only (A1), or also remove the
   daemon preset / `AUTH_KIND_API_KEY` path in
   `fvkit/crates/fvkit-core/src/connections.rs`? The helper "refuses" a
   provider that is not in the registry; the vehicle daemon still *can*
   create a `buildbuddy` connection. Two different layers.
2. **C2 — `install.sh` download repo.** Decided: `install.sh` now
   downloads from `fastverk/desktop` (`fastverk-app/vX.Y.Z` tags). The
   in-app updater still watches published `fvkit` 0.0.4
   (`RELEASE_REPO = fastverk/fastverk-app`) until that crate is rebuilt
   against this tree. Do not point `install.sh` at the retired
   `fastverk/fastverk` or `fastverk/fastverk-app` remotes.
3. **C3 — When to delete the meta copies.** Consolidation step 3 says:
   port (A), green macOS build, *then* delete meta
   `app/desktop`, `app/settings`, `tools/credhelper`, `tools/macos`, the
   competing meta `release.yml`, and the four duplicated protos (keep
   `graph.proto`). **Not this PR.** Do not archive the meta-repo.
4. **C4 — `bootstrap-cred-helper.sh` on the meta-repo.** Repoint after
   the crate is gone. Meta-only; Linux-safe when its time comes.

## Linux-safe vs must-run-on-macOS

| Work | Where | Why |
| --- | --- | --- |
| This inventory (markdown only) | Linux | No compile. |
| Byte-diff meta vs vehicle once the private remote is readable | Linux | `diff` / `sha256sum`. |
| A1 settings port **compile + test** | **macOS** | `tao` / `tray-icon` / `eframe` link macOS frameworks. Vehicle CI: `fastverk-app` is `macos-latest` only. |
| A1 settings port **edit** | Linux-safe to type; **not** verify | A Cloud Agent can apply a reviewed hunk; it cannot green-build it here. |
| `app/desktop` / dashboard / `//tools/macos:fastverk_app` | **macOS** | Same frameworks + `codesign` / `hdiutil` (`manual` + `local`). |
| Developer ID sign, notarize, staple | **macOS + Cirrus secrets** | `sign_notarize.sh` / `release.yml`. Do not mint `DEVELOPER_ID_*`, `AC_API_*`, `KEYCHAIN_PASSWORD`. |
| iOS AuthKit / `ios-release.yml` | **out of scope** | Separate owner. |
| Proto **(B)/(D)** rows, LEDGER, this file | Linux | Already done or no-op. |
| Credhelper `cred_helper` crate unit tests | Linux-capable (`//tools/credhelper:cred_helper_test`) | No AppKit. `cred_helper_layer` is the linux/amd64 tar. |

## What this PR will not do

- Merge itself (Matthew merge-commits).
- Archive or delete `fastverk/fastverk`.
- Copy any tree wholesale.
- Apply A1 without a meta-side file in hand and a macOS verify.
- Invent or rotate Apple / WorkOS / Cirrus secrets.
- Change `fastverk-app/app/ios/**` or `ios-release.yml`.

## Next steps (Matthew, merge-commit)

1. **Merge-commit this PR** so the inventory sits next to `LEDGER.md`.
2. Grant a Cloud Agent **read** on private `fastverk/fastverk` (no write,
   no secrets). Re-run the four `diff -u` lines and fill meta sha256s.
3. If A1 is still (A): port `app/settings/src/main.rs` by hunk on a
   **macOS** agent or CI `macos-latest` job. Leave `connections.rs` until C1.
4. Flip credhelper to (D) or attach the real `cred_helper.rs` hunk (C).
5. Decide C2 (`install.sh` repo) and C1 (daemon BuildBuddy preset).
6. Only after a green `bazel test //...` / `bazel build //app/settings:fastverk-settings` on macOS: schedule the meta-repo *delete of the duplicated trees* as a later PR **on the meta-repo**, not an archive of the remote.

Companion reading: [LEDGER.md](LEDGER.md) (imported SHAs + retirement),
[docs consolidation](https://docs.fastverk.com/consolidation.html) (sequence).
