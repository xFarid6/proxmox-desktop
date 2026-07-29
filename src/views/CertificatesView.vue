<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import {
  api,
  type AcmeAccountDetail,
  type AcmeAccountEntry,
  type AcmePlugin,
  type CertificateInfo,
} from "../api";
import { expiryLabel, expiryState } from "../certs";
import { formatTimestamp } from "../format";
import { nodes, refreshCluster } from "../stores/cluster";
import { activeId, connections, refreshConnections } from "../stores/connections";
import { toast } from "../stores/toast";

const node = ref("");
const certs = ref<CertificateInfo[]>([]);
const accounts = ref<AcmeAccountEntry[]>([]);
const accountName = ref("");
const accountDetail = ref<AcmeAccountDetail | null>(null);
const plugins = ref<AcmePlugin[]>([]);
const loading = ref(false);
const busy = ref(false);
const error = ref("");
// ACME is optional and its endpoints need Sys.Modify, so a failure there must
// not take the certificate listing down with it.
const acmeError = ref("");

// Upload form. The key is only ever held here and is cleared the moment the
// request returns — it is never toasted, logged or echoed in an error.
const certPem = ref("");
const keyPem = ref("");
const force = ref(false);
const restart = ref(true);
const confirmRevert = ref(false);
const acmeForce = ref(false);

/** The saved connection the app is talking through. Its `acceptInvalidCerts`
 * is what a replaced certificate makes reconsiderable. */
const activeConnection = computed(
  () => connections.value.find((c) => c.id === activeId.value) ?? null,
);
const pinnedToSelfSigned = computed(() => !!activeConnection.value?.acceptInvalidCerts);

// Set once a certificate has been replaced in this session: only then is there
// a reason to re-check whether the self-signed opt-in is still needed.
const replaced = ref(false);
const probe = ref<"idle" | "busy" | "ok" | "failed">("idle");
const probeMsg = ref("");
const showReverify = computed(() => replaced.value && pinnedToSelfSigned.value);

async function refresh() {
  error.value = "";
  acmeError.value = "";
  confirmRevert.value = false;
  if (!activeId.value) {
    certs.value = [];
    accounts.value = [];
    plugins.value = [];
    return;
  }
  void refreshCluster();
  // Only ConnectionsView loads the connection list, and the re-verify prompt
  // needs the active connection's flag even when this view is opened first.
  void refreshConnections();
  if (!node.value || !nodes.value.some((n) => n.node === node.value)) {
    node.value = nodes.value[0]?.node ?? "";
  }
  if (!node.value) return;
  loading.value = true;
  const [info, accs, plugs] = await Promise.allSettled([
    api.certificatesInfo(activeId.value, node.value),
    api.acmeAccounts(activeId.value),
    api.acmePlugins(activeId.value),
  ]);
  if (info.status === "fulfilled") {
    certs.value = info.value;
  } else {
    certs.value = [];
    error.value = String(info.reason);
  }
  if (accs.status === "fulfilled") {
    accounts.value = accs.value;
  } else {
    accounts.value = [];
    acmeError.value = String(accs.reason);
  }
  plugins.value = plugs.status === "fulfilled" ? plugs.value : [];
  if (!accounts.value.some((a) => a.name === accountName.value)) {
    accountName.value = accounts.value[0]?.name ?? "";
  }
  await loadAccount();
  loading.value = false;
}

async function loadAccount() {
  accountDetail.value = null;
  if (!activeId.value || !accountName.value) return;
  try {
    accountDetail.value = await api.acmeAccount(activeId.value, accountName.value);
  } catch (e) {
    acmeError.value = String(e);
  }
}

async function act(what: string, fn: () => Promise<unknown>) {
  busy.value = true;
  try {
    await fn();
    toast(what);
    await refresh();
  } catch (e) {
    toast(String(e), "error");
  } finally {
    busy.value = false;
  }
}

/** A replaced certificate is the moment the saved connection's self-signed
 * opt-in may have become unnecessary. Any earlier probe answered for the old
 * certificate, so it is discarded here. */
