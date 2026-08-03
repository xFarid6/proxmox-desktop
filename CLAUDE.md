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

**The generic-SSH-host family is done.** #102 (connection type + nav gating),
#103 terminal, #104 ports & services, #105 Docker and #106 MJPEG viewer all
shipped 2026-08-03, driven by `wyse-server`'s 2026-08-02 webcam outage. #112/#113
then fixed the three defects the first GUI pass found — all four tabs had been
merged with their commands verified over raw SSH and their views never opened,
and two of the three failed *silently*. Hence `tools/cdp.mjs`: a GUI pass is part
of shipping now, not a nice-to-have.

**Open as of 2026-08-03: #99, #100, #101 (the LLM panel) and #114 (two cosmetic
defects in #102's connection form).**

*LLM panel* — #99 chat panel for a guest serving an OpenAI-compatible endpoint,
then #100 switch the served model, #101 context controls (clear / budget /
compact). The differentiator is *discovery*, not the chat UI.

Driving case, **topology confirmed live 2026-08-03**: `lab`'s CT 100 runs
llama.cpp on `0.0.0.0:8080` serving `qwen3-30b-a3b`, on the port-less NAT bridge
`vmbr1` (`vmbr0` is down). Four ways in, two of which work:

| path | address | result |
|---|---|---|
| guest tailnet | `100.111.194.35:8080` | **200** |
| host LAN DNAT | `192.168.1.13:8080` | **200** |
| host tailnet | `100.117.56.34:8080` | fails — the DNAT is on `wlo1`/`vmbr0`, not `tailscale0` |
| Proxmox-visible | `10.20.20.10:8080` | fails — unroutable from off-box |

So a candidate list must include the *guest's own* tailnet address and the node's
LAN address, and must not assume the node's tailnet address inherits the node's
DNAT rules. Note `tailscale status` lied about this box being offline for 12h —
when it disagrees with a direct ping, believe the ping.

**Two corrections to the issues' own text:**

- **#99 says its endpoint resolution is shared with #65 and should be lifted.**
  Half wrong. #65 is the wrong donor — it reaches guests over SSH-to-the-node plus
  `pct exec` / `qm guest exec` (see `docker.rs`'s module doc) and never resolves an
  IP or a port. But there *is* something to lift, from **#75**:
  `scan.rs::scan_tailscale` already enumerates tailnet peers, and the guest is its
  own peer (`llm`, `100.111.194.35`) — which is the candidate that actually works.
- **#100 is not an API call.** `llama-server` serves one model per process
  (`--model .../Qwen3-30B-A3B-Instruct-2507-UD-IQ2_M.gguf`, four `.gguf` files on
  disk). Switching means editing `/opt/llm/docker-compose.yml` and restarting,
  which rides the `pct exec` transport #65 already has. There is no load-a-model
  endpoint to call.

`--slots`, `--metrics` and `--tokenize` are all enabled on the real box, so #101
can use real token counts rather than a chars/4 estimate — but must still degrade
when they are off, which is llama.cpp's default.

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
python tools/gates.py          # all seven, in order, with a report
python tools/gates.py --only rust
```

which is exactly:

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

Green gates also do not mean the view renders: #112 shipped four tabs whose
commands were verified over raw SSH and whose views had never been opened, and
two of the three defects it found failed silently. **Open the tab in the running
app before calling a feature done.** Do that over CDP, not synthetic mouse and
keyboard — `node tools/cdp.mjs dev` then `node tools/cdp.mjs eval '<expr>'`; see
[tools/README.md](tools/README.md). Vite serves the frontend unbundled in dev, so
an eval can `await import("/src/api.ts")` and call any Tauri command directly.
Windows/WebView2 only.

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
