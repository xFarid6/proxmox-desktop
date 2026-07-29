# API surface — proxmox-desktop

Reference, not prose. Covers every `#[tauri::command]` in `src-tauri/src/`
(commands.rs, console.rs, ssh_console.rs), its matching wrapper in
`src/api.ts`, the headless `pxx` CLI, and how to point either at a live
cluster.

Docker-in-guest commands (`docker.rs`, issue #65) are landing on a parallel
branch and are not present on this branch — see that branch/PR for their
command table once merged.

## Tauri commands

All commands return `Result<T, String>` to the frontend — errors surface as a
rejected `invoke()` promise carrying a plain-string message. Unless noted,
that string comes from one of: the connections-store error mapper (bad/corrupt
connections file, missing connection id, keyring failure), or
`proxmox::Error`'s `Display` (`HTTP error: ...`, `Proxmox API error (status):
body`, `unexpected response shape: ...`). "Mutates" / "Destructive" call out
commands that write to or remove something on the cluster, vs. plain reads.

| Command | Params | Returns | Does | Frontend call (`src/api.ts`) |
|---|---|---|---|---|
| `list_connections` | `app` | `Vec<ConnectionInfo>` | Lists saved connections (host/name/cert-flag; no secrets) from the local store. | `api.listConnections()` |
| `save_connection` | `app, info: ConnectionInfo, token: Option<String>, ssh_secret: Option<String>` | `()` | Upserts a connection; token/SSH secret go to the OS keyring, never disk. **Mutates.** | `api.saveConnection(info, token?, sshSecret?)` |
| `delete_connection` | `app, id: String` | `()` | Deletes a connection and its keyring secrets (token + namespaced SSH secret). **Destructive.** | `api.deleteConnection(id)` |
| `cluster_resources` | `app, connection_id: String` | `Vec<ClusterResource>` | `GET /cluster/resources` — nodes, guests, storage in one call. | `api.clusterResources(connectionId)` |
| `guest_power` | `app, connection_id, node: String, kind: GuestKind, vmid: u32, action: PowerAction` | `String` (task UPID) | Start/stop/reboot/shutdown a guest. **Mutates.** | `api.guestPower(connectionId, node, kind, vmid, action)` |
| `node_storages` | `app, connection_id, node: String` | `Vec<StorageSummary>` | Storages available on a node. | `api.nodeStorages(connectionId, node)` |
| `storage_content` | `app, connection_id, node, storage: String, content: Option<String>` | `Vec<StorageContent>` | Volumes on a storage, optionally filtered (`iso`, `vztmpl`, `images`, ...). | `api.storageContent(connectionId, node, storage, content?)` |
| `create_guest` | `app, connection_id, node, kind, params: HashMap<String,String>` | `String` (task UPID) | Creates a VM/CT from raw Proxmox form params. **Mutates.** | `api.createGuest(connectionId, node, kind, params)` |
| `guest_config` | `app, connection_id, node, kind, vmid` | `serde_json::Value` | Raw guest config; key set varies qemu vs. lxc. | `api.guestConfig(connectionId, node, kind, vmid)` |
| `set_guest_config` | `app, connection_id, node, kind, vmid, params` | `Option<String>` (UPID for qemu, `None` for lxc) | Updates config fields (cores, memory, ...). **Mutates.** | `api.setGuestConfig(connectionId, node, kind, vmid, params)` |
| `resize_disk` | `app, connection_id, node, kind, vmid, disk: String, size: String` | `Option<String>` | Grows a disk (`size` like `+5G`); Proxmox does not support shrink. **Mutates.** | `api.resizeDisk(connectionId, node, kind, vmid, disk, size)` |
| `node_network` | `app, connection_id, node` | `Vec<NetworkInterface>` | Network interfaces on a node — read-only view. | `api.nodeNetwork(connectionId, node)` |
| `node_tasks` | `app, connection_id, node` | `Vec<TaskEntry>` | Recent tasks on a node (server-side limit 50). | `api.nodeTasks(connectionId, node)` |
| `task_status` | `app, connection_id, node, upid: String` | `TaskStatus` | Status of one task. | `api.taskStatus(connectionId, node, upid)` |
| `task_log` | `app, connection_id, node, upid, start: Option<u64>` | `Vec<TaskLogLine>` | Task log lines from `start` (default 0). | `api.taskLog(connectionId, node, upid, start?)` |
| `vzdump` | `app, connection_id, node, params` | `String` (task UPID) | Backs up guests now (vmid/storage/mode/compress in `params`). **Mutates.** | `api.vzdump(connectionId, node, params)` |
| `delete_volume` | `app, connection_id, node, storage, volid: String` | `Option<String>` | Deletes a storage volume (e.g. a backup archive). **Destructive.** | `api.deleteVolume(connectionId, node, storage, volid)` |
| `backup_jobs` | `app, connection_id` | `Vec<BackupJob>` | Scheduled backup jobs, cluster-wide. | `api.backupJobs(connectionId)` |
| `replication_jobs` | `app, connection_id` | `Vec<ReplicationJob>` | Replication jobs, cluster-wide. | `api.replicationJobs(connectionId)` |
| `access_users` | `app, connection_id` | `Vec<AccessUser>` | Lists users. | `api.accessUsers(connectionId)` |
| `add_user` | `app, connection_id, params` | `()` | Creates a user (`userid` like `name@pve`; password only works on the `pve` realm). **Mutates.** | `api.addUser(connectionId, params)` |
| `delete_user` | `app, connection_id, userid: String` | `()` | Deletes a user. **Destructive.** | `api.deleteUser(connectionId, userid)` |
| `access_domains` | `app, connection_id` | `Vec<AccessDomain>` | Lists auth realms. | `api.accessDomains(connectionId)` |
| `access_roles` | `app, connection_id` | `Vec<AccessRole>` | Lists roles. | `api.accessRoles(connectionId)` |
| `access_acl` | `app, connection_id` | `Vec<AclEntry>` | Lists ACL entries. | `api.accessAcl(connectionId)` |
| `set_acl` | `app, connection_id, params` | `()` | Grants/revokes an ACL (`path`, `roles`, `users`\|`groups`\|`tokens`, `delete=1` to revoke). **Mutates.** | `api.setAcl(connectionId, params)` |
| `storage_configs` | `app, connection_id` | `Vec<StorageConfig>` | Cluster-wide storage definitions (`storage.cfg`). | `api.storageConfigs(connectionId)` |
| `add_storage` | `app, connection_id, params` | `()` | Adds a storage definition (`storage`, `type`, `content`, `path`/`server`/...). **Mutates.** | `api.addStorage(connectionId, params)` |
| `delete_storage` | `app, connection_id, storage: String` | `()` | Removes a storage definition; leaves the data on it untouched. **Destructive.** | `api.deleteStorage(connectionId, storage)` |
| `firewall_rules` | `app, connection_id, node: Option<String>, kind: Option<GuestKind>, vmid: Option<u32>` | `Vec<FirewallRule>` | Lists firewall rules at cluster/node/guest scope (scope picked by which of `node`/`kind`/`vmid` are set). | `api.firewallRules(connectionId, scope)` |
| `add_firewall_rule` | `app, connection_id, node?, kind?, vmid?, params` | `()` | Adds a firewall rule at the given scope. **Mutates.** | `api.addFirewallRule(connectionId, scope, params)` |
| `delete_firewall_rule` | `app, connection_id, node?, kind?, vmid?, pos: u32` | `()` | Deletes a firewall rule by position. **Destructive.** | `api.deleteFirewallRule(connectionId, scope, pos)` |
| `firewall_options` | `app, connection_id, node?, kind?, vmid?` | `serde_json::Value` | Raw firewall options for a scope (`enable`, `policy_in`, ...). | `api.firewallOptions(connectionId, scope)` |
| `set_firewall_options` | `app, connection_id, node?, kind?, vmid?, params` | `()` | Sets firewall options for a scope. **Mutates.** | `api.setFirewallOptions(connectionId, scope, params)` |
| `test_connection` | `app, host: String, token: Option<String>, accept_invalid_certs: bool, connection_id: Option<String>` | `Version` | Probes host+token (`GET /version`) before saving; for a saved connection, pass no token and it's read from the keyring. | `api.testConnection({host, token?, acceptInvalidCerts, connectionId?})` |
| `open_console` (console.rs) | `app, connection_id, node, kind, vmid, mode: ConsoleMode (Vnc\|Term)` | `ConsoleInfo { port, ticket, user }` | Fetches a vncproxy/termproxy ticket, binds a one-shot local `ws://127.0.0.1:{port}` listener, pipes it to the authenticated remote `wss://` endpoint (the webview can't send an Authorization header, so Rust dials it instead). | `api.openConsole(connectionId, node, kind, vmid, mode)` |
| `open_ssh_shell` (ssh_console.rs) | `app, sessions: State<SshSessions>, connection_id` | `ConsoleInfo { port, ticket, user }` | Opens a root shell over SSH (creds from keyring/agent/key file) and bridges it through a local pve-xtermjs-speaking websocket, gated by a random one-time nonce ticket (this socket is otherwise the only thing between a local process and a root shell). | `api.openSshShell(connectionId)` |

## `pxx` — headless CLI

`src-tauri/src/bin/pxx.rs`. Links `proxmox_desktop_lib` and drives
`proxmox::Client` directly — no Tauri runtime, no GUI, no keyring (a raw API
token is passed by hand via env var). **Read-only**: every subcommand is a
`GET`. Power, delete, create, vzdump, and firewall-write actions are
deliberately not implemented here — see "Left out" below.

Build/run: `cargo run --bin pxx -- <command> [args]` (from `src-tauri/`), or
build once with `cargo build --bin pxx` and run the resulting `pxx`/`pxx.exe`
directly.

### Env vars (credentials only — never flags, never a file)

| Var | Meaning |
|---|---|
| `PXX_HOST` | Base URL, e.g. `https://100.80.231.52:8006` |
| `PXX_TOKEN` | Full Proxmox API token: `user@realm!tokenid=uuid`. Never printed or logged. |
| `PXX_INSECURE` | Set to `1` to accept a self-signed certificate. |

Missing `PXX_HOST` or `PXX_TOKEN` exits 1 with a message naming which one is
unset. All output is pretty-printed JSON to stdout; errors go to stderr with
exit code 1; success exits 0.

### Subcommands

| Subcommand | Args | Maps to `Client` method |
|---|---|---|
| `version` | — | `client.version()` (cheap auth/reachability probe) |
| `resources` | — | `client.cluster_resources()` |
| `guest-config` | `<node> <qemu\|lxc> <vmid>` | `client.guest_config(node, kind, vmid)` |
| `node-network` | `<node>` | `client.node_network(node)` |
| `storages` | `<node>` | `client.node_storages(node)` |
| `tasks` | `<node>` | `client.node_tasks(node)` |

### Example

```sh
export PXX_HOST="https://100.80.231.52:8006"
export PXX_TOKEN="root@pam!pxx-cli=00000000-0000-0000-0000-000000000000"
export PXX_INSECURE=1   # only if the cluster uses a self-signed cert

cargo run --bin pxx -- resources
cargo run --bin pxx -- guest-config pve 100 lxc   # wrong order errors out cleanly
cargo run --bin pxx -- guest-config pve lxc 100    # node, kind, vmid
```

### Left out on purpose

Power actions, create/delete guest, resize-disk, vzdump, delete-volume, and
every firewall/ACL/storage write endpoint all mutate or destroy state on a
real cluster. This CLI is meant to be safe to run unattended against the live
homelab cluster while investigating or scripting, so none of them are wired
up. If a mutating command becomes genuinely necessary for agent workflows,
add it deliberately and narrowly (one action, one guarded subcommand) rather
than exposing `Client`'s full write surface.

## Testing against a live cluster

CI only ever exercises a mocked Proxmox API (see `CLAUDE.md`'s testing
caveat) — green CI is not proof of live-cluster behavior. To check against
the real thing:

- The live cluster is `proxmox` in `CLAUDE.md`'s host table (API on `:8006`,
  SSH on `:22`, reachable over the tailnet). `wyse-server` is the
  non-Proxmox SSH bastion used for #23 SSH-mode testing, not this CLI.
- Get an API token from that cluster's own UI/CLI (`Datacenter > Permissions
  > API Tokens`, or `pveum user token add ...`) — **never** commit it, put it
  in a script, or paste it into this repo. Export it as `PXX_TOKEN` in your
  shell for the session only.
- Run read-only `pxx` subcommands first (`version`, `resources`) to confirm
  connectivity and auth before anything that touches a real guest.
- Never point any mutating action (not exposed by `pxx`, but reachable via
  the GUI or `commands.rs` directly) at anything on the live host that isn't
  a throwaway guest made for the test — see `CLAUDE.md`.
