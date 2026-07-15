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

export const api = {
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
