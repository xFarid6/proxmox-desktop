import { invoke } from "@tauri-apps/api/core";
import { explainError } from "./apierror";

/** Every command below goes through here rather than calling `invoke`
 * directly, so a permission failure is explained once instead of in each of
 * the fifteen views that print `String(e)` (#90). Rejects with a plain string
 * exactly like `invoke` does — views keep working unchanged. */
function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args).catch((e) => {
    throw explainError(e);
  });
}

/** Per-connection SSH shell config. Auth method is picked by which fields
 * are set: keyPath means key-file auth (secret = passphrase), otherwise
 * useAgent tries the platform's running ssh-agent/Pageant, otherwise the
 * secret is a plain password. */
export interface SshInfo {
  user: string;
  port: number;
  keyPath?: string | null;
  useAgent: boolean;
}

/** Whether a saved connection is a Proxmox cluster or a plain SSH host.
 * An SSH host has no PVE API and no API token — only `ssh` credentials. */
export type ConnectionKind = "pve" | "ssh";

export interface ConnectionInfo {
  id: string;
  name: string;
  host: string;
  kind: ConnectionKind;
  acceptInvalidCerts: boolean;
  ssh?: SshInfo | null;
}

export interface Version {
  version: string;
  release: string;
}

export type GuestKind = "qemu" | "lxc";
export type PowerAction = "start" | "stop" | "reboot" | "shutdown";

/** A Docker container running *inside* a guest, as its `docker ps` reported
 * it. `state` is the machine-readable one ("running", "exited"); `status` is
 * the human one ("Up 3 hours"). */
export interface DockerContainer {
  id: string;
  name: string;
  image: string;
  state: string;
  status: string;
  ports: string;
  /** Comma-separated network names; empty when the container is attached to
   * nothing, which is what a lost attachment looks like (#105). */
  networks: string;
  /** `always`, `unless-stopped`, `on-failure`, `no`. Only the SSH-host tab
   * fetches it; `null` for a guest's containers and when none is set. */
  restartPolicy: string | null;
}

export type DockerAction = "start" | "stop" | "restart";

/** A listening socket on a plain SSH host, from its `ss` output (#104).
 * `process`/`pid` are null when the SSH user is not root — `ss` omits the
 * owning process for sockets it does not own, which is normal, not an error.
 * `address` is verbatim: `0.0.0.0`, `[::]`, `*`, or a specific address. */
export interface ListeningPort {
  proto: string;
  address: string;
  port: number;
  process: string | null;
  pid: number | null;
}

/** A systemd service unit. `active` is the high-level state
 * ("active"/"failed"), `sub` the detailed one ("running"/"failed"). */
export interface ServiceUnit {
  name: string;
  load: string;
  active: string;
  sub: string;
  description: string;
}

/** A media endpoint found among an SSH host's listening ports (#106).
 * `kind` is "stream" for `multipart/x-mixed-replace` (an MJPEG feed the
 * viewer can keep open) and "snapshot" for a single still image. `path` is
 * the path that was actually probed, so the URL rendered is the URL proven
 * to answer. */
export interface StreamEndpoint {
  port: number;
  path: string;
  contentType: string;
  kind: "stream" | "snapshot";
  process: string | null;
}

export interface ClusterResource {
  id: string;
  type: "node" | "qemu" | "lxc" | "storage";
  node?: string;
  vmid?: number;
  name?: string;
  status?: string;
  template?: number;
  cpu?: number;
  maxcpu?: number;
  mem?: number;
  maxmem?: number;
  disk?: number;
  maxdisk?: number;
  uptime?: number;
  storage?: string;
  netin?: number;
  netout?: number;
}

export interface TaskEntry {
  upid: string;
  node: string;
  type: string;
  status?: string;
  starttime?: number;
  endtime?: number;
  user?: string;
  id?: string;
}

export interface TaskLogLine {
  n: number;
  t: string;
}

export interface TaskStatus {
  upid: string;
  status: string;
  exitstatus?: string;
}

export interface StorageSummary {
  storage: string;
  content?: string;
  active?: number;
  avail?: number;
  total?: number;
}

