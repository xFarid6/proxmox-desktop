import { computed, ref, watch } from "vue";
import { api, type ClusterResource } from "../api";
import { activeId } from "./connections";

// Cluster state for the active connection. A connection is always a cluster
// of N >= 1 nodes — single-node installs are just a cluster of one.
export const resources = ref<ClusterResource[]>([]);
export const loading = ref(false);
export const error = ref("");

export const nodes = computed(() => resources.value.filter((r) => r.type === "node"));
export const guests = computed(() =>
  resources.value.filter((r) => r.type === "qemu" || r.type === "lxc"),
);
export const storages = computed(() => resources.value.filter((r) => r.type === "storage"));

export async function refreshCluster() {
  if (!activeId.value) {
    resources.value = [];
    return;
  }
  loading.value = true;
  error.value = "";
  try {
    resources.value = await api.clusterResources(activeId.value);
  } catch (e) {
    // Keep stale data — a network switch or offline blip shouldn't blank
    // the UI; the error banner shows alongside.
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

watch(activeId, refreshCluster);
