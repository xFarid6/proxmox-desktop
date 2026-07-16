<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import {
  api,
  type BackupJob,
  type ReplicationJob,
  type StorageContent,
  type StorageSummary,
} from "../api";
import { activeId } from "../stores/connections";
import { guests, nodes, refreshCluster } from "../stores/cluster";
import { toast } from "../stores/toast";
import { formatBytes } from "../format";

const node = ref("");
const storage = ref("");
const storages = ref<StorageSummary[]>([]);
const backups = ref<StorageContent[]>([]);
const jobs = ref<BackupJob[]>([]);
const replications = ref<ReplicationJob[]>([]);
const loading = ref(false);
const error = ref("");

// Backup-now form.
const backupVmid = ref<number>();
const mode = ref("snapshot");
const compress = ref("zstd");
const submitting = ref(false);

// Per-row UI state: restore target vmid input and delete confirmation.
const restoreVmid = ref<Record<string, number>>({});
const confirmDelete = ref("");

const backupStorages = computed(() =>
  storages.value.filter((s) => (s.content ?? "").split(",").includes("backup")),
);

function backupDate(ctime?: number): string {
  return ctime ? new Date(ctime * 1000).toLocaleString() : "—";
}

async function loadStorages() {
  if (!activeId.value || !node.value) return;
  try {
    storages.value = await api.nodeStorages(activeId.value, node.value);
    if (!backupStorages.value.some((s) => s.storage === storage.value)) {
      storage.value = backupStorages.value[0]?.storage ?? "";
    }
  } catch (e) {
    error.value = String(e);
  }
}

async function loadBackups() {
  if (!activeId.value || !node.value || !storage.value) {
    backups.value = [];
    return;
  }
  loading.value = true;
  error.value = "";
  try {
    const list = await api.storageContent(activeId.value, node.value, storage.value, "backup");
    backups.value = list.sort((a, b) => (b.ctime ?? 0) - (a.ctime ?? 0));
  } catch (e) {
    error.value = String(e);
    backups.value = [];
  } finally {
    loading.value = false;
  }
}

async function loadJobs() {
  if (!activeId.value) return;
  try {
    [jobs.value, replications.value] = await Promise.all([
      api.backupJobs(activeId.value),
      api.replicationJobs(activeId.value),
    ]);
  } catch {
    // Backup/replication job listing needs cluster-level perms; keep the
    // volume list usable without them.
    jobs.value = [];
    replications.value = [];
  }
}

async function backupNow() {
  if (!activeId.value || !node.value || !storage.value || !backupVmid.value) return;
  submitting.value = true;
  try {
    const upid = await api.vzdump(activeId.value, node.value, {
      vmid: String(backupVmid.value),
      storage: storage.value,
      mode: mode.value,
      compress: compress.value,
    });
    toast(`Backup task started: ${upid}`);
  } catch (e) {
    toast(String(e), "error");
  } finally {
    submitting.value = false;
  }
}

function guestKindOf(volid: string): "qemu" | "lxc" | null {
  if (volid.includes("vzdump-qemu-")) return "qemu";
  if (volid.includes("vzdump-lxc-")) return "lxc";
  return null;
}

async function restore(b: StorageContent) {
  const kind = guestKindOf(b.volid);
  const vmid = restoreVmid.value[b.volid];
  if (!activeId.value || !node.value || !kind || !vmid) return;
  const params: Record<string, string> =
    kind === "qemu"
      ? { vmid: String(vmid), archive: b.volid }
      : { vmid: String(vmid), ostemplate: b.volid, restore: "1" };
  try {
    const upid = await api.createGuest(activeId.value, node.value, kind, params);
    toast(`Restore task started: ${upid}`);
    await refreshCluster();
  } catch (e) {
    toast(String(e), "error");
  }
}

async function removeBackup(b: StorageContent) {
  if (confirmDelete.value !== b.volid) {
    confirmDelete.value = b.volid;
    return;
  }
  confirmDelete.value = "";
  if (!activeId.value || !node.value || !storage.value) return;
  try {
    await api.deleteVolume(activeId.value, node.value, storage.value, b.volid);
    toast(`Deleted ${b.volid}`);
    await loadBackups();
  } catch (e) {
    toast(String(e), "error");
  }
}

onMounted(async () => {
  if (nodes.value.length === 0) await refreshCluster();
  if (!node.value && nodes.value.length > 0) node.value = nodes.value[0].node ?? "";
  backupVmid.value = guests.value[0]?.vmid;
  await loadJobs();
});

watch(node, async () => {
  await loadStorages();
  await loadBackups();
});
watch(storage, loadBackups);
watch(activeId, () => {
  node.value = "";
  storage.value = "";
  backups.value = [];
});
</script>

