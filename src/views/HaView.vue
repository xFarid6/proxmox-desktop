<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { api, type ClusterResource, type HaGroup, type HaResource, type HaStatus } from "../api";
import { formatHaNodes, guestSid, normalizeHaNodes, sidVmid } from "../ha";
import { guests, nodes, refreshCluster } from "../stores/cluster";
import { activeId } from "../stores/connections";
import { toast } from "../stores/toast";

const STATES = ["started", "stopped", "disabled", "ignored"];

const status = ref<HaStatus[]>([]);
const resources = ref<HaResource[]>([]);
const groups = ref<HaGroup[]>([]);
const loading = ref(false);
const error = ref("");
const confirmDeleteRes = ref("");
const confirmDeleteGroup = ref("");

// Add-resource form.
const newSid = ref("");
const newState = ref("started");
const newGroup = ref("");
const newMaxRestart = ref("");
const newMaxRelocate = ref("");

// Add-group form.
const newGroupName = ref("");
const newGroupNodes = ref("");
const newGroupRestricted = ref(false);
const newGroupNofailback = ref(false);

// Per-row pending state edits, keyed by sid.
const stateEdits = ref<Record<string, string>>({});

/** `guests` is already filtered to qemu/lxc rows, so the cast is safe. */
function sidOf(g: ClusterResource): string {
  return guestSid(g.type as "qemu" | "lxc", g.vmid ?? 0);
}

/** Guests not already under HA — no point offering them twice. */
const addable = computed(() =>
  guests.value
    .filter((g) => g.vmid != null && !resources.value.some((r) => r.sid === sidOf(g)))
    .sort((a, b) => (a.vmid ?? 0) - (b.vmid ?? 0)),
);

/** HA only makes sense on a quorate cluster; below three nodes a failure
 * takes quorum with it. Shown as a hint, never as a reason to hide the UI —
 * the user may be part-way through building the cluster. */
const showHint = computed(() => nodes.value.length < 3 || status.value.length === 0);

/** `quorate` arrives as "1"/"0" on some PVE versions and 1/0 on others —
 * the string "0" is truthy, so it has to go through Number(). */
function isBad(s: HaStatus): boolean {
  return s.type === "quorum" && Number(s.quorate) !== 1;
}

function guestName(sid: string): string {
  const vmid = sidVmid(sid);
  return guests.value.find((g) => g.vmid === vmid)?.name ?? "";
}

