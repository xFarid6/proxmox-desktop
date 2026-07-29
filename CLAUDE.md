# CLAUDE.md — proxmox-desktop (pxx-dex)

Agent-facing notes. Terse on purpose — this file gets read every session.
Public-facing prose lives in `README.md`; strategy lives in the Obsidian vault.

## What this is

Native Proxmox VE client. Tauri v2 + Vue 3 + TS + Rust. One codebase, two
targets: desktop + Android (scaffold `src-tauri/gen/android/`). The original of
the four-sibling suite. **Public, dual MIT/Apache-2.0.**

## Licensing — read before porting any sibling code

| Repo | Visibility | License |
|---|---|---|
| proxmox-desktop (this) | public | MIT OR Apache-2.0 |
| conn-manager-rs | public | MIT OR Apache-2.0 (relicensed 2026-07-29 to unblock #64) |
| hopline | **private** | FSL-1.1-MIT |
| dockshell | public | FSL-1.1-MIT |
| pgcove | public | FSL-1.1-MIT |

**Do not copy code from hopline, dockshell or pgcove into this repo.** You own
the copyright, so it is not a legal violation — it is a business one. hopline is
the private, paid product; moving its core into a public permissive repo
publishes the moat for free.

Reading a sibling to understand a *design* is fine and encouraged. Copying its
*text* is not. When an issue says "port hopline's SSH work", it means:
re-implement the security design (TOFU host-key verification, auth ladder,
connect timeout) as fresh MIT/Apache code here. Not copy-paste — and that
includes small files like `known_hosts.rs`.

conn-manager-rs is the one exception, and only because it was relicensed to
match this repo. Depend on it freely.

## State

[v0.2.0 released](https://github.com/xFarid6/proxmox-desktop/releases/tag/v0.2.0)
2026-07-29 — v1, v2, android-v1 all shipped. Release-signing secrets live in
repo secrets. Two open milestones:

**v4-integration** (runs first)
- #64 adopt conn-manager crate
- #23 SSH mode — fresh implementation, hopline as design reference only
- #65 Docker inside a guest — `docker` CLI over the #23 channel, not a bollard port
- #66 live-cluster + real-device validation

**v3** — #22 network editing, #20 certificates, #24 multi-cluster, #18 HA, #19 Ceph.

Order is a real dependency chain, not taste: #64 → #23 → #65. SSH creds ride the
migrated secret store; Docker talks over the SSH channel. In v3, #24 before
#18/#19 so those views are born multi-cluster-aware.

#65 is the most speculative item — "containers on Proxmox" usually means LXC,
which this app already manages via the API. #65 is about *Docker inside* an
LXC/VM. Real homelab pattern, but cut it first if the milestone runs long.

## Workflow

- One branch + one PR per issue. Small commits. Merge `--no-ff`, never squash.
- Branch protection on `main` is enforced (public repo): required checks
  `secrets, frontend, rust, tauri-build`, enforce_admins on, **strict** — head
  must be up to date with base, so merges go serial, never parallel:
  ```sh
  gh api -X PUT repos/xFarid6/proxmox-desktop/pulls/N/update-branch
  gh pr merge N --merge --auto
  ```
  (`gh pr update-branch` does not exist in the installed gh.)
- Board: GitHub Projects v2 #2 "Proxmox Desktop". Move cards as you work them.
- Never more than two feature branches open at once.

## Gates — all must pass before pushing

```sh
pnpm typecheck && pnpm lint && pnpm test && pnpm exec vite build
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

pnpm v11+. Never npm/yarn. Frontend tests are Vitest.

## Testing caveat

CI runs against a **mocked** Proxmox API (fixture HTTP server) and builds the
Android app without running it. Green CI means correct behaviour against
recorded responses — not verified against a real cluster or a real phone. That
gap is #66.

Live targets on the tailnet (credentials NEVER in the repo; topology in the
vault's `desktop-reference/`):

| Host | Address | Use |
|---|---|---|
| proxmox | `100.80.231.52:8006`, SSH `:22` | the real cluster |
| wyse-server | `100.77.208.85:22` | Debian, non-Proxmox SSH target / bastion for #23 |

Phone `redmi-note-13-pro-5g` was offline as of 2026-07-29 — the real-device leg
of #66 is blocked until it is back on the tailnet.

Never point a destructive action (delete, restore-over, firewall drop) at
anything on the live host that is not a throwaway guest made for the test.

## Windows dev box quirks

- cargo/rustc not on PATH in a fresh shell: `export PATH="$HOME/.cargo/bin:$PATH"`
  (bash) / `$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"` (PowerShell).
- Android: JDK 21 `%LOCALAPPDATA%\Java\jdk-21.0.11+10`, SDK
  `%LOCALAPPDATA%\Android\Sdk`, NDK `26.1.10909125`. Set `JAVA_HOME`,
  `ANDROID_HOME`, `NDK_HOME`.
- **The space in this repo's path breaks the NDK clang linker.** `subst` does not
  help (Tauri CLI canonicalizes back, panics `AssetDirOutsideOfAppRoot`).
  Fix: `$env:CARGO_TARGET_DIR = "$env:USERPROFILE\cargo-target\proxmox-desktop"`.
- SDK licences: write hash files directly into `Sdk\licenses\`; piping `y` into
  `sdkmanager.bat` silently fails here.
- R8/ProGuard needs `-dontwarn javax.annotation.**` (tink, via
  `androidx.security:security-crypto`).
- Local signed Android build: gitignored
  `src-tauri/gen/android/keystore.properties` (`keyAlias`/`password`/`storeFile`),
  then `pnpm tauri android build --target aarch64`. Omit the file for unsigned.
- Upload keystore + password live outside this repo on the dev box. **Losing them
  breaks updates for anyone who installed the APK.**
- `keyring`'s `sync-secret-service` feature needs dbus dev headers on
  `ubuntu-latest`. This repo does not install them explicitly and is green
  anyway — `libwebkit2gtk-4.1-dev` pulls them in transitively. That is an
  implicit dependency, so if a future change drops or slims the webkit apt line,
  add `libdbus-1-dev pkg-config` in the same commit. conn-manager-rs hit this
  gap for real and had to add them.

## Longer-form context

Obsidian vault: `Claude-understandings/proxmox-desktop-status.md` (hub),
`ship-and-sell-plan.md` (strategy), `conn-manager-rs-status.md`.
