<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import { api, type GuestKind } from "../api";
import { activeId } from "../stores/connections";

const route = useRoute();
const node = route.params.node as string;
const kind = route.params.kind as GuestKind;
const vmid = Number(route.params.vmid);

const config = ref<Record<string, unknown>>({});
const loading = ref(false);
const error = ref("");
const notice = ref("");

// Edit form state, seeded from config.
const cores = ref("");
const memory = ref("");
const disk = ref("");
const grow = ref("");
const saving = ref(false);

const configRows = computed(() =>
  Object.entries(config.value)
    .filter(([k]) => k !== "digest")
    .sort(([a], [b]) => a.localeCompare(b)),
);

// Disk-ish config keys: scsi0, virtio1, sata0, ide2, rootfs, mp0...
const diskKeys = computed(() =>
  Object.keys(config.value)
    .filter((k) => /^(scsi|sata|ide|virtio|rootfs$|mp)\d*$/.test(k))
    .filter((k) => !String(config.value[k]).includes("media=cdrom"))
    .sort(),
);

async function refresh() {
  if (!activeId.value) return;
  loading.value = true;
  error.value = "";
  try {
    config.value = await api.guestConfig(activeId.value, node, kind, vmid);
    cores.value = String(config.value.cores ?? "");
    memory.value = String(config.value.memory ?? "");
    if (!disk.value && diskKeys.value.length > 0) disk.value = diskKeys.value[0];
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

async function saveHardware() {
  if (!activeId.value) return;
  saving.value = true;
  notice.value = "";
  error.value = "";
  try {
    const params: Record<string, string> = {};
    if (cores.value && cores.value !== String(config.value.cores ?? "")) {
      params.cores = cores.value;
    }
    if (memory.value && memory.value !== String(config.value.memory ?? "")) {
      params.memory = memory.value;
    }
    if (Object.keys(params).length > 0) {
      await api.setGuestConfig(activeId.value, node, kind, vmid, params);
      notice.value = "Config updated.";
    }
    if (disk.value && grow.value) {
      await api.resizeDisk(activeId.value, node, kind, vmid, disk.value, `+${grow.value}G`);
      notice.value += " Disk resize started.";
      grow.value = "";
    }
    await refresh();
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}

onMounted(refresh);
</script>

<template>
  <div>
    <div class="head">
      <h1>{{ kind === "qemu" ? "VM" : "CT" }} {{ vmid }} <small>on {{ node }}</small></h1>
      <router-link :to="`/guests/${node}/${kind}/${vmid}/console`">
        Console
      </router-link>
      <router-link to="/guests">
        Back to list
      </router-link>
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
    <p
      v-if="error"
      class="error"
    >
      {{ error }}
    </p>
    <p
      v-if="notice"
      class="notice"
    >
      {{ notice }}
    </p>

    <div
      v-if="activeId && !loading"
      class="cols"
    >
      <section class="card">
        <h2>Hardware</h2>
        <label>
          Cores
          <input
            v-model="cores"
            type="number"
            min="1"
          >
        </label>
        <label>
          Memory (MiB)
          <input
            v-model="memory"
            type="number"
            min="16"
          >
        </label>
        <label v-if="diskKeys.length > 0">
          Grow disk
          <span class="row">
            <select v-model="disk">
              <option
                v-for="d in diskKeys"
                :key="d"
                :value="d"
              >{{ d }}</option>
            </select>
            <input
              v-model="grow"
              type="number"
              min="1"
              placeholder="GiB"
            >
            <span>GiB</span>
          </span>
        </label>
        <p class="hint">
          Disks can only grow — Proxmox does not shrink volumes.
        </p>
        <button
          :disabled="saving"
          @click="saveHardware"
        >
          {{ saving ? "Applying…" : "Apply" }}
        </button>
      </section>

      <section class="card">
        <h2>Config</h2>
        <table>
          <tbody>
            <tr
              v-for="[k, v] in configRows"
              :key="k"
            >
              <td class="key">
                {{ k }}
              </td>
              <td>{{ v }}</td>
            </tr>
          </tbody>
        </table>
      </section>
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

.head small {
  font-weight: normal;
  opacity: 0.6;
}

.cols {
  display: grid;
  grid-template-columns: 280px 1fr;
  gap: 16px;
  align-items: start;
}

.card {
  border: 1px solid #ccc3;
  border-radius: 8px;
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.card h2 {
  margin: 0;
  font-size: 1em;
}

label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 0.9em;
}

.row {
  display: flex;
  gap: 6px;
  align-items: center;
}

.row input {
  width: 70px;
}

.hint {
  font-size: 0.8em;
  opacity: 0.6;
  margin: 0;
}

table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.9em;
}

td {
  padding: 4px 8px;
  border-bottom: 1px solid #ccc2;
  word-break: break-all;
}

.key {
  font-weight: 600;
  white-space: nowrap;
  vertical-align: top;
}

.error {
  color: #c33;
}

.notice {
  color: #2a7;
}
</style>
