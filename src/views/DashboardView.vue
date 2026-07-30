<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import UsageBar from "../components/UsageBar.vue";
import { api, type HaStatus } from "../api";
import { formatBytes, formatUptime, percent } from "../format";
import { hiddenStatsHint, statsHidden } from "../privileges";
import { connections, setActive } from "../stores/connections";
import {
  allGuests,
  clusterList,
  multiCluster,
  refreshAllClusters,
  type ClusterState,
} from "../stores/cluster";

// Every saved connection is shown, grouped by cluster (#24). An unreachable
// cluster renders its own error and the rest still draw.

// Node rows in /cluster/resources carry no network counters; guests do.
// Per-node network = sum over its guests — good enough at a glance. Keyed by
// connection *and* node, since two clusters can both have a node called pve1.
const netByNode = computed(() => {
  const m = new Map<string, { netin: number; netout: number }>();
  for (const g of allGuests.value) {
    if (!g.node) continue;
    const key = `${g.connectionId}/${g.node}`;
    const cur = m.get(key) ?? { netin: 0, netout: 0 };
    cur.netin += g.netin ?? 0;
    cur.netout += g.netout ?? 0;
    m.set(key, cur);
  }
  return m;
});

function netFor(connectionId: string, node?: string) {
  return netByNode.value.get(`${connectionId}/${node ?? ""}`);
}

// HA summary (#18), per cluster. A cluster without HA answers empty or errors
// outright — both mean "no HA here", so its line simply does not render.
const ha = ref<Record<string, HaStatus[]>>({});

function haEntry(id: string, type: string) {
  return (ha.value[id] ?? []).find((e) => e.type === type);
}

function haServices(id: string) {
  return (ha.value[id] ?? []).filter((e) => e.type === "service");
}

async function loadHa() {
  await Promise.all(
    connections.value.map(async (c) => {
      try {
        ha.value = { ...ha.value, [c.id]: await api.haStatusCurrent(c.id) };
      } catch {
        ha.value = { ...ha.value, [c.id]: [] };
      }
    }),
  );
}

function clusterNodes(c: ClusterState) {
  return c.resources.filter((r) => r.type === "node");
}

/** Guest and node links are resolved against the active connection, so
 * following one out of a cluster group has to switch clusters first. */
function open(connectionId: string) {
  setActive(connectionId);
}

onMounted(() => {
  void refreshAllClusters();
  void loadHa();
});
</script>

<template>
  <div>
    <div class="head">
      <h1>Dashboard</h1>
      <button @click="refreshAllClusters">
        Refresh all
      </button>
    </div>

    <p v-if="connections.length === 0">
      No connections yet. Add one under Connections.
    </p>

    <section
      v-for="c in clusterList"
      :key="c.id"
      class="cluster"
    >
      <div
        v-if="multiCluster"
        class="cluster-head"
      >
        <h2>{{ c.name }}</h2>
        <span
          v-if="c.loading"
          class="hint"
        >loading…</span>
        <span
          v-else-if="c.error"
          class="status offline"
        >unreachable</span>
        <span
          v-else
          class="hint"
        >{{ clusterNodes(c).length }} node{{ clusterNodes(c).length === 1 ? "" : "s" }}</span>
      </div>

      <router-link
        v-if="(ha[c.id] ?? []).length > 0"
        to="/ha"
        class="ha-line"
        @click="open(c.id)"
      >
        <strong>HA</strong>
        <span :class="Number(haEntry(c.id, 'quorum')?.quorate) === 1 ? 'ok' : 'bad'">
          quorum {{ Number(haEntry(c.id, "quorum")?.quorate) === 1 ? "OK" : "lost" }}
        </span>
        <span>master {{ haEntry(c.id, "master")?.node ?? "none" }}</span>
        <span>
          {{ haServices(c.id).length }}
          service{{ haServices(c.id).length === 1 ? "" : "s" }}
        </span>
      </router-link>

      <p
        v-if="c.error"
        class="error"
      >
        {{ c.error }}
      </p>
      <p v-else-if="c.loading && clusterNodes(c).length === 0">
        Loading…
      </p>

      <div class="grid">
        <div
          v-for="n in clusterNodes(c)"
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
              @click="open(c.id)"
            >
              SSH shell
            </router-link>
          </div>
          <!-- Blank usage bars on an online node mean the token cannot read
               them, not that the node is idle (#87). -->
          <p
            v-if="statsHidden(n)"
            class="hint-block"
          >
            {{ hiddenStatsHint(n.node) }}
          </p>
          <template v-else>
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
          </template>
          <div
            v-if="!statsHidden(n)"
            class="meta"
          >
            <span>Net in {{ formatBytes(netFor(c.id, n.node)?.netin) }}</span>
            <span>Net out {{ formatBytes(netFor(c.id, n.node)?.netout) }}</span>
            <span>Up {{ formatUptime(n.uptime) }}</span>
          </div>
        </div>
      </div>

      <p v-if="!c.loading && !c.error && clusterNodes(c).length === 0">
        No nodes visible on this connection.
      </p>
    </section>
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

.cluster + .cluster {
  margin-top: 28px;
}

.cluster-head {
  display: flex;
  align-items: baseline;
  gap: 12px;
  margin-bottom: 10px;
}

.cluster-head h2 {
  margin: 0;
}

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

.status.offline {
  background: #c332;
  color: #c33;
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

.hint {
  font-size: 0.85em;
  opacity: 0.7;
}

.hint-block {
  margin: 0;
  font-size: 0.85em;
  line-height: 1.4;
  opacity: 0.8;
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
