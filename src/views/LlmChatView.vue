<script setup lang="ts">
import { computed, nextTick, onMounted, ref, useTemplateRef } from "vue";
import { useRoute } from "vue-router";
import { Channel } from "@tauri-apps/api/core";
import {
  api,
  type ChatChunk,
  type ChatMessage,
  type GuestKind,
  type LlmEndpoint,
  type ModelFile,
} from "../api";
import { formatBytes } from "../format";
import {
  RELOAD_POLL_MS,
  RELOAD_TIMEOUT_MS,
  appendDelta,
  forgetProbe,
  isUsable,
  modelSwitchProblem,
  probeLlm,
} from "../llm";
import { activeId } from "../stores/connections";
import { toast } from "../stores/toast";

const route = useRoute();
const node = route.params.node as string;
const kind = route.params.kind as GuestKind;
const vmid = Number(route.params.vmid);

const endpoint = ref<LlmEndpoint | null>(null);
const model = ref("");
const messages = ref<ChatMessage[]>([]);
const draft = ref("");
const probing = ref(false);
const streaming = ref(false);
const error = ref("");
const override = ref("");
const editingEndpoint = ref(false);
const log = useTemplateRef<HTMLElement>("log");

// The id the running request answers to, so Cancel has something to name.
let requestId = "";
// Set when a stream died mid-reply. The endpoint may have moved or the guest
// may have restarted, so the next send re-probes before trying to use it.
const stale = ref(false);

// Model switching (#100). `switching` names the file being loaded; while it is
// set the endpoint is expected to be down, which is a progress state and not
// an error.
const models = ref<ModelFile[] | null>(null);
const switching = ref("");
const reloadSeconds = ref(0);

const canSend = computed(
  () => !!draft.value.trim() && !streaming.value && !switching.value && isUsable(endpoint.value),
);

async function scrollToBottom() {
  await nextTick();
  if (log.value) log.value.scrollTop = log.value.scrollHeight;
}

/** The guest's name, which is what a tailnet peer is matched on. LXC calls it
 * `hostname` and QEMU calls it `name`; a guest with neither still probes, just
 * without its tailnet address as a candidate. */
async function guestName(): Promise<string> {
  if (!activeId.value) return "";
  try {
    const config = await api.guestConfig(activeId.value, node, kind, vmid);
    return String(config.hostname ?? config.name ?? "");
  } catch {
    return "";
  }
}

async function probe(force = false) {
  if (!activeId.value) return;
  probing.value = true;
  error.value = "";
  try {
    endpoint.value = await probeLlm(activeId.value, kind, vmid, await guestName(), force);
    override.value = endpoint.value?.baseUrl ?? "";
    if (!model.value || !endpoint.value?.models.includes(model.value)) {
      model.value = endpoint.value?.models[0] ?? "";
    }
    stale.value = false;
  } finally {
    probing.value = false;
  }
  await loadModels();
}

/** The model files on the guest's disk. Absence is not an error: a guest we
 * cannot exec into, or one that keeps its models somewhere unusual, simply
 * gets no switcher. */
async function loadModels() {
  if (!activeId.value) return;
  try {
    models.value = await api.llmModelsAvailable(
      activeId.value,
      kind,
      vmid,
      endpoint.value?.baseUrl ?? null,
    );
  } catch {
    models.value = null;
  }
}

/** Switch the served model and wait for the endpoint to come back.
 *
 * The reload takes about a minute for a 10 GiB model, during which the endpoint
 * is **down** — expected, not an error. The chat stays disabled and the elapsed
 * time is on screen so the wait is visibly progress rather than a hang. */
