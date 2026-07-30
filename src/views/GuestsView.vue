<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api, type PowerAction } from "../api";
import { formatBytes, percent } from "../format";
import { connections, setActive } from "../stores/connections";
import {
  allGuests,
  clusterList,
  multiCluster,
  refreshAllClusters,
  type OwnedResource,
} from "../stores/cluster";
import { toast } from "../stores/toast";

// Guests from every saved connection in one list (#24). Each row carries the
// connection it came from, so every action routes to the owning cluster.

const clusterFilter = ref("");
const nodeFilter = ref("");
/** Keyed by connection + vmid: vmids are unique within a cluster, not across. */
const pending = ref(new Set<string>());

const anyLoading = computed(() => clusterList.value.some((c) => c.loading));
/** Per-cluster failures — one unreachable cluster must not hide the others. */
const failures = computed(() => clusterList.value.filter((c) => c.error));

const nodeNames = computed(() =>
  [
    ...new Set(
      allGuests.value
        .filter((g) => !clusterFilter.value || g.connectionId === clusterFilter.value)
        .map((g) => g.node)
        .filter(Boolean),
    ),
  ].sort(),
);

const filtered = computed(() =>
  allGuests.value
    .filter((g) => !clusterFilter.value || g.connectionId === clusterFilter.value)
    .filter((g) => !nodeFilter.value || g.node === nodeFilter.value)
    .sort(
      (a, b) =>
        a.clusterName.localeCompare(b.clusterName) || (a.vmid ?? 0) - (b.vmid ?? 0),
    ),
);

function key(g: OwnedResource): string {
  return `${g.connectionId} ${g.vmid}`;
}

function actionsFor(g: OwnedResource): PowerAction[] {
  if (g.template) return [];
  return g.status === "running" ? ["shutdown", "reboot", "stop"] : ["start"];
}

/** The guest detail view resolves its route against the active connection, so
 * opening a guest from another cluster has to switch to it first. */
function open(g: OwnedResource) {
  setActive(g.connectionId);
}

async function power(g: OwnedResource, action: PowerAction) {
  if (!g.node || g.vmid == null) return;
  pending.value = new Set(pending.value).add(key(g));
  try {
    await api.guestPower(g.connectionId, g.node, g.type as "qemu" | "lxc", g.vmid, action);
    toast(`${action} sent to ${g.vmid} on ${g.clusterName}`);
    // Status flips async on the server; refresh shortly after.
    setTimeout(() => void refreshAllClusters(), 1500);
  } catch (e) {
    toast(String(e), "error");
  } finally {
    const next = new Set(pending.value);
    next.delete(key(g));
    pending.value = next;
  }
}

onMounted(refreshAllClusters);
</script>

<template>
  <div>
    <div class="head">
      <h1>VMs &amp; Containers</h1>
      <label v-if="multiCluster">
        Cluster
        <select
          v-model="clusterFilter"
          @change="nodeFilter = ''"
        >
          <option value="">
            all
          </option>
          <option
            v-for="c in clusterList"
            :key="c.id"
            :value="c.id"
          >
            {{ c.name }}
          </option>
        </select>
      </label>
      <label v-if="nodeNames.length > 1">
        Node
        <select v-model="nodeFilter">
          <option value="">
            all
          </option>
          <option
            v-for="n in nodeNames"
            :key="n"
            :value="n"
          >
            {{ n }}
          </option>
        </select>
      </label>
      <button @click="refreshAllClusters">
        Refresh
      </button>
      <router-link to="/guests/new">
        Create
      </router-link>
    </div>

    <p v-if="connections.length === 0">
      No connections yet. Add one under Connections.
    </p>
    <p v-else-if="anyLoading && filtered.length === 0">
      Loading…
    </p>

    <p
      v-for="c in failures"
      :key="c.id"
      class="error"
    >
      {{ c.name }}: {{ c.error }}
    </p>

    <table
      v-if="filtered.length > 0"
      v-cards
    >
      <thead>
        <tr>
          <th v-if="multiCluster">
            Cluster
          </th>
          <th>ID</th>
          <th>Name</th>
          <th>Type</th>
          <th>Node</th>
          <th>Status</th>
          <th>CPU</th>
          <th>RAM</th>
          <th />
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="g in filtered"
          :key="`${g.connectionId} ${g.id}`"
        >
          <td v-if="multiCluster">
            {{ g.clusterName }}
          </td>
          <td>{{ g.vmid }}</td>
          <td>
            <router-link
              :to="`/guests/${g.node}/${g.type}/${g.vmid}`"
              @click="open(g)"
            >
              {{ g.name ?? g.vmid }}
            </router-link>
          </td>
          <td>{{ g.type === "qemu" ? "VM" : "CT" }}{{ g.template ? " (template)" : "" }}</td>
          <td>{{ g.node }}</td>
          <td>
            <span
              class="status"
              :class="g.status"
            >{{ g.status }}</span>
          </td>
          <td>{{ g.status === "running" ? `${((g.cpu ?? 0) * 100).toFixed(0)}%` : "—" }}</td>
          <td>{{ g.status === "running" ? `${percent(g.mem, g.maxmem)}% of ${formatBytes(g.maxmem)}` : "—" }}</td>
          <td class="actions">
            <button
              v-for="a in actionsFor(g)"
              :key="a"
              :disabled="pending.has(key(g))"
              @click="power(g, a)"
            >
              {{ a }}
            </button>
          </td>
        </tr>
      </tbody>
    </table>

    <p v-else-if="connections.length > 0 && !anyLoading">
      No guests found.
    </p>
  </div>
</template>

<style scoped>
.head {
  display: flex;
  align-items: center;
  gap: 16px;
}

.head h1 {
  margin-right: auto;
}

table {
  width: 100%;
  border-collapse: collapse;
}

th,
td {
  text-align: left;
  padding: 8px 10px;
  border-bottom: 1px solid #ccc3;
}

.status {
  font-size: 0.85em;
  padding: 2px 8px;
  border-radius: 10px;
  background: #8883;
}

.status.running {
  background: #2a72;
  color: #2a7;
}

.actions {
  display: flex;
  gap: 4px;
}

.error {
  color: #c33;
}
</style>
