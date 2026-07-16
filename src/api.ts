import { invoke } from "@tauri-apps/api/core";

export interface ConnectionInfo {
  id: string;
  name: string;
  host: string;
  acceptInvalidCerts: boolean;
}

export interface Version {
  version: string;
  release: string;
}

export type GuestKind = "qemu" | "lxc";
export type PowerAction = "start" | "stop" | "reboot" | "shutdown";

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
  saveConnection: (info: ConnectionInfo, token?: string) =>
    invoke<void>("save_connection", { info, token: token || null }),
  deleteConnection: (id: string) => invoke<void>("delete_connection", { id }),
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