async function switchTo(m: ModelFile) {
  if (!activeId.value || modelSwitchProblem(m) || switching.value) return;
  const base = endpoint.value?.baseUrl;
  switching.value = m.file;
  reloadSeconds.value = 0;
  error.value = "";
  try {
    await api.llmSwitchModel(activeId.value, kind, vmid, m.file);
  } catch (e) {
    error.value = String(e);
    switching.value = "";
    return;
  }

  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const deadline = Date.now() + RELOAD_TIMEOUT_MS;
  let healthy = false;
  while (Date.now() < deadline) {
    await sleep(RELOAD_POLL_MS);
    reloadSeconds.value = Math.round((RELOAD_TIMEOUT_MS - (deadline - Date.now())) / 1000);
    // The address does not change across a restart, so the old base is still
    // the right thing to poll -- a re-probe here would race the container
    // being recreated and could cache a miss.
    if (base && (await api.llmHealth(base).catch(() => false))) {
      healthy = true;
      break;
    }
  }
  switching.value = "";
  if (!healthy) {
    error.value =
      `${m.file} did not start serving within ${RELOAD_TIMEOUT_MS / 1000}s. The previous model ` +
      `may not have come back either — check the container on the guest, then Re-probe.`;
    stale.value = true;
    return;
  }
  // The alias changes with the model, so the picker has to be rebuilt from a
  // fresh probe rather than kept.
  if (activeId.value) forgetProbe(activeId.value, kind, vmid);
  await probe(true);
  toast(`Now serving ${m.file}`);
}

/** Pin the endpoint the user typed, or clear the pin when the box is empty.
 * Either way the probe cache is dropped so the next probe reflects the change. */
async function saveEndpoint() {
  if (!activeId.value) return;
  const value = override.value.trim();
  try {
    await api.llmSetEndpoint(activeId.value, kind, vmid, value || null);
    forgetProbe(activeId.value, kind, vmid);
    editingEndpoint.value = false;
    await probe(true);
    toast(value ? "Endpoint pinned" : "Endpoint cleared");
  } catch (e) {
    error.value = String(e);
  }
}

async function send() {
  if (!canSend.value || !endpoint.value) return;
  if (stale.value) {
    await probe(true);
    if (!isUsable(endpoint.value)) return;
  }

  messages.value = [...messages.value, { role: "user", content: draft.value.trim() }];
  draft.value = "";
  error.value = "";
  streaming.value = true;
  requestId = crypto.randomUUID();
  await scrollToBottom();

  const channel = new Channel<ChatChunk>();
  channel.onmessage = (chunk) => {
    if (chunk.delta) {
      messages.value = appendDelta(messages.value, chunk.delta);
      void scrollToBottom();
    }
    if (chunk.done) {
      streaming.value = false;
      if (chunk.error) {
        // The partial reply stays on screen: it is real output, and hiding it
        // would lose the only evidence of how far the model got.
        error.value = chunk.error;
        stale.value = true;
      }
    }
  };

  try {
    await api.llmChat(endpoint.value.baseUrl, model.value, messages.value, requestId, channel);
  } catch (e) {
    error.value = String(e);
    stale.value = true;
  } finally {
    streaming.value = false;
  }
}

async function cancel() {
  if (!requestId) return;
  try {
    await api.llmCancel(requestId);
  } catch (e) {
    error.value = String(e);
  }
}

onMounted(() => probe());
</script>

