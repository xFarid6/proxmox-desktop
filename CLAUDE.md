# CLAUDE.md — proxmox-desktop (pxx-dex)

Agent-facing notes. Terse on purpose — this file gets read every session.
Public-facing prose lives in `README.md`; strategy lives in the Obsidian vault.

## What this is

Native Proxmox VE client. Tauri v2 + Vue 3 + TS + Rust. One codebase, two
targets: desktop + Android (scaffold `src-tauri/gen/android/`). The original of
the four-sibling suite. **Public, dual MIT/Apache-2.0.**

## Licensing — copying between the suite is the plan

| Repo | Visibility | License |
|---|---|---|
| proxmox-desktop (this) | public | FSL-1.1-MIT (relicensed 2026-07-29) |
| hopline | private | FSL-1.1-MIT |
| dockshell | public | FSL-1.1-MIT |
| pgcove | public | FSL-1.1-MIT |
| conn-manager-rs | public | MIT OR Apache-2.0 |

Same owner, same licence across all four apps. **Copy freely between them** —
building the small tools first so their hardened code could come back here was
the deliberate strategy, not an accident. When an issue says "port hopline's SSH
work", it means port it: take the code.

conn-manager-rs is permissive on purpose (a permissive shared library inside FSL
apps is a standard, fine pattern). Depend on it freely.

This repo was MIT/Apache-2.0 until 2026-07-29; **releases up to and including
v0.2.0 stay MIT/Apache forever** — already published, can't be withdrawn, and
that is fine. Everything from here is FSL. Don't "fix" the old release notes.

Adding a genuinely third-party dependency still needs its licence checked
normally — this rule is about the four sibling repos only.

## State

[v0.2.0 released](https://github.com/xFarid6/proxmox-desktop/releases/tag/v0.2.0)
2026-07-29 — v1, v2, android-v1 all shipped. Release-signing secrets live in
repo secrets.

**Both v3 and v4-integration are complete; both milestones are closed.** Do not
plan work off a milestone list — check `gh issue list --state open` first.

Shipped since the last time this section was accurate:

- **v4-integration**, all closed 2026-07-29: #64 conn-manager crate, #23 SSH mode
  (hopline's `ssh.rs` + `known_hosts.rs` ported), #65 Docker inside a guest,
  #66 live-cluster + real-device validation, #71 frontend tests.
- **v3**: #18 HA, #19 Ceph, #20 certificates (PRs #76/#77/#78) as
  `HaView`/`CephView`/`CertificatesView` + client methods + wiremock tests;
  #22 network editing and #24 multi-cluster also closed.
- **A UX/diagnostics wave** the old notes predate: #75 LAN + Tailscale host
  discovery, #84/#85 loading indicators, #86 split token ID/secret fields,
  #87/#90/#91 explain empty dashboards, 403s and empty storage instead of showing
  blanks, #88 create-task button, #89 backup preflight checks.

**Open as of 2026-08-03: eight issues, in two families.**

*Generic SSH host* — connect to a plain SSH box, not a PVE cluster. Driving case
is `wyse-server`'s 2026-08-02 webcam outage, diagnosed entirely by hand over ssh.
#102 is the foundation (connection type + nav gating); #103 terminal, #104 ports
& services, #105 Docker, #106 MJPEG viewer each add one tab on top of it.

*LLM panel* — #99 chat panel for a guest serving an OpenAI-compatible endpoint,
then #100 switch the served model, #101 context controls (clear / budget /
compact). Driving case is `lab`'s CT 100. The differentiator is *discovery*
(probe guests for `/v1/models`), not the chat UI: its hard part is that a guest's
service address is often not its Proxmox-visible IP — the real case sits behind a
NAT bridge and is reached over Tailscale or a host DNAT rule.

**#99's own issue text is wrong on one point:** it says the endpoint-resolution
logic is shared with #65 and should be lifted rather than rewritten. It cannot
be. #65 reaches guests over SSH-to-the-node plus `pct exec` / `qm guest exec`
(see `docker.rs`'s module doc) and never resolves an IP or a port at all. There
is no such code in this repo to lift.

Historical note kept because it still shapes the code: the v3 plan wanted #24
before #18/#19 so those views would be born multi-cluster-aware. It went the other
way, so #24 had to retrofit three views that each read `activeId` directly.

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
| lab (`lab.local.secondo`) | `192.168.1.13:8006`, tailnet `100.117.56.34` | **second** PVE 9.1 host, built 2026-08-01 |

`lab` is a second, single-purpose Proxmox box running a local LLM (CT 100, an
OpenAI-compatible endpoint on `:8080`). It is the driving case for #99 and a useful
second target generally: PVE **9.1** vs the main host's 8.x, a WiFi uplink, and one
guest on a port-less NAT bridge whose service IP is not its Proxmox-visible IP —
exactly the topology that breaks naive endpoint assumptions. Full detail in the
vault: `Claude-understandings/desktop-reference/lab-secondo-reference.md`.

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