export interface StorageContent {
  volid: string;
  content: string;
  format?: string;
  size?: number;
  vmid?: number;
  ctime?: number;
  notes?: string;
}

export interface BackupJob {
  id: string;
  schedule?: string;
  storage?: string;
  vmid?: string;
  all?: number;
  enabled?: number;
  mode?: string;
  node?: string;
}

export interface AccessUser {
  userid: string;
  comment?: string;
  enable?: number;
  expire?: number;
  email?: string;
}

export interface AccessDomain {
  realm: string;
  type?: string;
  comment?: string;
  default?: number;
}

export interface AclEntry {
  path: string;
  type: string;
  ugid: string;
  roleid: string;
  propagate?: number;
}

/** `/access/permissions` — the connection's *own* effective privileges, keyed
 * by ACL path then privilege name. PVE sends `1` for what is granted and omits
 * the rest, so presence is the answer. Only paths an ACL names are listed, so
 * read it through `hasPrivilege` in backup.ts rather than by direct lookup. */
export type Permissions = Record<string, Record<string, number>>;

export interface AccessRole {
  roleid: string;
  privs?: string;
  special?: number;
}

export interface StorageConfig {
  storage: string;
  type: string;
  content?: string;
  path?: string;
  server?: string;
  export?: string;
  share?: string;
  nodes?: string;
  shared?: number;
  disable?: number;
}

export interface FirewallRule {
  pos: number;
  type: string;
  action: string;
  enable?: number;
  proto?: string;
  dport?: string;
  sport?: string;
  source?: string;
  dest?: string;
  iface?: string;
  comment?: string;
}

/** Firewall scope: {} = cluster, {node} = node, {node, kind, vmid} = guest. */
export interface FirewallScope {
  node?: string;
  kind?: GuestKind;
  vmid?: number;
}

export interface ReplicationJob {
  id: string;
  type?: string;
  guest?: number;
  target?: string;
  schedule?: string;
  disable?: number;
}

/** One entry from `/nodes/{node}/network`. Everything past `type` is
 * type-dependent and optional — and all of it is declared because
 * `update_network_iface` replaces the definition wholesale, so a field the
 * edit form cannot see is a field the next save drops. */
export interface NetworkInterface {
  iface: string;
  type: string;
  method?: string;
  address?: string;
  netmask?: string;
  cidr?: string;
  gateway?: string;
  cidr6?: string;
  gateway6?: string;
  bridge_ports?: string;
  bridge_vlan_aware?: number;
  slaves?: string;
  bond_mode?: string;
  bond_xmit_hash_policy?: string;
  "vlan-id"?: number;
  "vlan-raw-device"?: string;
  mtu?: number;
  comments?: string;
  active?: number;
  autostart?: number;
}

/** `/nodes/{node}/network` in full. PVE stages edits in
 * `/etc/network/interfaces.new`; `changes` is the diff against the live file,
 * and is null when nothing is pending. */
export interface NodeNetwork {
  interfaces: NetworkInterface[];
  changes: string | null;
}

/** A guest under HA. `sid` is the HA service id, "qemu:100" / "lxc:101". */
export interface HaResource {
  sid: string;
  type?: string;
  state?: string;
  group?: string;
  comment?: string;
  max_restart?: number;
  max_relocate?: number;
}

export interface HaGroup {
  group: string;
  type?: string;
  /** Comma-separated `node[:priority]` list, e.g. "pve1:2,pve2". */
  nodes?: string;
  restricted?: number;
  nofailback?: number;
  comment?: string;
}

/** One entry of `/cluster/ha/status/current`. The list is heterogeneous —
 * `type` decides which fields the entry actually carries. */
export interface HaStatus {
  id: string;
  type?: string;
  status?: string;
  node?: string;
  quorate?: string | number;
  crm_state?: string;
  timestamp?: number;
}

/** One entry from `/nodes/{node}/ceph/pool`. `pool_name` is the name every
 * other endpoint addresses the pool by; `pool` is the numeric id. */
export interface CephPool {
  pool?: number;
  pool_name: string;
  size?: number;
  min_size?: number;
  pg_num?: number;
  /** Numeric id on older PVE, the rule name on newer. */
  crush_rule?: string | number;
  crush_rule_name?: string;
  percent_used?: number;
  bytes_used?: number;
  pg_autoscale_mode?: string;
  type?: string;
}

