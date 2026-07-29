<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import UsageBar from "../components/UsageBar.vue";
import { api, type HaStatus } from "../api";
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

// HA summary (#18). A cluster without HA answers empty or errors outright —
// both mean "no HA here", so the card simply does not render.
const ha = ref<HaStatus[]>([]);
const quorum = computed(() => ha.value.find((e) => e.type === "quorum"));
const master = computed(() => ha.value.find((e) => e.type === "master"));
const services = computed(() => ha.value.filter((e) => e.type === "service"));

async function loadHa() {
  if (!activeId.value) return;
  try {
    ha.value = await api.haStatusCurrent(activeId.value);
  } catch {
    ha.value = [];
  }
}

onMounted(() => {
  void refreshCluster();
  void loadHa();
});
</script>

<template>
  <div>
    <h1>Dashboard</h1>

    <router-link
      v-if="ha.length > 0"
      to="/ha"
      class="ha-line"
    >
      <strong>HA</strong>
      <span :class="Number(quorum?.quorate) === 1 ? 'ok' : 'bad'">
        quorum {{ Number(quorum?.quorate) === 1 ? "OK" : "lost" }}
      </span>
      <span>master {{ master?.node ?? "none" }}</span>
      <span>{{ services.length }} service{{ services.length === 1 ? "" : "s" }}</span>
    </router-link>

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
          <router-link
            class="ssh-link"
            :to="`/nodes/${n.node}/ssh`"
          >
            SSH shell
          </router-link>
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

.ssh-link {
  font-size: 0.8em;
  margin-left: auto;
}

.meta {
  display: flex;
  gap: 12px;
  font-size: 0.8em;
  opacity: 0.7;
}

.ha-line {
  display: flex;
  gap: 14px;
  align-items: center;
  margin-bottom: 16px;
  padding: 8px 14px;
  border: 1px solid #ccc3;
  border-radius: 8px;
  font-size: 0.85em;
}

.ok {
  color: #2a7;
}

.bad {
  color: #c33;
}

.error {
  color: #c33;
}
</style>
