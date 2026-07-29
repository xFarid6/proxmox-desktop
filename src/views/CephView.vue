<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { api, type CephPool, type CephService, type CephStatus } from "../api";
import UsageBar from "../components/UsageBar.vue";
import {
  cephAvailable,
  flattenOsdTree,
  healthClass,
  osdPercent,
  pgStates,
  poolPercent,
  probeCeph,
  type OsdRow,
} from "../ceph";
import { formatBytes, percent } from "../format";
import { nodes, refreshCluster } from "../stores/cluster";
import { activeId } from "../stores/connections";
import { toast } from "../stores/toast";

const node = ref("");
const status = ref<CephStatus | null>(null);
const osds = ref<OsdRow[]>([]);
const pools = ref<CephPool[]>([]);
const daemons = ref<Record<"mon" | "mgr" | "mds", CephService[]>>({ mon: [], mgr: [], mds: [] });
const loading = ref(false);
const error = ref("");

// Destructive-action opt-ins. Never remembered across a refresh, never
// defaulted on: cleanup wipes the disk, remove_storages drops PVE storages.
const cleanup = ref(false);
const removeStorages = ref(false);
const confirmDestroy = ref<number | null>(null);
const confirmDeletePool = ref("");

// Pool create form. The defaults are Proxmox's own for a replicated pool.
const newName = ref("");
const newSize = ref("3");
const newMinSize = ref("2");
const newPgNum = ref("128");
const newCrushRule = ref("");
const newAddStorages = ref(false);

// Per-row pending pool edits, keyed by pool name.
type PoolEdit = { size: string; min_size: string; pg_num: string };
const poolEdits = ref<Record<string, PoolEdit>>({});

const health = computed(() => status.value?.health?.status);
const checks = computed(() =>
  Object.entries(status.value?.health?.checks ?? {}).map(([code, c]) => ({
    code,
    severity: c.severity ?? "",
    message: c.summary?.message ?? "",
  })),
);
const quorum = computed(() => status.value?.quorum_names ?? []);
const monCount = computed(() => status.value?.monmap?.mons?.length ?? quorum.value.length);
const capacity = computed(() => status.value?.pgmap);

function poolEditOf(p: CephPool): PoolEdit {
  return {
    size: String(p.size ?? ""),
    min_size: String(p.min_size ?? ""),
    pg_num: String(p.pg_num ?? ""),
  };
}

function poolDirty(p: CephPool): boolean {
  const e = poolEdits.value[p.pool_name];
  if (!e) return false;
  const o = poolEditOf(p);
  return e.size !== o.size || e.min_size !== o.min_size || e.pg_num !== o.pg_num;
}

/** Start/stop and destroy have to be sent to the node the OSD actually lives
 * on — they touch that node's systemd unit and disk. In/out are monmap edits
 * and work from any node. */
function osdNode(osd: OsdRow): string {
  return osd.host !== "—" ? osd.host : node.value;
}

