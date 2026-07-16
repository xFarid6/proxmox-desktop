<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { api, type StorageConfig } from "../api";
import { activeId } from "../stores/connections";
import { toast } from "../stores/toast";

const configs = ref<StorageConfig[]>([]);
const loading = ref(false);
const error = ref("");
const confirmDelete = ref("");

// Add-storage form.
const newName = ref("");
const newType = ref("dir");
const newContent = ref("images,iso,vztmpl,backup");
const newPath = ref("");
const newServer = ref("");
const newExport = ref("");
const newShare = ref("");
const newUsername = ref("");
const newPassword = ref("");
const adding = ref(false);

async function refresh() {
  if (!activeId.value) {
    configs.value = [];
    return;
  }
  loading.value = true;
  error.value = "";
  confirmDelete.value = "";
  try {
    const list = await api.storageConfigs(activeId.value);
    configs.value = list.sort((a, b) => a.storage.localeCompare(b.storage));
  } catch (e) {
    error.value = String(e);
    configs.value = [];
  } finally {
    loading.value = false;
  }
}

async function addStorage() {
  if (!activeId.value || !newName.value) return;
  adding.value = true;
  try {
    const params: Record<string, string> = {
      storage: newName.value,
      type: newType.value,
      content: newContent.value,
    };
    if (newType.value === "dir") params.path = newPath.value;
    if (newType.value === "nfs") {
      params.server = newServer.value;
      params.export = newExport.value;
    }
    if (newType.value === "cifs") {
      params.server = newServer.value;
      params.share = newShare.value;
      if (newUsername.value) params.username = newUsername.value;
      if (newPassword.value) params.password = newPassword.value;
    }
    await api.addStorage(activeId.value, params);
    toast(`Storage ${newName.value} added`);
    newName.value = "";
    await refresh();
  } catch (e) {
    toast(String(e), "error");
  } finally {
    adding.value = false;
  }
}

async function removeStorage(s: StorageConfig) {
  if (confirmDelete.value !== s.storage) {
    confirmDelete.value = s.storage;
    return;
  }
  confirmDelete.value = "";
  if (!activeId.value) return;
  try {
    await api.deleteStorage(activeId.value, s.storage);
    toast(`Storage ${s.storage} removed (data untouched)`);
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
      <h1>Storage</h1>
      <button @click="refresh">
        Refresh
      </button>
    </div>

    <p v-if="!activeId">
      No active connection. Add one under Connections.
    </p>

    <template v-else>
      <div class="card row">
        <label>
          Name
          <input
            v-model="newName"
            placeholder="mystorage"
          >
        </label>
        <label>
          Type
          <select v-model="newType">
            <option value="dir">dir</option>
            <option value="nfs">nfs</option>
            <option value="cifs">cifs</option>
          </select>
        </label>
        <label>
          Content
          <input v-model="newContent">
        </label>
        <label v-if="newType === 'dir'">
          Path
          <input
            v-model="newPath"
            placeholder="/mnt/data"
          >
        </label>
        <template v-if="newType === 'nfs' || newType === 'cifs'">
          <label>
            Server
            <input
              v-model="newServer"
              placeholder="10.0.0.5"
            >
          </label>
          <label v-if="newType === 'nfs'">
            Export
            <input
              v-model="newExport"
              placeholder="/export/backups"
            >
          </label>
          <template v-if="newType === 'cifs'">
            <label>
              Share
              <input v-model="newShare">
            </label>
            <label>
              Username
              <input v-model="newUsername">
            </label>
            <label>
              Password
              <input
                v-model="newPassword"
                type="password"
                autocomplete="new-password"
              >
            </label>
          </template>
        </template>
        <button
          :disabled="adding || !newName"
          @click="addStorage"
        >
          {{ adding ? "Adding…" : "Add storage" }}
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

      <table v-else-if="configs.length > 0">
        <thead>
          <tr>
            <th>Storage</th>
            <th>Type</th>
            <th>Content</th>
            <th>Source</th>
            <th>Nodes</th>
            <th>Shared</th>
            <th>Enabled</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="s in configs"
            :key="s.storage"
          >
            <td>{{ s.storage }}</td>
            <td>{{ s.type }}</td>
            <td class="content-cell">
              {{ s.content ?? "—" }}
            </td>
            <td>{{ s.path ?? (s.server ? `${s.server}:${s.export ?? s.share ?? ""}` : "—") }}</td>
            <td>{{ s.nodes ?? "all" }}</td>
            <td>{{ s.shared ? "yes" : "no" }}</td>
            <td>
              <span :class="s.disable ? 'off' : 'ok'">{{ s.disable ? "no" : "yes" }}</span>
            </td>
            <td>
              <button
                class="danger"
                @click="removeStorage(s)"
              >
                {{ confirmDelete === s.storage ? "Confirm?" : "Remove" }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <p v-else-if="!loading">
        No storage definitions.
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

.row input {
  width: 140px;
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

.content-cell {
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ok {
  color: #2a7;
}

.off {
  opacity: 0.6;
}

.danger {
  color: #c33;
}

.error {
  color: #c33;
}
</style>
