<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { api, type DockerContainer } from "../api";
import { activeConnection, activeId } from "../stores/connections";
import { isDetached, sortContainers } from "../hostservices";

// Read-only by design (#105): no start/stop/restart here, unlike a guest's
// Docker section. `null` means the host has no docker at all, which is a fact
// to state rather than an error -- same convention as docker_ps.
const containers = ref<DockerContainer[] | null>(null);
const loading = ref(false);
const error = ref("");

const sorted = computed(() => sortContainers(containers.value ?? []));
const detachedCount = computed(() => sorted.value.filter(isDetached).length);

async function refresh() {
  if (!activeId.value) return;
  loading.value = true;
  error.value = "";
  try {
    containers.value = await api.hostDockerPs(activeId.value);
  } catch (e) {
    error.value = String(e);
    containers.value = null;
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
      <h1>Docker — {{ activeConnection?.name ?? "host" }}</h1>
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

    <section
      v-else
      class="card"
    >
      <p
        v-if="containers === null"
        class="hint"
      >
        This host has no <code>docker</code> installed.
      </p>
      <p v-else-if="sorted.length === 0">
        No containers on this host.
      </p>
      <template v-else>
        <p
          v-if="detachedCount > 0"
          class="warn"
        >
          {{ detachedCount }} running container(s) attached to no network — they cannot be reached
          whatever their status says.
        </p>
        <table v-cards>
          <thead>
            <tr>
              <th>Name</th>
              <th>Image</th>
              <th>State</th>
              <th>Ports</th>
              <th>Networks</th>
              <th>Restart</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="c in sorted"
              :key="c.id"
            >
              <td class="key">
                {{ c.name || c.id }}
              </td>
              <td>{{ c.image }}</td>
              <td :title="c.status">
                {{ c.state || c.status }}
              </td>
              <td>{{ c.ports || "—" }}</td>
              <td :class="{ bad: isDetached(c) }">
                {{ c.networks || (isDetached(c) ? "none" : "—") }}
              </td>
              <td>{{ c.restartPolicy ?? "—" }}</td>
            </tr>
          </tbody>
        </table>
      </template>
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

.hint {
  max-width: 70ch;
  font-size: 0.85em;
  opacity: 0.8;
}

.warn {
  max-width: 70ch;
  color: #c33;
}

.bad {
  color: #c33;
}

.error {
  color: #c33;
}
</style>
