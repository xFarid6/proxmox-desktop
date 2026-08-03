<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { api, type StreamEndpoint } from "../api";
import { activeConnection, activeId } from "../stores/connections";
import { sortStreams, streamUrl } from "../hostservices";

// The viewer is an <img> pointed at the endpoint: browsers render
// multipart/x-mixed-replace natively, so there is no decoding to do here and
// no bytes proxied through the app. What the app adds is finding the endpoint
// and saying plainly when it is not reachable.
const endpoints = ref<StreamEndpoint[] | null>(null);
const selected = ref<StreamEndpoint | null>(null);
const loading = ref(false);
const error = ref("");
// Bumped on every retry so the URL changes and the webview cannot answer a
// failed reload out of its cache.
const nonce = ref(0);
// null while the first frame is still on its way.
const live = ref<boolean | null>(null);

const sorted = computed(() => sortStreams(endpoints.value ?? []));
const url = computed(() =>
  selected.value && activeConnection.value
    ? streamUrl(activeConnection.value.host, selected.value, nonce.value)
    : "",
);

function open(ep: StreamEndpoint) {
  selected.value = ep;
  live.value = null;
  nonce.value += 1;
}

async function refresh() {
  if (!activeId.value) return;
  loading.value = true;
  error.value = "";
  selected.value = null;
  live.value = null;
  try {
    endpoints.value = await api.hostStreams(activeId.value);
    // One endpoint is the common case (a host has one camera); opening it
    // saves a click and makes "is it live" the first thing on screen.
    const [first] = sortStreams(endpoints.value ?? []);
    if (first) open(first);
  } catch (e) {
    error.value = String(e);
    endpoints.value = null;
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
      <h1>Stream — {{ activeConnection?.name ?? "host" }}</h1>
      <button
        :disabled="loading"
        @click="refresh"
      >
        Rescan
      </button>
    </div>

    <p v-if="!activeId">
      No active connection. Add one under Connections.
    </p>
    <p v-else-if="loading">
      Probing listening ports for a media endpoint…
    </p>
    <p
      v-else-if="error"
      class="error"
    >
      {{ error }}
    </p>

    <template v-else>
      <section class="card">
        <h2>Endpoints</h2>
        <p
          v-if="endpoints === null"
          class="hint"
        >
          This host has no <code>ss</code> or no <code>curl</code>, so its ports cannot be probed
          for a stream.
        </p>
        <p v-else-if="sorted.length === 0">
          No media endpoint among this host's listening ports. Only <code>{{ "/?action=stream" }}</code>
          is probed, so a server that streams from a different path is not found here.
        </p>
        <table
          v-else
          v-cards
        >
          <thead>
            <tr>
              <th>Port</th>
              <th>Kind</th>
              <th>Process</th>
              <th>Content type</th>
              <th />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="ep in sorted"
              :key="ep.port"
            >
              <td>{{ ep.port }}</td>
              <td>{{ ep.kind }}</td>
              <td>{{ ep.process ?? "—" }}</td>
              <td class="ct">
                {{ ep.contentType }}
              </td>
              <td>
                <button
                  :disabled="selected?.port === ep.port"
                  @click="open(ep)"
                >
                  {{ selected?.port === ep.port ? "Viewing" : "View" }}
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </section>

      <section
        v-if="selected"
        class="card"
      >
        <h2>
          {{ selected.kind === "stream" ? "Live view" : "Snapshot" }}
          <span
            v-if="live === false"
            class="state bad"
          >not live</span>
          <button
            class="retry"
            @click="open(selected)"
          >
            Reload
          </button>
        </h2>

        <!-- The probe reached this endpoint over the host's own loopback, so a
             failure here means the service is up and the path from this
             desktop to it is not. Saying which of the two broke is the whole
             point of the tab. -->
        <p
          v-if="live === false"
          class="error"
        >
          The service answered on the host itself, but <code>{{ url }}</code> did not load from
          here. The stream is running and the network path to it is broken — check that the port is
          published outside the host, and that no firewall sits in between.
        </p>

        <img
          :key="url"
          :src="url"
          class="viewer"
          alt="Live stream from the host"
          @load="live = true"
          @error="live = false"
        >
        <p class="hint">
          {{ url }}
        </p>
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
  word-break: break-all;
}

.ct {
  font-size: 0.85em;
  word-break: break-all;
}

.viewer {
  display: block;
  max-width: 100%;
  border-radius: 6px;
  background: #8882;
}

.state {
  font-size: 0.7em;
  margin-left: 8px;
  padding: 2px 8px;
  border-radius: 10px;
  background: #c332;
  color: #c33;
}

.retry {
  margin-left: 12px;
  font-size: 0.8rem;
}

.error {
  color: #c33;
  max-width: 70ch;
}
</style>
