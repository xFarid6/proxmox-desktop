# CLAUDE.md — proxmox-desktop (pxx-dex)

## What this is

Native Proxmox VE desktop client (Tauri v2 + Vue 3 + TS + Rust). The original of
the four-sibling suite (dockshell, pgcove, hopline reuse its architecture).
**Public repo, MIT/Apache-2.0** — unlike the three private siblings; don't copy
FSL-licensed code from them into here.

## Workflow

- One branch + one PR per issue; CI must be green before merge (branch
  protection IS enforced here — public repo).
- Board: GitHub Project "proxmox-desktop". Move issues as you work them.

## Windows toolchain quirks (this dev machine)

- cargo/rustc not on PATH in fresh shells:
  `$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"`.
- Package manager is pnpm (v11+). Never npm/yarn.
- Live testing target: the homelab Proxmox host (see the vault's
  `desktop-reference/` for topology; credentials NEVER in the repo).

## Testing rules

- Backend: `cargo test` in `src-tauri/` (Proxmox API mocked in CI).
- Frontend: `pnpm test` (Vitest). Lint gates: `cargo fmt --check`,
  `cargo clippy -- -D warnings`, `pnpm lint`, `pnpm typecheck`.

## Current sprint (2026-07-17 → 2026-07-30)

**FROZEN this sprint.** Full plan: Obsidian vault →
`Claude-understandings/ship-and-sell-plan.md`. Only exception: work here that
directly feeds hopline's v0.1 (console/terminal fixes). pxx-dex v1 + Pro (€15)
is scheduled for November per `04-six-month-roadmap.md`; the shared-crate
extraction (issue #14 pattern across the suite) happens in September. If an
agent is pointed here by mistake, redirect effort to hopline.
