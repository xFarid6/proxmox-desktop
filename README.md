# Proxmox Desktop

Native desktop client for [Proxmox VE](https://www.proxmox.com/en/proxmox-virtual-environment/overview), built with Tauri v2 (Rust) and Vue 3.

> **Status: v1 feature-complete, pre-release.** Tested against a mocked Proxmox API in CI — not yet verified against a live cluster.

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

### v1 (in progress)

- Manage multiple Proxmox connections (host + API token, secure storage, self-signed cert opt-in)
- Cluster/node dashboard: CPU/RAM/disk/network at a glance
- VM/CT list with start/stop/reboot/shutdown
- Basic VM/CT create wizard
- VM/CT detail + hardware edit (cores, RAM, disk resize)
- Embedded console (noVNC / xterm.js)
- Live task/log panel
- Read-only network view

### Backlog (not v1)

User/realm/ACL management, firewall, backup/restore + replication, HA, Ceph, certificates, storage pools, network editing, SSH fallback mode, multi-cluster, full-parity create wizard, Windows/macOS packaged releases, notifications.

## Testing

CI runs against a **mocked Proxmox API** (fixture HTTP server) — there is no live Proxmox cluster in CI. Green CI means the client behaves correctly against recorded/mocked responses, not that it has been verified against a real cluster.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
