<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { api, type TaskEntry, type TaskLogLine } from "../api";
import { activeId } from "../stores/connections";
import { nodes, refreshCluster } from "../stores/cluster";

const node = ref("");
const tasks = ref<TaskEntry[]>([]);
const loading = ref(false);
const error = ref("");

const selected = ref<TaskEntry | null>(null);
const logLines = ref<TaskLogLine[]>([]);
const logError = ref("");

async function refreshTasks() {
  if (!activeId.value || !node.value) {
    tasks.value = [];
    return;
  }
  loading.value = true;
  error.value = "";
  try {
    tasks.value = await api.nodeTasks(activeId.value, node.value);
  } catch (e) {
    error.value = String(e);
    tasks.value = [];
  } finally {
    loading.value = false;
  }
}

async function openTask(t: TaskEntry) {
  if (!activeId.value) return;
  selected.value = t;
  logLines.value = [];
  logError.value = "";
  try {
    logLines.value = await api.taskLog(activeId.value, t.node, t.upid);
  } catch (e) {
    logError.value = String(e);
  }
}

function taskResult(t: TaskEntry): string {
  if (!t.endtime) return "running";
  return t.status ?? "—";
}

function when(ts?: number): string {
  return ts ? new Date(ts * 1000).toLocaleString() : "—";
}

onMounted(async () => {
  if (nodes.value.length === 0) await refreshCluster();
  if (!node.value && nodes.value.length > 0) node.value = nodes.value[0].node ?? "";
});

watch(node, refreshTasks);
watch(activeId, () => {
  node.value = "";
  tasks.value = [];
  selected.value = null;
});
</script>

<template>
  <div>
    <div class="head">
      <h1>Tasks</h1>
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
      <button @click="refreshTasks">
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

    <div
      v-else
      class="split"
    >
      <table v-if="tasks.length > 0">
        <thead>
          <tr>
            <th>Start</th>
            <th>Type</th>
            <th>ID</th>
            <th>User</th>
            <th>Result</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="t in tasks"
            :key="t.upid"
            :class="{ selected: selected?.upid === t.upid }"
            @click="openTask(t)"
          >
            <td>{{ when(t.starttime) }}</td>
            <td>{{ t.type }}</td>
            <td>{{ t.id ?? "—" }}</td>
            <td>{{ t.user ?? "—" }}</td>
            <td>
              <span
                class="result"
                :class="{ ok: taskResult(t) === 'OK', running: taskResult(t) === 'running' }"
              >{{ taskResult(t) }}</span>
            </td>
          </tr>
        </tbody>
      </table>
      <p v-else-if="node">
        No tasks on {{ node }}.
      </p>

      <div
        v-if="selected"
        class="log"
      >
        <div class="log-head">
          <strong>{{ selected.upid }}</strong>
          <button @click="selected = null">
            Close
          </button>
        </div>
        <p
          v-if="logError"
          class="error"
        >
          {{ logError }}
        </p>
        <pre v-else>{{ logLines.map((l) => l.t).join("\n") || "(empty log)" }}</pre>
      </div>
    </div>
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

.split {
  display: flex;
  flex-direction: column;
  gap: 16px;
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

tbody tr {
  cursor: pointer;
}

tbody tr:hover,
tr.selected {
  background: #8881;
}

.result {
  font-size: 0.85em;
}

.result.ok {
  color: #2a7;
}

.result.running {
  color: #e5a000;
}

.log {
  border: 1px solid #ccc3;
  border-radius: 8px;
  padding: 10px 14px;
}

.log-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
}

.log pre {
  max-height: 320px;
  overflow: auto;
  font-size: 0.85em;
  white-space: pre-wrap;
}

.error {
  color: #c33;
}
</style>