async function refresh() {
  if (!activeId.value) {
    status.value = [];
    resources.value = [];
    groups.value = [];
    return;
  }
  loading.value = true;
  error.value = "";
  confirmDeleteRes.value = confirmDeleteGroup.value = "";
  try {
    void refreshCluster();
    [status.value, resources.value, groups.value] = await Promise.all([
      api.haStatusCurrent(activeId.value),
      api.haResources(activeId.value),
      api.haGroups(activeId.value),
    ]);
    resources.value.sort((a, b) => a.sid.localeCompare(b.sid));
    groups.value.sort((a, b) => a.group.localeCompare(b.group));
    stateEdits.value = Object.fromEntries(
      resources.value.map((r) => [r.sid, r.state ?? "started"]),
    );
    if (!newSid.value && addable.value.length > 0) {
      newSid.value = sidOf(addable.value[0]);
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

async function addResource() {
  if (!activeId.value || !newSid.value) return;
  try {
    const params: Record<string, string> = { sid: newSid.value, state: newState.value };
    if (newGroup.value) params.group = newGroup.value;
    if (newMaxRestart.value) params.max_restart = newMaxRestart.value;
    if (newMaxRelocate.value) params.max_relocate = newMaxRelocate.value;
    await api.addHaResource(activeId.value, params);
    toast(`${newSid.value} added to HA`);
    newSid.value = newMaxRestart.value = newMaxRelocate.value = "";
    await refresh();
  } catch (e) {
    toast(String(e), "error");
  }
}

async function saveState(r: HaResource) {
  if (!activeId.value) return;
  const state = stateEdits.value[r.sid];
  if (!state || state === (r.state ?? "started")) return;
  try {
    await api.updateHaResource(activeId.value, r.sid, { state });
    toast(`${r.sid} set to ${state}`);
    await refresh();
  } catch (e) {
    toast(String(e), "error");
  }
}

async function removeResource(sid: string) {
  if (confirmDeleteRes.value !== sid) {
    confirmDeleteRes.value = sid;
    return;
  }
  confirmDeleteRes.value = "";
  if (!activeId.value) return;
  try {
    await api.deleteHaResource(activeId.value, sid);
    toast(`${sid} removed from HA`);
    await refresh();
  } catch (e) {
    toast(String(e), "error");
  }
}

async function addGroup() {
  if (!activeId.value || !newGroupName.value) return;
  try {
    const params: Record<string, string> = {
      group: newGroupName.value,
      nodes: normalizeHaNodes(newGroupNodes.value),
    };
    if (newGroupRestricted.value) params.restricted = "1";
    if (newGroupNofailback.value) params.nofailback = "1";
    await api.addHaGroup(activeId.value, params);
    toast(`Group ${newGroupName.value} created`);
    newGroupName.value = newGroupNodes.value = "";
    newGroupRestricted.value = newGroupNofailback.value = false;
    await refresh();
  } catch (e) {
    toast(String(e), "error");
  }
}

async function removeGroup(group: string) {
  if (confirmDeleteGroup.value !== group) {
    confirmDeleteGroup.value = group;
    return;
  }
  confirmDeleteGroup.value = "";
  if (!activeId.value) return;
  try {
    await api.deleteHaGroup(activeId.value, group);
    toast(`Group ${group} deleted`);
    await refresh();
  } catch (e) {
    toast(String(e), "error");
  }
}

onMounted(refresh);
watch(activeId, refresh);
</script>

<template>
  <div>
    <div class="head">
      <h1>High availability</h1>
      <button @click="refresh">
        Refresh
      </button>
    </div>

    <p v-if="!activeId">
      No active connection. Add one under Connections.
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
      <p
        v-if="showHint"
        class="hint"
      >
        HA needs a quorate cluster of at least 3 nodes — this one reports
        {{ nodes.length }}. Rules can be written now, but nothing fails over
        until the cluster is quorate.
      </p>

      <h2>Status</h2>
      <table
        v-if="status.length > 0"
        v-cards
      >
        <thead>
          <tr>
            <th>Id</th>
            <th>Type</th>
            <th>Node</th>
            <th>Status</th>
            <th>CRM state</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="s in status"
            :key="s.id"
          >
            <td>{{ s.id }}</td>
            <td>{{ s.type ?? "—" }}</td>
            <td>{{ s.node ?? "—" }}</td>
            <td>
              <span :class="isBad(s) ? 'bad' : 'ok'">
                {{ s.status ?? "—" }}
              </span>
            </td>
            <td>{{ s.crm_state ?? "—" }}</td>
          </tr>
        </tbody>
      </table>
      <p v-else>
        No HA status reported by this cluster.
      </p>

      <h2>Resources</h2>
      <div class="card row">
        <label>
          Guest
          <select v-model="newSid">
            <option
              v-for="g in addable"
              :key="g.id"
              :value="sidOf(g)"
            >{{ g.vmid }} {{ g.name }} ({{ g.type }})</option>
          </select>
        </label>
        <label>
          State
          <select v-model="newState">
            <option
              v-for="s in STATES"
              :key="s"
              :value="s"
            >{{ s }}</option>
          </select>
        </label>
        <label>
          Group
          <select v-model="newGroup">
            <option value="">
              none
            </option>
            <option
              v-for="g in groups"
              :key="g.group"
              :value="g.group"
            >{{ g.group }}</option>
          </select>
        </label>
        <label>
          Max restart
          <input
            v-model="newMaxRestart"
            type="number"
            min="0"
          >
        </label>
        <label>
          Max relocate
          <input
            v-model="newMaxRelocate"
            type="number"
            min="0"
          >
        </label>
        <button
          :disabled="!newSid"
          @click="addResource"
        >
          Add to HA
        </button>
      </div>

      <table
        v-if="resources.length > 0"
        v-cards
      >
        <thead>
          <tr>
            <th>Service</th>
            <th>Group</th>
            <th>Max restart</th>
            <th>Max relocate</th>
            <th>State</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="r in resources"
            :key="r.sid"
          >
            <td>{{ r.sid }} <small>{{ guestName(r.sid) }}</small></td>
            <td>{{ r.group ?? "—" }}</td>
            <td>{{ r.max_restart ?? "—" }}</td>
            <td>{{ r.max_relocate ?? "—" }}</td>
            <td>
              <select v-model="stateEdits[r.sid]">
                <option
                  v-for="s in STATES"
                  :key="s"
                  :value="s"
                >
                  {{ s }}
                </option>
              </select>
            </td>
            <td class="row">
              <button
                :disabled="stateEdits[r.sid] === (r.state ?? 'started')"
                @click="saveState(r)"
              >
                Save
              </button>
              <button
                class="danger"
                @click="removeResource(r.sid)"
              >
                {{ confirmDeleteRes === r.sid ? "Confirm?" : "Remove" }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <p v-else>
        No guests under HA.
      </p>

      <h2>Groups</h2>
      <div class="card row">
        <label>
          Name
          <input
            v-model="newGroupName"
            placeholder="prod"
          >
        </label>
        <label>
          Nodes
          <input
            v-model="newGroupNodes"
            class="wide"
            :placeholder="nodes.map((n) => n.node).join(',') || 'pve1:2,pve2'"
          >
        </label>
        <label class="check">
          <input
            v-model="newGroupRestricted"
            type="checkbox"
          >
          restricted
        </label>
        <label class="check">
          <input
            v-model="newGroupNofailback"
            type="checkbox"
          >
          nofailback
        </label>
        <button
          :disabled="!newGroupName || !newGroupNodes"
          @click="addGroup"
        >
          Add group
        </button>
      </div>
      <p class="hint">
        Nodes are comma-separated, each optionally <code>:priority</code> — higher
        priority wins. Restricted keeps services on the listed nodes only;
        nofailback stops them moving back when a preferred node returns.
      </p>

      <table
        v-if="groups.length > 0"
        v-cards
      >
        <thead>
          <tr>
            <th>Group</th>
            <th>Nodes</th>
            <th>Restricted</th>
            <th>No failback</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="g in groups"
            :key="g.group"
          >
            <td>{{ g.group }}</td>
            <td>{{ formatHaNodes(g.nodes) }}</td>
            <td>{{ g.restricted ? "yes" : "no" }}</td>
            <td>{{ g.nofailback ? "yes" : "no" }}</td>
            <td>
              <button
                class="danger"
                @click="removeGroup(g.group)"
              >
                {{ confirmDeleteGroup === g.group ? "Confirm?" : "Delete" }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <p v-else>
        No HA groups.
      </p>
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

.card {
  border: 1px solid #ccc3;
  border-radius: 8px;
  padding: 12px 16px;
  margin: 12px 0;
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
}

.row input {
  width: 100px;
}

.row input.wide {
  width: 220px;
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

td small {
  opacity: 0.6;
}

.ok {
  color: #2a7;
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
}

.error {
  color: #c33;
}
</style>