<template>
  <div>
    <div class="head">
      <h1>
        LLM — {{ kind === "qemu" ? "VM" : "CT" }} {{ vmid }}
        <small>on {{ node }}</small>
      </h1>
      <button
        :disabled="probing || streaming"
        @click="probe(true)"
      >
        Re-probe
      </button>
      <router-link :to="`/guests/${node}/${kind}/${vmid}`">
        Back
      </router-link>
    </div>

    <p v-if="!activeId">
      No active connection. Add one under Connections.
    </p>
    <p v-else-if="probing">
      Looking for an OpenAI-compatible endpoint on this guest…
    </p>

    <template v-else>
      <section class="card endpoint">
        <template v-if="!editingEndpoint">
          <span v-if="endpoint">
            <code>{{ endpoint.baseUrl }}</code>
            <span
              v-if="endpoint.manual"
              class="badge"
            >pinned</span>
          </span>
          <span v-else>No endpoint found on this guest.</span>
          <button @click="editingEndpoint = true">
            {{ endpoint ? "Change" : "Set address" }}
          </button>
        </template>
        <template v-else>
          <input
            v-model="override"
            placeholder="http://100.111.194.35:8080"
            class="url"
            @keyup.enter="saveEndpoint"
          >
          <button @click="saveEndpoint">
            Save
          </button>
          <button @click="editingEndpoint = false">
            Cancel
          </button>
        </template>
      </section>

      <!-- The Proxmox-visible IP is often not the service address: a guest on a
           NAT bridge is reached over the tailnet or a host port-forward
           instead. Saying so here is what turns "it doesn't work" into a fix. -->
      <p
        v-if="!endpoint"
        class="hint"
      >
        Nothing answered <code>/v1/models</code> on this guest's address, on
        {{ node }}, or on the guest's tailnet address. If the service is published
        somewhere else — a port-forward on another host, a reverse proxy, a
        non-standard port — set the address above and it will be used as-is.
      </p>
      <p
        v-else-if="!isUsable(endpoint)"
        class="hint"
      >
        The endpoint answered but has no model loaded, so there is nothing to
        send a message to.
      </p>

      <template v-else>
        <section class="card">
          <label class="model">
            Model
            <select
              v-model="model"
              :disabled="streaming"
            >
              <option
                v-for="m in endpoint.models"
                :key="m"
                :value="m"
              >
                {{ m }}
              </option>
            </select>
          </label>
          <p
            v-if="endpoint.models.length === 1"
            class="hint"
          >
            This server exposes one model — <code>llama-server</code> loads exactly
            one per process, so switching means restarting it below.
          </p>
        </section>

        <section
          v-if="models"
          class="card"
        >
          <h2>Models on this guest</h2>
          <!-- The reload window is the whole design problem here: the endpoint
               is down for it, and that is the guest doing what it was asked. -->
          <p
            v-if="switching"
            class="hint"
          >
            Loading <code>{{ switching }}</code> — the endpoint is down while it
            reads the file, usually about a minute. {{ reloadSeconds }}s elapsed.
          </p>
          <table v-cards>
            <thead>
              <tr>
                <th>File</th>
                <th>Size</th>
                <th />
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="m in models"
                :key="m.path"
              >
                <td>
                  {{ m.file }}
                  <span
                    v-if="m.loaded"
                    class="badge"
                  >loaded</span>
                </td>
                <td>{{ formatBytes(m.bytes) }}</td>
                <td>
                  <button
                    :disabled="!!modelSwitchProblem(m) || !!switching || streaming"
                    :title="modelSwitchProblem(m) ?? ''"
                    @click="switchTo(m)"
                  >
                    {{ switching === m.file ? "Loading…" : "Use" }}
                  </button>
                  <span
                    v-if="!m.fits"
                    class="hint"
                  >won't fit in RAM</span>
                </td>
              </tr>
            </tbody>
          </table>
        </section>

        <section
          ref="log"
          class="card log"
        >
          <p
            v-if="messages.length === 0"
            class="hint"
          >
            No messages yet.
          </p>
          <div
            v-for="(m, i) in messages"
            :key="i"
            class="msg"
            :class="m.role"
          >
            <span class="role">{{ m.role }}</span>
            <pre>{{ m.content }}</pre>
          </div>
        </section>

        <p
          v-if="error"
          class="error"
        >
          {{ error }}
        </p>

        <section class="card compose">
          <textarea
            v-model="draft"
            rows="3"
            placeholder="Ask the model something…"
            :disabled="streaming"
            @keydown.ctrl.enter="send"
          />
          <div class="actions">
            <button
              v-if="streaming"
              @click="cancel"
            >
              Cancel
            </button>
            <button
              v-else
              :disabled="!canSend"
              @click="send"
            >
              Send
            </button>
            <span class="hint">Ctrl+Enter sends</span>
          </div>
        </section>
      </template>
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

.card + .card,
.card + p {
  margin-top: 16px;
}

.endpoint {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}

.url {
  flex: 1 1 24ch;
}

.badge {
  margin-left: 8px;
  font-size: 0.75em;
  padding: 2px 8px;
  border-radius: 10px;
  background: rgba(229, 112, 0, 0.15);
  color: #e57000;
}

.model {
  display: flex;
  align-items: center;
  gap: 8px;
}

.log {
  max-height: 55vh;
  overflow-y: auto;
}

.msg + .msg {
  margin-top: 12px;
}

.msg pre {
  margin: 2px 0 0;
  white-space: pre-wrap;
  word-break: break-word;
}

.msg .role {
  font-size: 0.75em;
  text-transform: uppercase;
  opacity: 0.6;
}

.msg.user pre {
  opacity: 0.85;
}

.compose textarea {
  width: 100%;
  resize: vertical;
}

.actions {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 8px;
}

.hint {
  max-width: 70ch;
  font-size: 0.85em;
  opacity: 0.8;
}

.error {
  color: #c33;
  max-width: 70ch;
}
</style>
