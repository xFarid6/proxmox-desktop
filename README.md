# Proxmox Desktop

Native desktop client for [Proxmox VE](https://www.proxmox.com/en/proxmox-virtual-environment/overview), built with Tauri v2 (Rust) and Vue 3. One codebase, two targets: desktop and Android (scaffold at `src-tauri/gen/android/`).

> **Status: v1, v2, and android-v1 shipped — [v0.2.0 released](https://github.com/xFarid6/proxmox-desktop/releases/tag/v0.2.0).** 7 desktop installers (Windows NSIS+MSI, macOS, Linux AppImage/deb/rpm) plus a signed Android APK/AAB pipeline. Tested against a mocked Proxmox API in CI — not yet verified against a live cluster.

## Tech stack

- **Backend:** Rust, Tauri v2, `reqwest` Proxmox API client
- **Frontend:** Vue 3 (Composition API, `<script setup>`) + TypeScript + Vite
- **Auth:** Proxmox API tokens, stored in OS-native secure storage (never plaintext)

## Build / run locally

Prerequisites: Rust (stable), Node.js ≥ 20, pnpm, and [Tauri v2 platform deps](https://v2.tauri.app/start/prerequisites/).

```sh
pnpm install
pnpm tauri dev
```

## Scope

### v1 (done)

- Manage multiple Proxmox connections (host + API token, secure storage, self-signed cert opt-in)
- Cluster/node dashboard: CPU/RAM/disk/network at a glance
- VM/CT list with start/stop/reboot/shutdown
- Basic VM/CT create wizard
- VM/CT detail + hardware edit (cores, RAM, disk resize)
- Embedded console (noVNC / xterm.js)
- Live task/log panel
- Read-only network view

### v2 (done)

- Backup/restore: backup now (vzdump), browse/restore/delete archives, scheduled job + replication views
- Firewall rules: list/add/delete + enable toggle at cluster/node/guest scope
- Storage pool management: list definitions, add dir/nfs/cifs, remove
- Users, realms and ACL management
- Task-failure alerts (toast + native notification)
- Create wizard: cloud-init, VLAN, guest agent, unprivileged/nesting, static IP, start-after-create
- Windows/macOS/Linux installers built on tag push (`v*`), cross-OS tests in CI

### android-v1 (done)

- Responsive mobile UI: bottom tab bar below 768px, tables collapse to cards, 44px touch targets
- Android Keystore token storage (EncryptedSharedPreferences, AES-256)
- Tailscale-friendly connection timeouts + offline stale-data handling
- Mobile console toolbar: Esc/Tab/sticky-Ctrl/Ctrl+Alt+Del, pinch-zoom and pan
- Background-aware task alert notifications
- Pull-to-refresh
- Signed Android APK/AAB release pipeline in CI

### v3 (backlog)

- [#22](https://github.com/xFarid6/proxmox-desktop/issues/22) Network editing (bridges/VLANs/bonds)
- [#20](https://github.com/xFarid6/proxmox-desktop/issues/20) Certificate management
- [#24](https://github.com/xFarid6/proxmox-desktop/issues/24) Multi-cluster support
- [#18](https://github.com/xFarid6/proxmox-desktop/issues/18) HA management
- [#19](https://github.com/xFarid6/proxmox-desktop/issues/19) Ceph management

### v4-integration (backlog)

- [#64](https://github.com/xFarid6/proxmox-desktop/issues/64) Adopt the shared conn-manager crate
- [#23](https://github.com/xFarid6/proxmox-desktop/issues/23) SSH connection mode (ports hopline's russh work)
- [#65](https://github.com/xFarid6/proxmox-desktop/issues/65) Docker containers inside a guest (dockshell crossover)
- [#66](https://github.com/xFarid6/proxmox-desktop/issues/66) Validate against the live cluster and a real device

## Sibling tools

[hopline](https://github.com/xFarid6/hopline) (SSH/terminal manager),
[dockshell](https://github.com/xFarid6/dockshell) (Docker GUI) and
[pgcove](https://github.com/xFarid6/pgcove) (Postgres/Supabase client) share
this repo's architecture and licence. They started as this app's
connection-manager and console code generalized into standalone tools; each one
hardened a piece in isolation, and those pieces come back here:

- [`conn-manager-rs`](https://github.com/xFarid6/conn-manager-rs) — shared
  profile store + OS keyring, used by all four (this repo adopts it in #64)
- hopline's russh SSH client → SSH mode (#23)
- dockshell's Docker management → Docker inside a guest (#65)

Same owner, same licence, so code moves between them freely.

## Testing

CI runs against a **mocked Proxmox API** (fixture HTTP server) — there is no live Proxmox cluster in CI. Green CI means the client behaves correctly against recorded/mocked responses, not that it has been verified against a real cluster. Live-cluster validation is tracked as [#66](https://github.com/xFarid6/proxmox-desktop/issues/66).

## License

[Functional Source License, Version 1.1, MIT Future License](LICENSE)
(FSL-1.1-MIT) — the same license as its siblings. Source-available: read, use,
modify and redistribute freely; you just can't sell a competing product with it.
Each version becomes plain MIT two years after release. Plain-language summary
in [docs/licensing.md](docs/licensing.md).

Releases up to and including v0.2.0 were published under MIT/Apache-2.0 and stay
that way.

## Docs

Agent-facing and reference docs live in [`docs/`](docs/README.md) — API surface,
licensing summary, and (as they land) anything else too long for this file.
