<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import {
  api,
  type DockerAction,
  type DockerContainer,
  type GuestKind,
  type HaResource,
} from "../api";
import { guestSid } from "../ha";
import { isUsable, probeLlm } from "../llm";
import { activeId } from "../stores/connections";
import { toast } from "../stores/toast";

const route = useRoute();
const node = route.params.node as string;
const kind = route.params.kind as GuestKind;
const vmid = Number(route.params.vmid);

const config = ref<Record<string, unknown>>({});
const loading = ref(false);
const error = ref("");

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

// Docker-inside-the-guest (issue #65). The section only exists for guests
// that actually run Docker, so `hasDocker` gates the whole card.
const containers = ref<DockerContainer[]>([]);
const hasDocker = ref(false);
const dockerError = ref("");
const dockerBusy = ref<Set<string>>(new Set());
const logsFor = ref("");
const logs = ref("");

const LOG_TAIL = 200;

// HA membership (#18). `haAvailable` gates the button: a cluster whose
// ha-manager refuses the call gets no HA control at all, not a broken one.
const sid = guestSid(kind, vmid);
const ha = ref<HaResource | null>(null);
const haAvailable = ref(false);
const haBusy = ref(false);

// LLM panel (#99). Link only, gated on the probe — the panel itself lives at
// /guests/:node/:kind/:vmid/llm.
const hasLlm = ref(false);

async function loadHa() {
  if (!activeId.value) return;
  try {
    const list = await api.haResources(activeId.value);
    ha.value = list.find((r) => r.sid === sid) ?? null;
    haAvailable.value = true;
  } catch {
    haAvailable.value = false;
  }
}

async function toggleHa() {
  if (!activeId.value) return;
  haBusy.value = true;
  try {
    if (ha.value) {
      await api.deleteHaResource(activeId.value, sid);
      toast(`${sid} removed from HA`);
    } else {
      await api.addHaResource(activeId.value, { sid, state: "started" });
      toast(`${sid} added to HA`);
    }
    await loadHa();
  } catch (e) {
    toast(String(e), "error");
  } finally {
    haBusy.value = false;
  }
}

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
  await Promise.all([loadDocker(), loadHa(), loadLlm()]);
}

/** Whether this guest serves an OpenAI-compatible LLM (#99). Gates the link
 * the same way `hasDocker` gates the Docker card — almost every guest serves
 * none, so absence is the normal case and never an error. Runs after the
 * config load because the probe needs the guest's name to match a tailnet
 * peer, and it is deliberately last: a miss costs seconds. */
async function loadLlm() {
  if (!activeId.value) return;
  const name = String(config.value.hostname ?? config.value.name ?? "");
  hasLlm.value = isUsable(await probeLlm(activeId.value, kind, vmid, name));
}

/** Lists containers, and doubles as the "does this guest run Docker?" probe.
 * A failure here is not shown until the section is already visible: no SSH
 * configured, guest powered off, or no qemu-guest-agent are all ordinary
 * reasons for a guest to have no Docker section, not errors to shout about. */
async function loadDocker() {
  if (!activeId.value) return;
  try {
    const list = await api.dockerPs(activeId.value, kind, vmid);
    hasDocker.value = list !== null;
    containers.value = list ?? [];
    dockerError.value = "";
  } catch (e) {
    if (hasDocker.value) dockerError.value = String(e);
  }
}

/** Acts on the container id, never the name — `docker ps` reports multiple
 * names as one comma-joined field, and the id is always unambiguous. */
async function dockerDo(c: DockerContainer, action: DockerAction) {
  if (!activeId.value) return;
  dockerBusy.value = new Set(dockerBusy.value).add(c.id);
  try {
    await api.dockerAction(activeId.value, kind, vmid, c.id, action);
    toast(`${action} sent to ${c.name || c.id}`);
    await loadDocker();
  } catch (e) {
    toast(String(e), "error");
  } finally {
    const next = new Set(dockerBusy.value);
    next.delete(c.id);
    dockerBusy.value = next;
  }
}

async function showLogs(c: DockerContainer) {
  if (!activeId.value) return;
  logsFor.value = c.name || c.id;
  logs.value = "Loading…";
  try {
    logs.value = await api.dockerLogs(activeId.value, kind, vmid, c.id, LOG_TAIL);
  } catch (e) {
    logs.value = String(e);
  }
}

async function saveHardware() {
  if (!activeId.value) return;
  saving.value = true;
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
      toast("Config updated.");
    }
    if (disk.value && grow.value) {
      await api.resizeDisk(activeId.value, node, kind, vmid, disk.value, `+${grow.value}G`);
      toast("Disk resize started.");
      grow.value = "";
    }
    await refresh();
  } catch (e) {
    toast(String(e), "error");
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
      <span
        v-if="ha"
        class="ha-badge"
      >HA {{ ha.state ?? "started" }}</span>
      <button
        v-if="haAvailable"
        :disabled="haBusy"
        @click="toggleHa"
      >
        {{ ha ? "Remove from HA" : "Add to HA" }}
      </button>
      <router-link :to="`/guests/${node}/${kind}/${vmid}/console`">
        Console
      </router-link>
      <router-link
        v-if="hasLlm"
        :to="`/guests/${node}/${kind}/${vmid}/llm`"
      >
        LLM
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
        <table v-cards>
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

      <section
        v-if="hasDocker"
        class="card wide"
      >
        <h2>Docker</h2>
        <p
          v-if="dockerError"
          class="error"
        >
          {{ dockerError }}
        </p>
        <p
          v-if="containers.length === 0"
          class="hint"
        >
          No containers in this guest.
        </p>
        <table v-else>
          <thead>
            <tr>
              <th>Name</th>
              <th>Image</th>
              <th>State</th>
              <th>Ports</th>
              <th />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="c in containers"
              :key="c.id"
            >
              <td class="key">
                {{ c.name || c.id }}
              </td>
              <td>{{ c.image }}</td>
              <td :title="c.status">
                {{ c.state || c.status }}
              </td>
              <td>{{ c.ports }}</td>
              <td class="row">
                <button
                  v-for="a in c.state === 'running'
                    ? (['restart', 'stop'] as DockerAction[])
                    : (['start'] as DockerAction[])"
                  :key="a"
                  :disabled="dockerBusy.has(c.id)"
                  @click="dockerDo(c, a)"
                >
                  {{ a }}
                </button>
                <button
                  :disabled="dockerBusy.has(c.id)"
                  @click="showLogs(c)"
                >
                  logs
                </button>
              </td>
            </tr>
          </tbody>
        </table>
        <template v-if="logsFor">
          <h3>Last {{ LOG_TAIL }} lines — {{ logsFor }}</h3>
          <pre class="logs">{{ logs }}</pre>
        </template>
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

.ha-badge {
  font-size: 0.8em;
  padding: 2px 8px;
  border-radius: 10px;
  background: #2a72;
  color: #2a7;
}

.cols {
  display: grid;
  grid-template-columns: 280px 1fr;
  gap: 16px;
  align-items: start;
}

/* The container table needs the full width, not the right-hand column. */
.wide {
  grid-column: 1 / -1;
}

.logs {
  margin: 0;
  max-height: 320px;
  overflow: auto;
  font-size: 0.8em;
  white-space: pre-wrap;
  word-break: break-all;
}

th {
  text-align: left;
  padding: 4px 8px;
  font-size: 0.85em;
  opacity: 0.7;
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
</style>
