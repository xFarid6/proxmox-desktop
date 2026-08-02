<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { api, type ListeningPort, type ServiceUnit } from "../api";
import { activeConnection, activeId } from "../stores/connections";
import { isFailed, sortPorts, sortUnits } from "../hostservices";

// Read-only by design (#104): no start/stop/restart here.
// `null` from either command means the tool is not installed on the host,
// which is a fact to state, not an error -- same convention as docker_ps.
const ports = ref<ListeningPort[] | null>(null);
const units = ref<ServiceUnit[] | null>(null);
const loading = ref(false);
const error = ref("");

const sortedPorts = computed(() => sortPorts(ports.value ?? []));
const sortedUnits = computed(() => sortUnits(units.value ?? []));
const failedCount = computed(() => sortedUnits.value.filter(isFailed).length);

async function refresh() {
  if (!activeId.value) return;
  loading.value = true;
  error.value = "";
  try {
    // One round trip each, both over the same cached SSH session.
    [ports.value, units.value] = await Promise.all([
      api.hostPorts(activeId.value),
      api.hostServices(activeId.value),
    ]);
  } catch (e) {
    error.value = String(e);
    ports.value = null;
    units.value = null;
  } finally {
    loading.value = false;
  }
}

onMounted(refresh);
watch(activeId, refresh);
</script>

<template>
  <div>
    <div class="head">
      <h1>Ports &amp; services — {{ activeConnection?.name ?? "host" }}</h1>
      <button
        :disabled="loading"
        @click="refresh"
      >
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

    <template v-else>
      <section class="card">
        <h2>Listening ports</h2>
        <p
          v-if="ports === null"
          class="hint"
        >
          This host has no <code>ss</code> installed, so listening ports cannot be read.
        </p>
        <p v-else-if="sortedPorts.length === 0">
          Nothing is listening on this host.
        </p>
        <template v-else>
          <table v-cards>
            <thead>
              <tr>
                <th>Proto</th>
                <th>Address</th>
                <th>Port</th>
                <th>Process</th>
                <th>PID</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="p in sortedPorts"
                :key="`${p.proto}-${p.address}-${p.port}`"
              >
                <td>{{ p.proto }}</td>
                <td>{{ p.address }}</td>
                <td>{{ p.port }}</td>
                <td>{{ p.process ?? "—" }}</td>
                <td>{{ p.pid ?? "—" }}</td>
              </tr>
            </tbody>
          </table>
          <!-- ss omits the owning process for sockets the SSH user does not
               own, with no warning of its own — say why the column is empty
               rather than letting it read as "no process". -->
          <p
            v-if="sortedPorts.every((p) => p.process === null)"
            class="hint"
          >
            No owning processes shown: <code>ss</code> only names them for sockets the SSH user
            owns. Connect as root to see them.
          </p>
        </template>
      </section>

      <section class="card">
        <h2>
          Services
          <span
            v-if="failedCount > 0"
            class="failed-count"
          >{{ failedCount }} failed</span>
        </h2>
        <p
          v-if="units === null"
          class="hint"
        >
          This host has no <code>systemctl</code>, so services cannot be read.
        </p>
        <p v-else-if="sortedUnits.length === 0">
          No running or failed service units.
        </p>
        <table
          v-else
          v-cards
        >
          <thead>
            <tr>
              <th>Unit</th>
              <th>State</th>
              <th>Description</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="u in sortedUnits"
              :key="u.name"
            >
              <td>{{ u.name }}</td>
              <td>
                <span
                  class="state"
                  :class="{ bad: isFailed(u) }"
                >{{ u.active }} / {{ u.sub }}</span>
              </td>
              <td>{{ u.description || "—" }}</td>
            </tr>
          </tbody>
        </table>
      </section>
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

.card + .card {
  margin-top: 16px;
}

.hint {
  max-width: 70ch;
  font-size: 0.85em;
  opacity: 0.8;
}

.state {
  font-size: 0.85em;
  padding: 2px 8px;
  border-radius: 10px;
  background: #8883;
}

.state.bad {
  background: #c332;
  color: #c33;
}

.failed-count {
  margin-left: 8px;
  font-size: 0.7em;
  color: #c33;
}

.error {
  color: #c33;
}
</style>