function certificateReplaced() {
  replaced.value = true;
  probe.value = "idle";
  probeMsg.value = "";
}

async function upload() {
  if (!activeId.value || !certPem.value || !keyPem.value) return;
  const params: Record<string, string> = { certificates: certPem.value, key: keyPem.value };
  if (force.value) params.force = "1";
  if (restart.value) params.restart = "1";
  busy.value = true;
  error.value = "";
  try {
    const info = await api.uploadCertificate(activeId.value, node.value, params);
    certPem.value = "";
    keyPem.value = "";
    toast(`Installed ${info.filename ?? "certificate"} on ${node.value}`);
    certificateReplaced();
    await refresh();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

function revert() {
  if (!confirmRevert.value) {
    confirmRevert.value = true;
    return;
  }
  confirmRevert.value = false;
  void act(`Reverted ${node.value} to its self-signed certificate`, async () => {
    await api.deleteCustomCertificate(activeId.value!, node.value, true);
    // Strict TLS cannot validate a self-signed certificate, so there is
    // nothing left to re-verify.
    replaced.value = false;
    probe.value = "idle";
  });
}

function order() {
  void act(`ACME order started on ${node.value}`, async () => {
    await api.acmeOrderCertificate(activeId.value!, node.value);
    certificateReplaced();
  });
}

function renew() {
  void act(`ACME renewal started on ${node.value}`, async () => {
    await api.acmeRenewCertificate(activeId.value!, node.value, acmeForce.value);
    certificateReplaced();
  });
}

/** Check that the new certificate actually validates before offering to drop
 * the opt-in: same host, same saved token, strict TLS. Only a success unlocks
 * the flip; a failure leaves the connection exactly as it was. */
async function probeStrict() {
  const conn = activeConnection.value;
  if (!conn) return;
  probe.value = "busy";
  probeMsg.value = "";
  try {
    const v = await api.testConnection({
      host: conn.host,
      acceptInvalidCerts: false,
      connectionId: conn.id,
    });
    probe.value = "ok";
    probeMsg.value = `Verified — Proxmox VE ${v.version} over a certificate this machine trusts.`;
  } catch (e) {
    probe.value = "failed";
    probeMsg.value = String(e);
  }
}

async function trustStrict() {
  const conn = activeConnection.value;
  if (!conn || probe.value !== "ok") return;
  try {
    // Passing no token leaves the stored one (and the SSH secret) untouched;
    // every later call builds its client from this saved flag.
    await api.saveConnection({ ...conn, acceptInvalidCerts: false });
    await refreshConnections();
    toast(`${conn.name} now requires a valid certificate`);
    replaced.value = false;
  } catch (e) {
    toast(String(e), "error");
  }
}

onMounted(refresh);
watch(activeId, () => {
  node.value = "";
  replaced.value = false;
  void refresh();
});
// The cluster store is usually still loading on mount, so there is no node to
// query yet; pick one up as soon as it arrives.
watch(nodes, () => {
  if (!node.value) void refresh();
});
</script>

<template>
  <div>
    <div class="head">
      <h1>Certificates</h1>
      <label v-if="activeId">
        Node
        <select
          v-model="node"
          @change="refresh"
        >
          <option
            v-for="n in nodes"
            :key="n.id"
            :value="n.node"
          >{{ n.node }}</option>
        </select>
      </label>
      <button @click="refresh">
        Refresh
      </button>
    </div>

    <p v-if="!activeId">
      No active connection. Add one under Connections.
    </p>

    <template v-else>
      <p
        v-if="error"
        class="error"
      >
        {{ error }}
      </p>

      <div
        v-if="showReverify"
        class="card reverify"
      >
        <h2 class="flush">
          This connection still accepts self-signed certificates
        </h2>
        <p class="hint">
          <strong>{{ activeConnection?.name }}</strong> ({{ activeConnection?.host }}) is saved
          with the self-signed opt-in, so it would trust any certificate. Now that
          {{ node }} has a new one, check whether the connection validates without it —
          for an ACME order, wait for the task to finish first.
        </p>
        <div class="row">
          <button
            :disabled="probe === 'busy'"
            @click="probeStrict"
          >
            {{ probe === "busy" ? "Checking…" : "Check with strict TLS" }}
          </button>
          <button
            :disabled="probe !== 'ok'"
            @click="trustStrict"
          >
            Require a valid certificate from now on
          </button>
        </div>
        <p
          v-if="probeMsg"
          :class="probe === 'ok' ? 'ok' : 'error'"
        >
          {{ probeMsg }}
        </p>
        <p
          v-if="probe === 'failed'"
          class="hint"
        >
          The opt-in is unchanged. Either the certificate does not cover
          {{ activeConnection?.host }}, its issuer is not trusted by this machine, or
          pveproxy has not picked it up yet.
        </p>
      </div>

      <p v-if="loading">
        Loading…
      </p>

      <table
        v-if="certs.length > 0"
        v-cards
      >
        <thead>
          <tr>
            <th>File</th>
            <th>Subject</th>
            <th>Issuer</th>
            <th>SAN</th>
            <th>Valid</th>
            <th>Expiry</th>
            <th>Key</th>
            <th>Fingerprint</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="(c, i) in certs"
            :key="c.filename ?? i"
          >
            <td>{{ c.filename ?? "—" }}</td>
            <td>{{ c.subject ?? "—" }}</td>
            <td>{{ c.issuer ?? "—" }}</td>
            <td>{{ c.san?.join(", ") || "—" }}</td>
            <td>
              {{ formatTimestamp(c.notbefore) }}
              <small>to {{ formatTimestamp(c.notafter) }}</small>
            </td>
            <td>
              <span
                class="badge"
                :class="expiryState(c.notafter)"
              >{{ expiryLabel(c.notafter) }}</span>
            </td>
            <td>{{ c.public_key_type ?? "—" }} {{ c.public_key_bits ?? "" }}</td>
            <td class="fp">
              <small>{{ c.fingerprint ?? "—" }}</small>
            </td>
          </tr>
        </tbody>
      </table>
      <p v-else-if="!loading">
        No certificates reported for {{ node || "this cluster" }}.
      </p>

      <p
        v-if="pinnedToSelfSigned"
        class="hint"
      >
        Heads up: <strong>{{ activeConnection?.name }}</strong> is saved with “accept
        self-signed certificate”, so this app does not check what
        {{ activeConnection?.host }} presents. Replace the certificate below and the
        prompt to re-verify appears here.
      </p>

      <h2>Upload a custom certificate</h2>
      <p class="hint">
        Both fields are PEM. The chain goes leaf first, then any intermediates; the
        key must match the leaf and must not be passphrase-protected. The key is sent
        to {{ node }} and kept nowhere in this app.
      </p>
      <div class="stack">
        <label>
          Certificate chain
          <textarea
            v-model="certPem"
            rows="6"
            spellcheck="false"
            placeholder="-----BEGIN CERTIFICATE-----"
          />
        </label>
        <label>
          Private key
          <textarea
            v-model="keyPem"
            rows="6"
            spellcheck="false"
            placeholder="-----BEGIN PRIVATE KEY-----"
          />
        </label>
        <label class="check">
          <input
            v-model="force"
            type="checkbox"
          >
          overwrite the existing custom certificate (<code>force</code>)
        </label>
        <label class="check">
          <input
            v-model="restart"
            type="checkbox"
          >
          restart <code>pveproxy</code> so it is served immediately
        </label>
        <p class="hint">
          Restarting pveproxy drops every open web session and console on
          {{ node }} — running guests are untouched, and this listing may need a
          Refresh while it comes back. Without the restart the node keeps serving the
          old certificate until pveproxy next restarts.
        </p>
        <div class="row">
          <button
            :disabled="busy || !certPem || !keyPem"
            @click="upload"
          >
            Upload certificate
          </button>
          <button
            class="danger"
            :disabled="busy"
            @click="revert"
          >
            {{
              confirmRevert
                ? `Confirm: revert ${node} to its self-signed certificate?`
                : "Revert to self-signed"
            }}
          </button>
        </div>
      </div>

      <h2>ACME</h2>
      <p
        v-if="acmeError"
        class="error"
      >
        {{ acmeError }}
      </p>
      <p
        v-if="accounts.length === 0"
        class="hint"
      >
        No ACME account is registered on this cluster. Register one in the Proxmox web
        UI (Datacenter → ACME) — account registration and DNS plugin configuration are
        not handled here. Ordering below will fail until then.
      </p>
      <template v-else>
        <div class="row">
          <label>
            Account
            <select
              v-model="accountName"
              @change="loadAccount"
            >
              <option
                v-for="a in accounts"
                :key="a.name"
                :value="a.name"
              >{{ a.name }}</option>
            </select>
          </label>
        </div>
        <div
          v-if="accountDetail"
          class="card"
        >
          <p class="hint">
            status: {{ accountDetail.account?.status ?? "unknown" }}
          </p>
          <p class="hint">
            contact: {{ accountDetail.account?.contact?.join(", ") || "—" }}
          </p>
          <p class="hint">
            directory: {{ accountDetail.directory ?? "—" }}
          </p>
          <p class="hint">
            terms accepted: {{ accountDetail.tos ?? "—" }}
          </p>
        </div>
      </template>

      <div class="row">
        <button
          :disabled="busy"
          @click="order"
        >
          Order certificate for {{ node }}
        </button>
        <button
          :disabled="busy"
          @click="renew"
        >
          Renew certificate
        </button>
        <label class="check">
          <input
            v-model="acmeForce"
            type="checkbox"
          >
          renew even if it is more than 30 days from expiry (<code>force</code>)
        </label>
      </div>
      <p class="hint">
        Both run as a task on {{ node }} and restart pveproxy when they succeed, using
        the ACME domains configured for that node. Watch it under Tasks.
      </p>

      <h2>Challenge plugins</h2>
      <p class="hint">
        Listed read-only — configure plugins in the Proxmox web UI.
      </p>
      <table
        v-if="plugins.length > 0"
        v-cards
      >
        <thead>
          <tr>
            <th>Plugin</th>
            <th>Type</th>
            <th>DNS API</th>
            <th>Validation delay</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="p in plugins"
            :key="p.plugin"
          >
            <td>{{ p.plugin }}</td>
            <td>{{ p.type ?? "—" }}</td>
            <td>{{ p.api ?? "—" }}</td>
            <td>{{ p["validation-delay"] ?? "—" }}</td>
          </tr>
        </tbody>
      </table>
      <p v-else>
        No challenge plugins configured. HTTP validation on port 80 is used by default.
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

h2 {
  margin-top: 28px;
}

h2.flush {
  margin-top: 0;
  font-size: 1em;
}

.card {
  border: 1px solid #ccc3;
  border-radius: 8px;
  padding: 12px 16px;
  margin: 12px 0;
}

.reverify {
  border-color: #e5a00080;
}

.row {
  display: flex;
  gap: 12px;
  align-items: flex-end;
  flex-wrap: wrap;
  margin: 12px 0;
}

.stack {
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-width: 640px;
}

label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 0.9em;
}

label.check {
  flex-direction: row;
  align-items: center;
  gap: 6px;
  font-size: 0.85em;
  opacity: 0.8;
}

textarea {
  font-family: monospace;
  font-size: 0.85em;
  resize: vertical;
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

td small {
  opacity: 0.6;
}

.fp {
  max-width: 180px;
  overflow-wrap: anywhere;
}

.badge {
  display: inline-block;
  padding: 1px 8px;
  border-radius: 10px;
  font-size: 0.8em;
  white-space: nowrap;
  border: 1px solid currentcolor;
}

.badge.ok {
  color: #2a7;
}

.badge.expiring {
  color: #e5a000;
}

.badge.expired {
  color: #c33;
}

.badge.unknown {
  opacity: 0.6;
}

.ok {
  color: #2a7;
}

.danger {
  color: #c33;
}

.hint {
  font-size: 0.85em;
  opacity: 0.7;
  margin: 6px 0;
}

.error {
  color: #c33;
}
</style>