<template>
  <div>
    <div class="head">
      <h1>Backups</h1>
      <label>
        Node
        <select v-model="node">
          <option
            v-for="n in nodes"
            :key="n.id"
            :value="n.node"
          >{{ n.node }}</option>
        </select>
      </label>
      <label>
        Storage
        <select v-model="storage">
          <option
            v-for="s in backupStorages"
            :key="s.storage"
            :value="s.storage"
          >{{ s.storage }}</option>
        </select>
      </label>
      <button @click="loadBackups">
        Refresh
      </button>
    </div>

    <p v-if="!activeId">
      No active connection. Add one under Connections.
    </p>

    <template v-else>
      <div class="card row">
        <label>
          Guest
          <select v-model.number="backupVmid">
            <option
              v-for="g in guests"
              :key="g.id"
              :value="g.vmid"
            >{{ g.vmid }} — {{ g.name ?? g.id }}</option>
          </select>
        </label>
        <label>
          Mode
          <select v-model="mode">
            <option value="snapshot">snapshot</option>
            <option value="suspend">suspend</option>
            <option value="stop">stop</option>
          </select>
        </label>
        <label>
          Compression
          <select v-model="compress">
            <option value="zstd">zstd</option>
            <option value="gzip">gzip</option>
            <option value="lzo">lzo</option>
            <option value="0">none</option>
          </select>
        </label>
        <button
          :disabled="submitting || !backupVmid || !storage"
          @click="backupNow"
        >
          {{ submitting ? "Starting…" : "Backup now" }}
        </button>
      </div>

      <p v-if="loading">
        Loading…
      </p>
      <p
        v-else-if="error"
        class="error"
      >
        {{ error }}
      </p>

      <table
        v-else-if="backups.length > 0"
        v-cards
      >
        <thead>
          <tr>
            <th>Archive</th>
            <th>VMID</th>
            <th>Size</th>
            <th>Date</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="b in backups"
            :key="b.volid"
          >
            <td :title="b.notes">
              {{ b.volid }}
            </td>
            <td>{{ b.vmid ?? "—" }}</td>
            <td>{{ formatBytes(b.size) }}</td>
            <td>{{ backupDate(b.ctime) }}</td>
            <td class="actions">
              <input
                v-model.number="restoreVmid[b.volid]"
                type="number"
                min="100"
                placeholder="new vmid"
              >
              <button
                :disabled="!restoreVmid[b.volid]"
                @click="restore(b)"
              >
                Restore
              </button>
              <button
                class="danger"
                @click="removeBackup(b)"
              >
                {{ confirmDelete === b.volid ? "Confirm?" : "Delete" }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <p v-else-if="storage && !loading">
        No backups on {{ storage }}.
      </p>

      <h2>Scheduled backup jobs</h2>
      <table
        v-if="jobs.length > 0"
        v-cards
      >
        <thead>
          <tr>
            <th>ID</th>
            <th>Schedule</th>
            <th>Storage</th>
            <th>Guests</th>
            <th>Mode</th>
            <th>Enabled</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="j in jobs"
            :key="j.id"
          >
            <td>{{ j.id }}</td>
            <td>{{ j.schedule ?? "—" }}</td>
            <td>{{ j.storage ?? "—" }}</td>
            <td>{{ j.all ? "all" : (j.vmid ?? "—") }}</td>
            <td>{{ j.mode ?? "—" }}</td>
            <td>{{ j.enabled ? "yes" : "no" }}</td>
          </tr>
        </tbody>
      </table>
      <p v-else>
        No scheduled backup jobs.
      </p>

      <h2>Replication</h2>
      <table
        v-if="replications.length > 0"
        v-cards
      >
        <thead>
          <tr>
            <th>ID</th>
            <th>Guest</th>
            <th>Target</th>
            <th>Schedule</th>
            <th>Enabled</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="r in replications"
            :key="r.id"
          >
            <td>{{ r.id }}</td>
            <td>{{ r.guest ?? "—" }}</td>
            <td>{{ r.target ?? "—" }}</td>
            <td>{{ r.schedule ?? "—" }}</td>
            <td>{{ r.disable ? "no" : "yes" }}</td>
          </tr>
        </tbody>
      </table>
      <p v-else>
        No replication jobs.
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

.card {
  border: 1px solid #ccc3;
  border-radius: 8px;
  padding: 12px 16px;
  margin: 12px 0;
}

.row {
  display: flex;
  gap: 16px;
  align-items: flex-end;
}

label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 0.9em;
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

h2 {
  margin-top: 28px;
}

.actions {
  display: flex;
  gap: 6px;
  align-items: center;
}

.actions input {
  width: 90px;
}

.danger {
  color: #c33;
}

.error {
  color: #c33;
}
</style>