/** `/nodes/{node}/ceph/status`. The real payload is `ceph status --format
 * json` and is far larger than this — only the fields the UI reads are
 * declared, since the rest is version-dependent. */
export interface CephStatus {
  health?: {
    status?: string;
    checks?: Record<string, { severity?: string; summary?: { message?: string } }>;
  };
  quorum_names?: string[];
  monmap?: { mons?: { name?: string }[] };
  pgmap?: {
    num_pgs?: number;
    pgs_by_state?: { state_name: string; count: number }[];
    bytes_total?: number;
    bytes_used?: number;
    bytes_avail?: number;
  };
}

/** A CRUSH tree node from `/nodes/{node}/ceph/osd`. Buckets (root, host,
 * rack, ...) carry `children`; leaves are the OSDs themselves. */
export interface CrushNode {
  id?: number;
  name?: string;
  type?: string;
  status?: string;
  in?: number;
  device_class?: string;
  percent_used?: number;
  total_space?: number;
  bytes_used?: number;
  children?: CrushNode[];
}

export interface CephOsdTree {
  root?: CrushNode;
}

export type CephServiceKind = "mon" | "mgr" | "mds";

/** One MON/MGR/MDS daemon. The three listings share a name and a host and
 * diverge after that, so the extra fields stay untyped. */
export interface CephService {
  name?: string;
  host?: string;
  addr?: string;
  state?: string;
  quorum?: number;
  ceph_version_short?: string;
}

/** One entry from `/nodes/{node}/certificates/info`. Every field is optional
 * in the PVE schema, so a row degrades rather than the listing failing.
 * `filename` is `pve-ssl.pem` for the node's self-signed certificate and
 * `pveproxy-ssl.pem` for a custom or ACME one. */
export interface CertificateInfo {
  filename?: string;
  fingerprint?: string;
  subject?: string;
  issuer?: string;
  /** Unix epoch seconds. */
  notbefore?: number;
  notafter?: number;
  san?: string[];
  /** The certificate in PEM. The private key is never returned. */
  pem?: string;
  public_key_type?: string;
  public_key_bits?: number;
}

/** `/cluster/acme/account` — names only. */
export interface AcmeAccountEntry {
  name: string;
}

/** `/cluster/acme/account/{name}`. `account` is whatever the ACME server
 * returned at registration, so only the fields the UI shows are declared. */
export interface AcmeAccountDetail {
  directory?: string;
  location?: string;
  tos?: string;
  account?: { status?: string; contact?: string[]; createdAt?: string };
}

/** One entry from `/cluster/acme/plugins`, listed read-only — plugin
 * configuration is out of scope for #20. Fields beyond these depend on the
 * DNS API behind the plugin. */
export interface AcmePlugin {
  plugin: string;
  type?: string;
  api?: string;
  "validation-delay"?: number;
}

export interface ConsoleInfo {
  port: number;
  ticket: string;
  user?: string;
}

export interface DiscoveredHost {
  ip: string;
  host: string;
  confirmed: boolean;
  version?: string | null;
}

export interface TailscalePeer {
  name: string;
  ip: string;
  online: boolean;
  os: string;
}

