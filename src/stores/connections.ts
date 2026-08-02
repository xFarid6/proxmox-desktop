import { computed, ref } from "vue";
import { api, type ConnectionInfo } from "../api";

// Module-singleton store — one window, no pinia needed.
export const connections = ref<ConnectionInfo[]>([]);
export const activeId = ref<string | null>(localStorage.getItem("activeConnectionId"));

/** Connections with a PVE API — everything except SSH-only hosts (#102). */
export const pveConnections = computed(() => connections.value.filter((c) => c.kind !== "ssh"));

/** Connections that are a plain SSH host, no PVE API. */
export const sshHosts = computed(() => connections.value.filter((c) => c.kind === "ssh"));

export const activeConnection = computed(() =>
  connections.value.find((c) => c.id === activeId.value),
);

export function setActive(id: string | null) {
  activeId.value = id;
  if (id) localStorage.setItem("activeConnectionId", id);
  else localStorage.removeItem("activeConnectionId");
}

export async function refreshConnections() {
  connections.value = await api.listConnections();
  // Drop a stale active id; default to the only connection.
  if (activeId.value && !connections.value.some((c) => c.id === activeId.value)) {
    setActive(null);
  }
  if (!activeId.value && connections.value.length > 0) {
    setActive(connections.value[0].id);
  }
}
