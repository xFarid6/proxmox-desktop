<script setup lang="ts">
import { computed, onMounted } from "vue";
import UsageBar from "../components/UsageBar.vue";
import { formatBytes, formatUptime, percent } from "../format";
import { activeId } from "../stores/connections";
import { error, guests, loading, nodes, refreshCluster } from "../stores/cluster";

// Node rows in /cluster/resources carry no network counters; guests do.
// Per-node network = sum over its guests — good enough at a glance.
const netByNode = computed(() => {
  const m = new Map<string, { netin: number; netout: number }>();
  for (const g of guests.value) {
    if (!g.node) continue;
    const cur = m.get(g.node) ?? { netin: 0, netout: 0 };
    cur.netin += g.netin ?? 0;
    cur.netout += g.netout ?? 0;
    m.set(g.node, cur);
  }
  return m;
});

onMounted(refreshCluster);
</script>

<template>
  <div>
    <h1>Dashboard</h1>

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

    <div
      v-else
      class="grid"
    >
      <div
        v-for="n in nodes"
        :key="n.id"
        class="node-card"
      >
        <div class="node-head">
          <strong>{{ n.node }}</strong>
          <span
            class="status"
            :class="n.status"
          >{{ n.status }}</span>
        </div>
        <UsageBar
          label="CPU"
          :value="Math.round((n.cpu ?? 0) * 100)"
          :detail="`${((n.cpu ?? 0) * 100).toFixed(1)}% of ${n.maxcpu ?? '?'} cores`"
        />
        <UsageBar
          label="RAM"
          :value="percent(n.mem, n.maxmem)"
          :detail="`${formatBytes(n.mem)} / ${formatBytes(n.maxmem)}`"
        />
        <UsageBar
          label="Disk"
          :value="percent(n.disk, n.maxdisk)"
          :detail="`${formatBytes(n.disk)} / ${formatBytes(n.maxdisk)}`"
        />
        <div class="meta">
          <span>Net in {{ formatBytes(netByNode.get(n.node ?? "")?.netin) }}</span>
          <span>Net out {{ formatBytes(netByNode.get(n.node ?? "")?.netout) }}</span>
          <span>Up {{ formatUptime(n.uptime) }}</span>
        </div>
      </div>

      <p v-if="nodes.length === 0">
        No nodes visible on this connection.
      </p>
    </div>
  </div>
</template>

<style scoped>
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 16px;
}

.node-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px 16px;
  border: 1px solid #ccc3;
  border-radius: 8px;
}

.node-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.status {
  font-size: 0.8em;
  padding: 2px 8px;
  border-radius: 10px;
  background: #8883;
}

.status.online {
  background: #2a72;
  color: #2a7;
}

.meta {
  display: flex;
  gap: 12px;
  font-size: 0.8em;
  opacity: 0.7;
}

.error {
  color: #c33;
}
</style>
