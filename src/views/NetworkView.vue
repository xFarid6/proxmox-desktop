<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { api, type NetworkInterface } from "../api";
import { activeId } from "../stores/connections";
import { nodes, refreshCluster } from "../stores/cluster";

const node = ref("");
const interfaces = ref<NetworkInterface[]>([]);
const loading = ref(false);
const error = ref("");

async function refreshNetwork() {
  if (!activeId.value || !node.value) {
    interfaces.value = [];
    return;
  }
  loading.value = true;
  error.value = "";
  try {
    const list = await api.nodeNetwork(activeId.value, node.value);
    interfaces.value = list.sort((a, b) => a.iface.localeCompare(b.iface));
  } catch (e) {
    error.value = String(e);
    interfaces.value = [];
  } finally {
    loading.value = false;
  }
}

onMounted(async () => {
  if (nodes.value.length === 0) await refreshCluster();
  if (!node.value && nodes.value.length > 0) node.value = nodes.value[0].node ?? "";
});

watch(node, refreshNetwork);
watch(activeId, () => {
  node.value = "";
  interfaces.value = [];
});
</script>

<template>
  <div>
    <div class="head">
      <h1>Network</h1>
      <span class="badge">read-only</span>
      <label>
        Node
        <select v-model="node">
          <option
            v-for="n in nodes"
            :key="n.id"
            :value="n.node"
          >
            {{ n.node }}
          </option>
        </select>
      </label>
      <button @click="refreshNetwork">
        Refresh
      </button>
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

    <table
      v-else-if="interfaces.length > 0"
      v-cards
    >
      <thead>
        <tr>
          <th>Interface</th>
          <th>Type</th>
          <th>Active</th>
          <th>Autostart</th>
          <th>Method</th>
          <th>CIDR</th>
          <th>Gateway</th>
          <th>Bridge ports</th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="i in interfaces"
          :key="i.iface"
        >
          <td>{{ i.iface }}</td>
          <td>{{ i.type }}</td>
          <td>
            <span :class="i.active ? 'ok' : 'off'">{{ i.active ? "yes" : "no" }}</span>
          </td>
          <td>{{ i.autostart ? "yes" : "no" }}</td>
          <td>{{ i.method ?? "—" }}</td>
          <td>{{ i.cidr ?? (i.address ? `${i.address}/${i.netmask ?? "?"}` : "—") }}</td>
          <td>{{ i.gateway ?? "—" }}</td>
          <td>{{ i.bridge_ports ?? "—" }}</td>
        </tr>
      </tbody>
    </table>

    <p v-else-if="node && !loading">
      No interfaces reported on {{ node }}.
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

.badge {
  font-size: 0.8em;
  padding: 2px 8px;
  border-radius: 10px;
  background: #8883;
  opacity: 0.8;
}

table {
  width: 100%;
  border-collapse: collapse;
}

th,
td {
  text-align: left;
  padding: 6px 10px;
  border-bottom: 1px solid #ccc3;
}

.ok {
  color: #2a7;
}

.off {
  opacity: 0.6;
}

.error {
  color: #c33;
}
</style>