export const api = {
  openConsole: (
    connectionId: string,
    node: string,
    kind: GuestKind,
    vmid: number,
    mode: "vnc" | "term",
  ) => call<ConsoleInfo>("open_console", { connectionId, node, kind, vmid, mode }),
  listConnections: () => call<ConnectionInfo[]>("list_connections"),
  saveConnection: (info: ConnectionInfo, token?: string, sshSecret?: string) =>
    call<void>("save_connection", {
      info,
      token: token || null,
      sshSecret: sshSecret || null,
    }),
  deleteConnection: (id: string) => call<void>("delete_connection", { id }),
  openSshShell: (connectionId: string) =>
    call<ConsoleInfo>("open_ssh_shell", { connectionId }),
  clusterResources: (connectionId: string) =>
    call<ClusterResource[]>("cluster_resources", { connectionId }),
  guestPower: (
    connectionId: string,
    node: string,
    kind: GuestKind,
    vmid: number,
    action: PowerAction,
  ) => call<string>("guest_power", { connectionId, node, kind, vmid, action }),
  nodeStorages: (connectionId: string, node: string) =>
    call<StorageSummary[]>("node_storages", { connectionId, node }),
  storageContent: (connectionId: string, node: string, storage: string, content?: string) =>
    call<StorageContent[]>("storage_content", {
      connectionId,
      node,
      storage,
      content: content ?? null,
    }),
  createGuest: (
    connectionId: string,
    node: string,
    kind: GuestKind,
    params: Record<string, string>,
  ) => call<string>("create_guest", { connectionId, node, kind, params }),
  /** Containers inside a guest. `null` means the guest is reachable but has
   * no `docker` — hide the section rather than showing an error. Needs the
   * connection's SSH config, plus qemu-guest-agent for a VM. */
  dockerPs: (connectionId: string, kind: GuestKind, vmid: number) =>
    call<DockerContainer[] | null>("docker_ps", { connectionId, kind, vmid }),
  dockerAction: (
    connectionId: string,
    kind: GuestKind,
    vmid: number,
    container: string,
    action: DockerAction,
  ) => call<void>("docker_action", { connectionId, kind, vmid, container, action }),
  dockerLogs: (
    connectionId: string,
    kind: GuestKind,
    vmid: number,
    container: string,
    tail: number,
  ) => call<string>("docker_logs", { connectionId, kind, vmid, container, tail }),
  /** Listening ports on an SSH host. `null` means the host has no `ss`. */
  hostPorts: (connectionId: string) =>
    call<ListeningPort[] | null>("host_ports", { connectionId }),
  /** Running and failed units on an SSH host. `null` means no `systemctl`. */
  hostServices: (connectionId: string) =>
    call<ServiceUnit[] | null>("host_services", { connectionId }),
  /** Containers on an SSH host, running or not, read-only (#105). `null` means
   * the host has no `docker`. */
  hostDockerPs: (connectionId: string) =>
    call<DockerContainer[] | null>("host_docker_ps", { connectionId }),
  /** Media endpoints among the host's listening ports (#106). `null` means the
   * host lacks `ss` or `curl`, so nothing can be probed; an empty list means it
   * was probed and serves no stream. Takes seconds — every candidate port is
   * curled with a 2s timeout, serially. */
  hostStreams: (connectionId: string) =>
    call<StreamEndpoint[] | null>("host_streams", { connectionId }),
  guestConfig: (connectionId: string, node: string, kind: GuestKind, vmid: number) =>
    call<Record<string, unknown>>("guest_config", { connectionId, node, kind, vmid }),
  setGuestConfig: (
    connectionId: string,
    node: string,
    kind: GuestKind,
    vmid: number,
    params: Record<string, string>,
  ) => call<string | null>("set_guest_config", { connectionId, node, kind, vmid, params }),
  resizeDisk: (
    connectionId: string,
    node: string,
    kind: GuestKind,
    vmid: number,
    disk: string,
    size: string,
  ) => call<string | null>("resize_disk", { connectionId, node, kind, vmid, disk, size }),
  nodeNetwork: (connectionId: string, node: string) =>
    call<NodeNetwork>("node_network", { connectionId, node }),
  createNetworkIface: (connectionId: string, node: string, params: Record<string, string>) =>
    call<void>("create_network_iface", { connectionId, node, params }),
  /** `params` must be the interface's full definition — PVE drops any key the
   * body leaves out. */
  updateNetworkIface: (
    connectionId: string,
    node: string,
    iface: string,
    params: Record<string, string>,
  ) => call<void>("update_network_iface", { connectionId, node, iface, params }),
  deleteNetworkIface: (connectionId: string, node: string, iface: string) =>
    call<void>("delete_network_iface", { connectionId, node, iface }),
  /** Runs `ifreload -a` on the node and returns the task UPID. Can drop the
   * management link if the staged config is wrong. */
  applyNetwork: (connectionId: string, node: string) =>
    call<string>("apply_network", { connectionId, node }),
  revertNetwork: (connectionId: string, node: string) =>
    call<void>("revert_network", { connectionId, node }),
  nodeTasks: (connectionId: string, node: string) =>
    call<TaskEntry[]>("node_tasks", { connectionId, node }),
  /** Starts an APT index refresh on the node and resolves with its task UPID.
   * Refreshes the index only — nothing is upgraded, no service restarts. */
  aptUpdate: (connectionId: string, node: string) =>
    call<string>("apt_update", { connectionId, node }),
  taskStatus: (connectionId: string, node: string, upid: string) =>
    call<TaskStatus>("task_status", { connectionId, node, upid }),
  taskLog: (connectionId: string, node: string, upid: string, start?: number) =>
    call<TaskLogLine[]>("task_log", { connectionId, node, upid, start: start ?? null }),
  vzdump: (connectionId: string, node: string, params: Record<string, string>) =>
    call<string>("vzdump", { connectionId, node, params }),
  deleteVolume: (connectionId: string, node: string, storage: string, volid: string) =>
    call<string | null>("delete_volume", { connectionId, node, storage, volid }),
  backupJobs: (connectionId: string) => call<BackupJob[]>("backup_jobs", { connectionId }),
  replicationJobs: (connectionId: string) =>
    call<ReplicationJob[]>("replication_jobs", { connectionId }),
  accessUsers: (connectionId: string) => call<AccessUser[]>("access_users", { connectionId }),
  addUser: (connectionId: string, params: Record<string, string>) =>
    call<void>("add_user", { connectionId, params }),
  deleteUser: (connectionId: string, userid: string) =>
    call<void>("delete_user", { connectionId, userid }),
  accessDomains: (connectionId: string) =>
    call<AccessDomain[]>("access_domains", { connectionId }),
  accessRoles: (connectionId: string) => call<AccessRole[]>("access_roles", { connectionId }),
  accessAcl: (connectionId: string) => call<AclEntry[]>("access_acl", { connectionId }),
  /** Needs no privilege of its own — a token holding none gets `{}` and a 200,
   * not a 403 — so this is safe to call for exactly the tokens whose limits
   * matter. Verified live against PVE 9.2.4. */
  accessPermissions: (connectionId: string) =>
    call<Permissions>("access_permissions", { connectionId }),
  setAcl: (connectionId: string, params: Record<string, string>) =>
    call<void>("set_acl", { connectionId, params }),
  haResources: (connectionId: string) => call<HaResource[]>("ha_resources", { connectionId }),
  addHaResource: (connectionId: string, params: Record<string, string>) =>
    call<void>("add_ha_resource", { connectionId, params }),
  updateHaResource: (connectionId: string, sid: string, params: Record<string, string>) =>
    call<void>("update_ha_resource", { connectionId, sid, params }),
  deleteHaResource: (connectionId: string, sid: string) =>
    call<void>("delete_ha_resource", { connectionId, sid }),
  haGroups: (connectionId: string) => call<HaGroup[]>("ha_groups", { connectionId }),
  addHaGroup: (connectionId: string, params: Record<string, string>) =>
    call<void>("add_ha_group", { connectionId, params }),
  updateHaGroup: (connectionId: string, group: string, params: Record<string, string>) =>
    call<void>("update_ha_group", { connectionId, group, params }),
  deleteHaGroup: (connectionId: string, group: string) =>
    call<void>("delete_ha_group", { connectionId, group }),
  haStatusCurrent: (connectionId: string) =>
    call<HaStatus[]>("ha_status_current", { connectionId }),
  /** Rejects on a node without Ceph — see `probeCeph` in ceph.ts. */
  cephStatus: (connectionId: string, node: string) =>
    call<CephStatus>("ceph_status", { connectionId, node }),
  cephOsds: (connectionId: string, node: string) =>
    call<CephOsdTree>("ceph_osds", { connectionId, node }),
  cephPools: (connectionId: string, node: string) =>
    call<CephPool[]>("ceph_pools", { connectionId, node }),
  cephServices: (connectionId: string, node: string, kind: CephServiceKind) =>
    call<CephService[]>("ceph_services", { connectionId, node, kind }),
  cephOsdInOut: (connectionId: string, node: string, osdid: number, into: boolean) =>
    call<string | null>("ceph_osd_in_out", { connectionId, node, osdid, into }),
  cephOsdPower: (connectionId: string, node: string, osdid: number, action: "start" | "stop") =>
    call<string | null>("ceph_osd_power", { connectionId, node, osdid, action }),
  cephOsdDestroy: (connectionId: string, node: string, osdid: number, cleanup: boolean) =>
    call<string | null>("ceph_osd_destroy", { connectionId, node, osdid, cleanup }),
  cephPoolCreate: (connectionId: string, node: string, params: Record<string, string>) =>
    call<string | null>("ceph_pool_create", { connectionId, node, params }),
  cephPoolUpdate: (
    connectionId: string,
    node: string,
    name: string,
    params: Record<string, string>,
  ) => call<string | null>("ceph_pool_update", { connectionId, node, name, params }),
  cephPoolDelete: (connectionId: string, node: string, name: string, removeStorages: boolean) =>
    call<string | null>("ceph_pool_delete", { connectionId, node, name, removeStorages }),
  certificatesInfo: (connectionId: string, node: string) =>
    call<CertificateInfo[]>("certificates_info", { connectionId, node }),
  /** `params`: certificates (PEM chain), key (PEM private key), force, restart.
   * The key never comes back and must not be logged on the way in. */
  uploadCertificate: (connectionId: string, node: string, params: Record<string, string>) =>
    call<CertificateInfo>("upload_certificate", { connectionId, node, params }),
  deleteCustomCertificate: (connectionId: string, node: string, restart: boolean) =>
    call<string | null>("delete_custom_certificate", { connectionId, node, restart }),
  /** Both ACME calls return a task UPID — the certificate is only in place
   * once that task finishes. */
  acmeOrderCertificate: (connectionId: string, node: string) =>
    call<string>("acme_order_certificate", { connectionId, node }),
  acmeRenewCertificate: (connectionId: string, node: string, force: boolean) =>
    call<string>("acme_renew_certificate", { connectionId, node, force }),
  acmeAccounts: (connectionId: string) =>
    call<AcmeAccountEntry[]>("acme_accounts", { connectionId }),
  acmeAccount: (connectionId: string, name: string) =>
    call<AcmeAccountDetail>("acme_account", { connectionId, name }),
  acmePlugins: (connectionId: string) => call<AcmePlugin[]>("acme_plugins", { connectionId }),
  storageConfigs: (connectionId: string) =>
    call<StorageConfig[]>("storage_configs", { connectionId }),
  addStorage: (connectionId: string, params: Record<string, string>) =>
    call<void>("add_storage", { connectionId, params }),
  deleteStorage: (connectionId: string, storage: string) =>
    call<void>("delete_storage", { connectionId, storage }),
  firewallRules: (connectionId: string, scope: FirewallScope) =>
    call<FirewallRule[]>("firewall_rules", { connectionId, ...scope }),
  addFirewallRule: (connectionId: string, scope: FirewallScope, params: Record<string, string>) =>
    call<void>("add_firewall_rule", { connectionId, ...scope, params }),
  deleteFirewallRule: (connectionId: string, scope: FirewallScope, pos: number) =>
    call<void>("delete_firewall_rule", { connectionId, ...scope, pos }),
  firewallOptions: (connectionId: string, scope: FirewallScope) =>
    call<Record<string, unknown>>("firewall_options", { connectionId, ...scope }),
  setFirewallOptions: (
    connectionId: string,
    scope: FirewallScope,
    params: Record<string, string>,
  ) => call<void>("set_firewall_options", { connectionId, ...scope, params }),
  testConnection: (opts: {
    host: string;
    token?: string;
    acceptInvalidCerts: boolean;
    connectionId?: string;
  }) =>
    call<Version>("test_connection", {
      host: opts.host,
      token: opts.token || null,
      acceptInvalidCerts: opts.acceptInvalidCerts,
      connectionId: opts.connectionId || null,
    }),
  scanLan: () => call<DiscoveredHost[]>("scan_lan"),
  scanTailscale: () => call<TailscalePeer[]>("scan_tailscale"),
};
