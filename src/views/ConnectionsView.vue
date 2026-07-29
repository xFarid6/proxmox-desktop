<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { api, type ConnectionInfo } from "../api";
import { activeId, connections, refreshConnections, setActive } from "../stores/connections";
import { toast } from "../stores/toast";

const editing = ref(false);
const testResult = ref("");
const error = ref("");
const busy = ref(false);

const blank = () => ({
  id: "",
  name: "",
  host: "",
  token: "",
  acceptInvalidCerts: false,
  sshEnabled: false,
  sshUser: "",
  sshPort: 22,
  sshAuth: "password" as "password" | "key" | "agent",
  sshKeyPath: "",
  sshSecret: "",
});
const form = reactive(blank());

function startAdd() {
  Object.assign(form, blank());
  testResult.value = "";
  error.value = "";
  editing.value = true;
}

function startEdit(c: ConnectionInfo) {
  Object.assign(form, {
    id: c.id,
    name: c.name,
    host: c.host,
    acceptInvalidCerts: c.acceptInvalidCerts,
    token: "",
    sshEnabled: !!c.ssh,
    sshUser: c.ssh?.user ?? "",
    sshPort: c.ssh?.port ?? 22,
    sshAuth: c.ssh?.useAgent ? "agent" : c.ssh?.keyPath ? "key" : "password",
    sshKeyPath: c.ssh?.keyPath ?? "",
    sshSecret: "",
  });
  testResult.value = "";
  error.value = "";
  editing.value = true;
}

async function test() {
  busy.value = true;
  testResult.value = "";
  error.value = "";
  try {
    const v = await api.testConnection({
      host: form.host,
      token: form.token || undefined,
      acceptInvalidCerts: form.acceptInvalidCerts,
      connectionId: form.id || undefined,
    });
    testResult.value = `OK — Proxmox VE ${v.version}`;
    toast(`Connected: Proxmox VE ${v.version}`);
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function save() {
  busy.value = true;
  error.value = "";
  try {
    const info: ConnectionInfo = {
      id: form.id || crypto.randomUUID(),
      name: form.name || form.host,
      host: form.host,
      acceptInvalidCerts: form.acceptInvalidCerts,
      ssh: form.sshEnabled
        ? {
            user: form.sshUser,
            port: form.sshPort,
            useAgent: form.sshAuth === "agent",
            keyPath: form.sshAuth === "key" ? form.sshKeyPath : null,
          }
        : null,
    };
    const sshSecret = form.sshEnabled && form.sshAuth !== "agent" ? form.sshSecret : undefined;
    await api.saveConnection(info, form.token || undefined, sshSecret || undefined);
    editing.value = false;
    toast("Connection saved");
    await refreshConnections();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function remove(id: string) {
  error.value = "";
  try {
    await api.deleteConnection(id);
    toast("Connection removed");
    await refreshConnections();
  } catch (e) {
    error.value = String(e);
  }
}

onMounted(refreshConnections);
</script>

<template>
  <div>
    <h1>Connections</h1>

    <p
      v-if="error"
      class="error"
    >
      {{ error }}
    </p>

    <div
      v-if="!editing"
      class="list"
    >
      <div
        v-for="c in connections"
        :key="c.id"
        class="card"
        :class="{ active: c.id === activeId }"
      >
        <div class="card-main">
          <strong>{{ c.name }}</strong>
          <span class="host">{{ c.host }}</span>
          <span
            v-if="c.acceptInvalidCerts"
            class="badge"
          >self-signed OK</span>
        </div>
        <div class="card-actions">
          <button
            :disabled="c.id === activeId"
            @click="setActive(c.id)"
          >
            {{ c.id === activeId ? "Active" : "Use" }}
          </button>
          <button @click="startEdit(c)">
            Edit
          </button>
          <button
            class="danger"
            @click="remove(c.id)"
          >
            Remove
          </button>
        </div>
      </div>

      <p v-if="connections.length === 0">
        No connections yet. Add your Proxmox server to get started.
      </p>
      <button @click="startAdd">
        Add connection
      </button>
    </div>

    <form
      v-else
      class="form"
      @submit.prevent="save"
    >
      <label>
        Name
        <input
          v-model="form.name"
          placeholder="Homelab"
        >
      </label>
      <label>
        Host
        <input
          v-model="form.host"
          placeholder="https://pve.example.com:8006"
          required
        >
      </label>
      <label>
        API token
        <input
          v-model="form.token"
          type="password"
          :placeholder="form.id ? '(unchanged)' : 'user@realm!tokenid=uuid'"
          :required="!form.id"
        >
      </label>
      <label class="check">
        <input
          v-model="form.acceptInvalidCerts"
          type="checkbox"
        >
        Accept self-signed certificate (only enable for hosts you trust)
      </label>

      <label class="check">
        <input
          v-model="form.sshEnabled"
          type="checkbox"
        >
        Enable SSH shell for this connection
      </label>

      <template v-if="form.sshEnabled">
        <label>
          SSH user
          <input
            v-model="form.sshUser"
            placeholder="root"
            :required="form.sshEnabled"
          >
        </label>
        <label>
          SSH port
          <input
            v-model.number="form.sshPort"
            type="number"
          >
        </label>
        <label>
          Auth method
          <select v-model="form.sshAuth">
            <option value="password">
              Password
            </option>
            <option value="key">
              Key file
            </option>
            <option value="agent">
              SSH agent / Pageant
            </option>
          </select>
        </label>
        <label v-if="form.sshAuth === 'key'">
          Key file path
          <input
            v-model="form.sshKeyPath"
            placeholder="C:\Users\you\.ssh\id_ed25519"
          >
        </label>
        <label v-if="form.sshAuth !== 'agent'">
          {{ form.sshAuth === "key" ? "Key passphrase (if any)" : "SSH password" }}
          <input
            v-model="form.sshSecret"
            type="password"
            :placeholder="form.id ? '(unchanged)' : ''"
          >
        </label>
      </template>

      <p
        v-if="testResult"
        class="ok"
      >
        {{ testResult }}
      </p>

      <div class="form-actions">
        <button
          type="button"
          :disabled="busy || !form.host"
          @click="test"
        >
          Test connection
        </button>
        <button
          type="submit"
          :disabled="busy"
        >
          Save
        </button>
        <button
          type="button"
          @click="editing = false"
        >
          Cancel
        </button>
      </div>
    </form>
  </div>
</template>

<style scoped>
.list,
.form {
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-width: 640px;
}

.card {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 16px;
  border: 1px solid #ccc3;
  border-radius: 8px;
}

.card.active {
  border-color: #e57000;
}

.card-main {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.host {
  opacity: 0.7;
  font-size: 0.9em;
}

.badge {
  font-size: 0.75em;
  opacity: 0.6;
}

.card-actions {
  display: flex;
  gap: 6px;
}

.form label {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.form .check {
  flex-direction: row;
  align-items: center;
  gap: 8px;
}

.form-actions {
  display: flex;
  gap: 8px;
}

.danger {
  border-color: #c33;
  color: #c33;
}

.error {
  color: #c33;
}

.ok {
  color: #2a7;
}
</style>
