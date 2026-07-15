<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api, type ClusterResource, type PowerAction } from "../api";
import { formatBytes, percent } from "../format";
import { activeId } from "../stores/connections";
import { error, guests, loading, refreshCluster } from "../stores/cluster";
import { toast } from "../stores/toast";

const nodeFilter = ref("");
const pending = ref(new Set<number>());

const nodeNames = computed(() =>
  [...new Set(guests.value.map((g) => g.node).filter(Boolean))].sort(),
);

const filtered = computed(() =>
  guests.value
    .filter((g) => !nodeFilter.value || g.node === nodeFilter.value)
    .sort((a, b) => (a.vmid ?? 0) - (b.vmid ?? 0)),
);

function actionsFor(g: ClusterResource): PowerAction[] {
  if (g.template) return [];
  return g.status === "running" ? ["shutdown", "reboot", "stop"] : ["start"];
}

async function power(g: ClusterResource, action: PowerAction) {
  if (!activeId.value || !g.node || g.vmid == null) return;
  pending.value = new Set(pending.value).add(g.vmid);
  try {
    await api.guestPower(activeId.value, g.node, g.type as "qemu" | "lxc", g.vmid, action);
    toast(`${action} sent to ${g.vmid}`);
    // Status flips async on the server; refresh shortly after.
    setTimeout(refreshCluster, 1500);
  } catch (e) {
    toast(String(e), "error");
  } finally {
    const next = new Set(pending.value);
    next.delete(g.vmid);
    pending.value = next;
  }
}

onMounted(refreshCluster);
</script>

<template>
  <div>
    <div class="head">
      <h1>VMs &amp; Containers</h1>
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
      <button @click="refreshCluster">
        Refresh
      </button>
      <router-link to="/guests/new">
        Create
      </router-link>
    </div>

    <p v-if="!activeId">
      No active connection. Add one under Connections.
    </p>
    <p v-else-if="loading">
      Loading…
    </p>
    <p
      v-else-if="error"
      class="error"
    >
      {{ error }}
    </p>
    <table v-if="activeId && !loading && filtered.length > 0">
      <thead>
        <tr>
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
          :key="g.id"
        >
          <td>{{ g.vmid }}</td>
          <td>
            <router-link :to="`/guests/${g.node}/${g.type}/${g.vmid}`">
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
              :disabled="pending.has(g.vmid ?? -1)"
              @click="power(g, a)"
            >
              {{ a }}
            </button>
          </td>
        </tr>
      </tbody>
    </table>

    <p v-else-if="activeId && !loading && !error">
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