async function refresh() {
  error.value = "";
  confirmDestroy.value = null;
  confirmDeletePool.value = "";
  cleanup.value = removeStorages.value = false;
  if (!activeId.value) {
    status.value = null;
    osds.value = [];
    pools.value = [];
    return;
  }
  void refreshCluster();
  if (!node.value || !nodes.value.some((n) => n.node === node.value)) {
    node.value = nodes.value[0]?.node ?? "";
  }
  // Re-probe on every explicit refresh so installing Ceph shows up without
  // restarting the app.
  await probeCeph(activeId.value, node.value, true);
  if (!cephAvailable.value || !node.value) return;
  loading.value = true;
  try {
    const [st, tree, pl, mon, mgr, mds] = await Promise.all([
      api.cephStatus(activeId.value, node.value),
      api.cephOsds(activeId.value, node.value),
      api.cephPools(activeId.value, node.value),
      api.cephServices(activeId.value, node.value, "mon"),
      api.cephServices(activeId.value, node.value, "mgr"),
      api.cephServices(activeId.value, node.value, "mds"),
    ]);
    status.value = st;
    osds.value = flattenOsdTree(tree);
    pools.value = pl.sort((a, b) => a.pool_name.localeCompare(b.pool_name));
    daemons.value = { mon, mgr, mds };
    poolEdits.value = Object.fromEntries(pools.value.map((p) => [p.pool_name, poolEditOf(p)]));
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

async function run(what: string, fn: () => Promise<string | null>) {
  try {
    await fn();
    toast(what);
    await refresh();
  } catch (e) {
    toast(String(e), "error");
  }
}

function osdInOut(osd: OsdRow, into: boolean) {
  void run(`${osd.name} marked ${into ? "in" : "out"}`, () =>
    api.cephOsdInOut(activeId.value!, node.value, osd.id, into),
  );
}

function osdPower(osd: OsdRow, action: "start" | "stop") {
  void run(`${osd.name} ${action} requested`, () =>
    api.cephOsdPower(activeId.value!, osdNode(osd), osd.id, action),
  );
}

function destroyOsd(osd: OsdRow) {
  if (confirmDestroy.value !== osd.id) {
    confirmDestroy.value = osd.id;
    return;
  }
  confirmDestroy.value = null;
  void run(`${osd.name} destroyed`, () =>
    api.cephOsdDestroy(activeId.value!, osdNode(osd), osd.id, cleanup.value),
  );
}

function createPool() {
  if (!activeId.value || !newName.value) return;
  const params: Record<string, string> = {
    name: newName.value,
    size: newSize.value,
    min_size: newMinSize.value,
    pg_num: newPgNum.value,
  };
  if (newCrushRule.value) params.crush_rule = newCrushRule.value;
  if (newAddStorages.value) params.add_storages = "1";
  const name = newName.value;
  void run(`Pool ${name} created`, async () => {
    const upid = await api.cephPoolCreate(activeId.value!, node.value, params);
    newName.value = "";
    return upid;
  });
}

function savePool(p: CephPool) {
  const e = poolEdits.value[p.pool_name];
  if (!e || !poolDirty(p)) return;
  void run(`Pool ${p.pool_name} updated`, () =>
    api.cephPoolUpdate(activeId.value!, node.value, p.pool_name, {
      size: e.size,
      min_size: e.min_size,
      pg_num: e.pg_num,
    }),
  );
}

function deletePool(p: CephPool) {
  if (confirmDeletePool.value !== p.pool_name) {
    confirmDeletePool.value = p.pool_name;
    return;
  }
  confirmDeletePool.value = "";
  void run(`Pool ${p.pool_name} deleted`, () =>
    api.cephPoolDelete(activeId.value!, node.value, p.pool_name, removeStorages.value),
  );
}

onMounted(refresh);
watch(activeId, () => {
  node.value = "";
  void refresh();
});
// The cluster store is usually still loading on mount, so there is no node to
// query yet; pick one up as soon as it arrives.
watch(nodes, () => {
  if (!node.value) void refresh();
});
</script>

<template>
  <div>
    <div class="head">
      <h1>Ceph</h1>
      <label v-if="cephAvailable">
        Node
        <select
          v-model="node"
          @change="refresh"
        >
          <option
            v-for="n in nodes"
            :key="n.id"
            :value="n.node"
          >{{ n.node }}</option>
        </select>
      </label>
      <button @click="refresh">
        Refresh
      </button>
    </div>

    <p v-if="!activeId">
      No active connection. Add one under Connections.
    </p>
    <p v-else-if="!cephAvailable">
      This cluster does not run Ceph, or the connection's token cannot read it.
      Nothing to show.
    </p>
    <p v-else-if="loading">
      Loading…
    </p>

    <template v-else>
      <p
        v-if="error"
        class="error"
      >
        {{ error }}
      </p>

      <div class="card summary">
        <div>
          <h2 class="flush">
            Health
          </h2>
          <p
            class="big"
            :class="healthClass(health)"
          >
            {{ health ?? "unknown" }}
          </p>
          <p
            v-for="c in checks"
            :key="c.code"
            class="hint"
          >
            {{ c.severity }} {{ c.code }}: {{ c.message }}
          </p>
        </div>
        <div>
          <h2 class="flush">
            Monitors
          </h2>
          <p class="big">
            {{ quorum.length }} / {{ monCount }}
          </p>
          <p class="hint">
            in quorum: {{ quorum.join(", ") || "none" }}
          </p>
        </div>
        <div>
          <h2 class="flush">
            Placement groups
          </h2>
          <p class="big">
            {{ capacity?.num_pgs ?? "—" }}
          </p>
          <p
            v-for="s in pgStates(status)"
            :key="s.state"
            class="hint"
          >
            {{ s.count }} {{ s.state }}
          </p>
        </div>
        <div class="cap">
          <h2 class="flush">
            Capacity
          </h2>
          <UsageBar
            label="raw usage"
            :value="percent(capacity?.bytes_used, capacity?.bytes_total)"
            :detail="`${formatBytes(capacity?.bytes_used)} / ${formatBytes(capacity?.bytes_total)}`"
          />
          <p class="hint">
            {{ formatBytes(capacity?.bytes_avail) }} available
          </p>
        </div>
      </div>

      <h2>OSDs</h2>
      <label class="check">
        <input
          v-model="cleanup"
          type="checkbox"
        >
        also wipe the disk when destroying (<code>cleanup</code>)
      </label>
      <table
        v-if="osds.length > 0"
        v-cards
      >
        <thead>
          <tr>
            <th>OSD</th>
            <th>Host</th>
            <th>Class</th>
            <th>Status</th>
            <th>Usage</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="o in osds"
            :key="o.id"
          >
            <td>{{ o.name }}</td>
            <td>{{ o.host }}</td>
            <td>{{ o.deviceClass }}</td>
            <td>
              <span :class="o.up ? 'ok' : 'bad'">{{ o.up ? "up" : "down" }}</span>
              /
              <span :class="o.in ? 'ok' : 'warn'">{{ o.in ? "in" : "out" }}</span>
            </td>
            <td class="usage-cell">
              <UsageBar
                :label="`${osdPercent(o)}%`"
                :value="osdPercent(o)"
                :detail="o.total ? `${formatBytes(o.used)} / ${formatBytes(o.total)}` : '—'"
              />
            </td>
            <td class="row">
              <button @click="osdInOut(o, !o.in)">
                {{ o.in ? "Out" : "In" }}
              </button>
              <button @click="osdPower(o, o.up ? 'stop' : 'start')">
                {{ o.up ? "Stop" : "Start" }}
              </button>
              <button
                class="danger"
                @click="destroyOsd(o)"
              >
                {{ confirmDestroy === o.id ? `Confirm destroy ${o.name}?` : "Destroy" }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <p v-else>
        No OSDs reported.
      </p>

      <h2>Pools</h2>
      <div class="card row">
        <label>
          Name
          <input
            v-model="newName"
            placeholder="vmdata"
          >
        </label>
        <label>
          Size
          <input
            v-model="newSize"
            type="number"
            min="1"
          >
        </label>
        <label>
          Min size
          <input
            v-model="newMinSize"
            type="number"
            min="1"
          >
        </label>
        <label>
          PGs
          <input
            v-model="newPgNum"
            type="number"
            min="1"
          >
        </label>
        <label>
          CRUSH rule
          <input
            v-model="newCrushRule"
            placeholder="replicated_rule"
          >
        </label>
        <label class="check">
          <input
            v-model="newAddStorages"
            type="checkbox"
          >
          add PVE storages
        </label>
        <button
          :disabled="!newName"
          @click="createPool"
        >
          Create pool
        </button>
      </div>
      <p class="hint">
        Size is the number of replicas, min size the number that must be online
        before writes are accepted. Lowering either below 3/2 trades durability
        for space.
      </p>

      <label class="check">
        <input
          v-model="removeStorages"
          type="checkbox"
        >
        also delete the PVE storages backed by the pool
        (<code>remove_storages</code>)
      </label>
      <table
        v-if="pools.length > 0"
        v-cards
      >
        <thead>
          <tr>
            <th>Pool</th>
            <th>Type</th>
            <th>Size / min</th>
            <th>PGs</th>
            <th>Rule</th>
            <th>Used</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="p in pools"
            :key="p.pool_name"
          >
            <td>{{ p.pool_name }}</td>
            <td>{{ p.type ?? "—" }}</td>
            <td class="row narrow">
              <input
                v-model="poolEdits[p.pool_name].size"
                type="number"
                min="1"
              >
              /
              <input
                v-model="poolEdits[p.pool_name].min_size"
                type="number"
                min="1"
              >
            </td>
            <td class="narrow">
              <input
                v-model="poolEdits[p.pool_name].pg_num"
                type="number"
                min="1"
              >
            </td>
            <td>{{ p.crush_rule_name ?? p.crush_rule ?? "—" }}</td>
            <td>{{ poolPercent(p) }}% <small>{{ formatBytes(p.bytes_used) }}</small></td>
            <td class="row">
              <button
                :disabled="!poolDirty(p)"
                @click="savePool(p)"
              >
                Save
              </button>
              <button
                class="danger"
                @click="deletePool(p)"
              >
                {{
                  confirmDeletePool === p.pool_name
                    ? `Confirm delete ${p.pool_name} and all its data?`
                    : "Delete"
                }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <p v-else>
        No pools.
      </p>

      <h2>Monitors, managers and metadata servers</h2>
      <table v-cards>
        <thead>
          <tr>
            <th>Kind</th>
            <th>Name</th>
            <th>Host</th>
            <th>Address</th>
            <th>State</th>
            <th>Version</th>
          </tr>
        </thead>
        <tbody>
          <template
            v-for="kind in (['mon', 'mgr', 'mds'] as const)"
            :key="kind"
          >
            <tr
              v-for="(d, i) in daemons[kind]"
              :key="`${kind}-${d.name ?? i}`"
            >
              <td>{{ kind }}</td>
              <td>{{ d.name ?? "—" }}</td>
              <td>{{ d.host ?? "—" }}</td>
              <td>{{ d.addr ?? "—" }}</td>
              <td>
                <span :class="d.state === 'running' || d.quorum ? 'ok' : 'warn'">
                  {{ d.state ?? (d.quorum ? "in quorum" : "—") }}
                </span>
              </td>
              <td>{{ d.ceph_version_short ?? "—" }}</td>
            </tr>
          </template>
        </tbody>
      </table>
    </template>
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

h2 {
  margin-top: 28px;
}

h2.flush {
  margin-top: 0;
  font-size: 1em;
  opacity: 0.7;
}

.card {
  border: 1px solid #ccc3;
  border-radius: 8px;
  padding: 12px 16px;
  margin: 12px 0;
}

.summary {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 20px;
}

.summary .big {
  font-size: 1.4em;
  margin: 4px 0;
}

.cap {
  min-width: 200px;
}

.row {
  display: flex;
  gap: 12px;
  align-items: flex-end;
  flex-wrap: wrap;
}

label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 0.9em;
}

label.check {
  flex-direction: row;
  align-items: center;
  gap: 6px;
  font-size: 0.85em;
  opacity: 0.8;
}

.row input {
  width: 110px;
}

.narrow input {
  width: 56px;
}

.narrow.row {
  gap: 4px;
  align-items: center;
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

.usage-cell {
  min-width: 140px;
}

td small {
  opacity: 0.6;
}

.ok {
  color: #2a7;
}

.warn {
  color: #e5a000;
}

.bad {
  color: #c33;
}

.danger {
  color: #c33;
}

.hint {
  font-size: 0.85em;
  opacity: 0.7;
  margin: 2px 0;
}

.error {
  color: #c33;
}
</style>
