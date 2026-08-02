import { computed, ref, watch } from "vue";
import { api, type ClusterResource } from "../api";
import { activeId, connections, pveConnections } from "./connections";

// Cluster state for every saved connection, not just the active one. A
// connection is always a cluster of N >= 1 nodes — single-node installs are
// just a cluster of one. State is keyed by connection id so an unreachable
// cluster carries its own error while the others still render (#24).
//
// PVE connections only (#102): an SSH host has no PVE API, so
// `api.clusterResources` would always fail for it. Fetching, counting or
// listing it here would turn every saved SSH host into a permanent error
// card on the Dashboard and Guests views. `pveConnections` filters those out;
// `connections` itself is still used for the stale-state cleanup below,
// which must fire for every kind of connection, not just PVE ones.

export interface ClusterState {
  id: string;
  name: string;
  resources: ClusterResource[];
  loading: boolean;
  error: string;
}

/** A resource plus the connection it came from. The aggregate views need this
 * to route an action back to the cluster that owns the guest — vmids are
 * unique within a cluster, not across them. */
export interface OwnedResource extends ClusterResource {
  connectionId: string;
  clusterName: string;
}

export const clusters = ref<Record<string, ClusterState>>({});

function emptyState(id: string, name: string): ClusterState {
  return { id, name, resources: [], loading: false, error: "" };
}

/** Every saved connection's state, in the order the connections are listed.
 * Connections not fetched yet show up empty rather than missing, so a view
 * can render the full list before any request finishes. */
export const clusterList = computed<ClusterState[]>(() =>
  pveConnections.value.map((c) => clusters.value[c.id] ?? emptyState(c.id, c.name)),
);

/** Stamp the owning connection onto each resource of the given types. */
export function tagOwner(state: ClusterState, kinds: string[]): OwnedResource[] {
  return state.resources
    .filter((r) => kinds.includes(r.type))
    .map((r) => ({ ...r, connectionId: state.id, clusterName: state.name }));
}

export const allNodes = computed(() => clusterList.value.flatMap((s) => tagOwner(s, ["node"])));
export const allGuests = computed(() =>
  clusterList.value.flatMap((s) => tagOwner(s, ["qemu", "lxc"])),
);

/** True once more than one connection is saved — the aggregate views use it to
 * decide whether a cluster column and filter are worth the space. */
export const multiCluster = computed(() => pveConnections.value.length > 1);

const active = computed(() => (activeId.value ? clusters.value[activeId.value] : undefined));

// Active-connection accessors. Every per-cluster view (HA, Ceph, certificates,
// network, firewall, storage, ...) reads these and is unaffected by the split
// above — they still see exactly one cluster, the selected one.
export const resources = computed(() => active.value?.resources ?? []);
export const loading = computed(() => active.value?.loading ?? false);
export const error = computed(() => active.value?.error ?? "");

export const nodes = computed(() => resources.value.filter((r) => r.type === "node"));
export const guests = computed(() =>
  resources.value.filter((r) => r.type === "qemu" || r.type === "lxc"),
);
export const storages = computed(() => resources.value.filter((r) => r.type === "storage"));

export async function refreshClusterById(id: string) {
  const conn = connections.value.find((c) => c.id === id);
  if (!conn) return;
  const state = clusters.value[id] ?? emptyState(id, conn.name);
  state.name = conn.name;
  state.loading = true;
  state.error = "";
  clusters.value[id] = state;
  try {
    state.resources = await api.clusterResources(id);
  } catch (e) {
    // Keep stale data — a network switch or an offline blip shouldn't blank
    // the UI. The error shows alongside, and only for this cluster.
    state.error = String(e);
  } finally {
    state.loading = false;
  }
}

/** Refresh the active connection only — what the per-cluster views call. */
export async function refreshCluster() {
  if (activeId.value) await refreshClusterById(activeId.value);
}

/** Refresh every saved connection at once. `refreshClusterById` absorbs its
 * own failures, so one dead cluster never rejects the batch. */
export async function refreshAllClusters() {
  await Promise.all(pveConnections.value.map((c) => refreshClusterById(c.id)));
}

watch(activeId, refreshCluster);

// Drop state for connections that no longer exist, so a deleted connection
// stops appearing in the aggregate views.
watch(connections, () => {
  for (const id of Object.keys(clusters.value)) {
    if (!connections.value.some((c) => c.id === id)) delete clusters.value[id];
  }
});
