import { invoke } from "@tauri-apps/api/core";

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

export interface ConnectionInfo {
  id: string;
  name: string;
  host: string;
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
}

export type DockerAction = "start" | "stop" | "restart";

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

export interface NetworkInterface {
  iface: string;
  type: string;
  method?: string;
  address?: string;
  netmask?: string;
  cidr?: string;
  gateway?: string;
  bridge_ports?: string;
  active?: number;
  autostart?: number;
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

export const api = {
  openConsole: (
    connectionId: string,
    node: string,
    kind: GuestKind,
    vmid: number,
    mode: "vnc" | "term",
  ) => invoke<ConsoleInfo>("open_console", { connectionId, node, kind, vmid, mode }),
  listConnections: () => invoke<ConnectionInfo[]>("list_connections"),
  saveConnection: (info: ConnectionInfo, token?: string, sshSecret?: string) =>
    invoke<void>("save_connection", {
      info,
      token: token || null,
      sshSecret: sshSecret || null,
    }),
  deleteConnection: (id: string) => invoke<void>("delete_connection", { id }),
  openSshShell: (connectionId: string) =>
    invoke<ConsoleInfo>("open_ssh_shell", { connectionId }),
  clusterResources: (connectionId: string) =>
    invoke<ClusterResource[]>("cluster_resources", { connectionId }),
  guestPower: (
    connectionId: string,
    node: string,
    kind: GuestKind,
    vmid: number,
    action: PowerAction,
  ) => invoke<string>("guest_power", { connectionId, node, kind, vmid, action }),
  nodeStorages: (connectionId: string, node: string) =>
    invoke<StorageSummary[]>("node_storages", { connectionId, node }),
  storageContent: (connectionId: string, node: string, storage: string, content?: string) =>
    invoke<StorageContent[]>("storage_content", {
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
  ) => invoke<string>("create_guest", { connectionId, node, kind, params }),
  /** Containers inside a guest. `null` means the guest is reachable but has
   * no `docker` — hide the section rather than showing an error. Needs the
   * connection's SSH config, plus qemu-guest-agent for a VM. */
  dockerPs: (connectionId: string, kind: GuestKind, vmid: number) =>
    invoke<DockerContainer[] | null>("docker_ps", { connectionId, kind, vmid }),
  dockerAction: (
    connectionId: string,
    kind: GuestKind,
    vmid: number,
    container: string,
    action: DockerAction,
  ) => invoke<void>("docker_action", { connectionId, kind, vmid, container, action }),
  dockerLogs: (
    connectionId: string,
    kind: GuestKind,
    vmid: number,
    container: string,
    tail: number,
  ) => invoke<string>("docker_logs", { connectionId, kind, vmid, container, tail }),
  guestConfig: (connectionId: string, node: string, kind: GuestKind, vmid: number) =>
    invoke<Record<string, unknown>>("guest_config", { connectionId, node, kind, vmid }),
  setGuestConfig: (
    connectionId: string,
    node: string,
    kind: GuestKind,
    vmid: number,
    params: Record<string, string>,
  ) => invoke<string | null>("set_guest_config", { connectionId, node, kind, vmid, params }),
  resizeDisk: (
    connectionId: string,
    node: string,
    kind: GuestKind,
    vmid: number,
    disk: string,
    size: string,
  ) => invoke<string | null>("resize_disk", { connectionId, node, kind, vmid, disk, size }),
  nodeNetwork: (connectionId: string, node: string) =>
    invoke<NetworkInterface[]>("node_network", { connectionId, node }),
  nodeTasks: (connectionId: string, node: string) =>
    invoke<TaskEntry[]>("node_tasks", { connectionId, node }),
  taskStatus: (connectionId: string, node: string, upid: string) =>
    invoke<TaskStatus>("task_status", { connectionId, node, upid }),
  taskLog: (connectionId: string, node: string, upid: string, start?: number) =>
    invoke<TaskLogLine[]>("task_log", { connectionId, node, upid, start: start ?? null }),
  vzdump: (connectionId: string, node: string, params: Record<string, string>) =>
    invoke<string>("vzdump", { connectionId, node, params }),
  deleteVolume: (connectionId: string, node: string, storage: string, volid: string) =>
    invoke<string | null>("delete_volume", { connectionId, node, storage, volid }),
  backupJobs: (connectionId: string) => invoke<BackupJob[]>("backup_jobs", { connectionId }),
  replicationJobs: (connectionId: string) =>
    invoke<ReplicationJob[]>("replication_jobs", { connectionId }),
  accessUsers: (connectionId: string) => invoke<AccessUser[]>("access_users", { connectionId }),
  addUser: (connectionId: string, params: Record<string, string>) =>
    invoke<void>("add_user", { connectionId, params }),
  deleteUser: (connectionId: string, userid: string) =>
    invoke<void>("delete_user", { connectionId, userid }),
  accessDomains: (connectionId: string) =>
    invoke<AccessDomain[]>("access_domains", { connectionId }),
  accessRoles: (connectionId: string) => invoke<AccessRole[]>("access_roles", { connectionId }),
  accessAcl: (connectionId: string) => invoke<AclEntry[]>("access_acl", { connectionId }),
  setAcl: (connectionId: string, params: Record<string, string>) =>
    invoke<void>("set_acl", { connectionId, params }),
  haResources: (connectionId: string) => invoke<HaResource[]>("ha_resources", { connectionId }),
  addHaResource: (connectionId: string, params: Record<string, string>) =>
    invoke<void>("add_ha_resource", { connectionId, params }),
  updateHaResource: (connectionId: string, sid: string, params: Record<string, string>) =>
    invoke<void>("update_ha_resource", { connectionId, sid, params }),
  deleteHaResource: (connectionId: string, sid: string) =>
    invoke<void>("delete_ha_resource", { connectionId, sid }),
  haGroups: (connectionId: string) => invoke<HaGroup[]>("ha_groups", { connectionId }),
  addHaGroup: (connectionId: string, params: Record<string, string>) =>
    invoke<void>("add_ha_group", { connectionId, params }),
  updateHaGroup: (connectionId: string, group: string, params: Record<string, string>) =>
    invoke<void>("update_ha_group", { connectionId, group, params }),
  deleteHaGroup: (connectionId: string, group: string) =>
    invoke<void>("delete_ha_group", { connectionId, group }),
  haStatusCurrent: (connectionId: string) =>
    invoke<HaStatus[]>("ha_status_current", { connectionId }),
  /** Rejects on a node without Ceph — see `probeCeph` in ceph.ts. */
  cephStatus: (connectionId: string, node: string) =>
    invoke<CephStatus>("ceph_status", { connectionId, node }),
  cephOsds: (connectionId: string, node: string) =>
    invoke<CephOsdTree>("ceph_osds", { connectionId, node }),
  cephPools: (connectionId: string, node: string) =>
    invoke<CephPool[]>("ceph_pools", { connectionId, node }),
  cephServices: (connectionId: string, node: string, kind: CephServiceKind) =>
    invoke<CephService[]>("ceph_services", { connectionId, node, kind }),
  cephOsdInOut: (connectionId: string, node: string, osdid: number, into: boolean) =>
    invoke<string | null>("ceph_osd_in_out", { connectionId, node, osdid, into }),
  cephOsdPower: (connectionId: string, node: string, osdid: number, action: "start" | "stop") =>
    invoke<string | null>("ceph_osd_power", { connectionId, node, osdid, action }),
  cephOsdDestroy: (connectionId: string, node: string, osdid: number, cleanup: boolean) =>
    invoke<string | null>("ceph_osd_destroy", { connectionId, node, osdid, cleanup }),
  cephPoolCreate: (connectionId: string, node: string, params: Record<string, string>) =>
    invoke<string | null>("ceph_pool_create", { connectionId, node, params }),
  cephPoolUpdate: (
    connectionId: string,
    node: string,
    name: string,
    params: Record<string, string>,
  ) => invoke<string | null>("ceph_pool_update", { connectionId, node, name, params }),
  cephPoolDelete: (connectionId: string, node: string, name: string, removeStorages: boolean) =>
    invoke<string | null>("ceph_pool_delete", { connectionId, node, name, removeStorages }),
  certificatesInfo: (connectionId: string, node: string) =>
    invoke<CertificateInfo[]>("certificates_info", { connectionId, node }),
  /** `params`: certificates (PEM chain), key (PEM private key), force, restart.
   * The key never comes back and must not be logged on the way in. */
  uploadCertificate: (connectionId: string, node: string, params: Record<string, string>) =>
    invoke<CertificateInfo>("upload_certificate", { connectionId, node, params }),
  deleteCustomCertificate: (connectionId: string, node: string, restart: boolean) =>
    invoke<string | null>("delete_custom_certificate", { connectionId, node, restart }),
  /** Both ACME calls return a task UPID — the certificate is only in place
   * once that task finishes. */
  acmeOrderCertificate: (connectionId: string, node: string) =>
    invoke<string>("acme_order_certificate", { connectionId, node }),
  acmeRenewCertificate: (connectionId: string, node: string, force: boolean) =>
    invoke<string>("acme_renew_certificate", { connectionId, node, force }),
  acmeAccounts: (connectionId: string) =>
    invoke<AcmeAccountEntry[]>("acme_accounts", { connectionId }),
  acmeAccount: (connectionId: string, name: string) =>
    invoke<AcmeAccountDetail>("acme_account", { connectionId, name }),
  acmePlugins: (connectionId: string) => invoke<AcmePlugin[]>("acme_plugins", { connectionId }),
  storageConfigs: (connectionId: string) =>
    invoke<StorageConfig[]>("storage_configs", { connectionId }),
  addStorage: (connectionId: string, params: Record<string, string>) =>
    invoke<void>("add_storage", { connectionId, params }),
  deleteStorage: (connectionId: string, storage: string) =>
    invoke<void>("delete_storage", { connectionId, storage }),
  firewallRules: (connectionId: string, scope: FirewallScope) =>
    invoke<FirewallRule[]>("firewall_rules", { connectionId, ...scope }),
  addFirewallRule: (connectionId: string, scope: FirewallScope, params: Record<string, string>) =>
    invoke<void>("add_firewall_rule", { connectionId, ...scope, params }),
  deleteFirewallRule: (connectionId: string, scope: FirewallScope, pos: number) =>
    invoke<void>("delete_firewall_rule", { connectionId, ...scope, pos }),
  firewallOptions: (connectionId: string, scope: FirewallScope) =>
    invoke<Record<string, unknown>>("firewall_options", { connectionId, ...scope }),
  setFirewallOptions: (
    connectionId: string,
    scope: FirewallScope,
    params: Record<string, string>,
  ) => invoke<void>("set_firewall_options", { connectionId, ...scope, params }),
  testConnection: (opts: {
    host: string;
    token?: string;
    acceptInvalidCerts: boolean;
    connectionId?: string;
  }) =>
    invoke<Version>("test_connection", {
      host: opts.host,
      token: opts.token || null,
      acceptInvalidCerts: opts.acceptInvalidCerts,
      connectionId: opts.connectionId || null,
    }),
};
