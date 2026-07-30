<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { api, type NetworkInterface } from "../api";
import {
  BOND_MODES,
  CREATABLE_KINDS,
  HASH_POLICIES,
  blankForm,
  buildIfaceParams,
  canEdit,
  formFromIface,
  needsHashPolicy,
  normalizePorts,
  pendingRisk,
  slaveCandidates,
  validateIface,
  type IfaceForm,
  type IfaceKind,
} from "../network";
import { nodes, refreshCluster } from "../stores/cluster";
import { activeId, connections } from "../stores/connections";
import { toast } from "../stores/toast";

const node = ref("");
const interfaces = ref<NetworkInterface[]>([]);
/** Diff of edits staged in `/etc/network/interfaces.new`; null when clean. */
const changes = ref<string | null>(null);
const loading = ref(false);
const error = ref("");

const form = ref<IfaceForm>(blankForm());
const formOpen = ref(false);
/** Name of the interface being edited, or null when the form creates one. */
const editing = ref<string | null>(null);
const formError = ref("");

const busy = ref(false);
const confirmApply = ref(false);
const confirmRevert = ref(false);
const confirmDelete = ref("");

/** The address this app reaches PVE on — the guard rail compares it against
 * the interfaces the staged diff touches. */
const host = computed(() => connections.value.find((c) => c.id === activeId.value)?.host ?? "");

/** Set when applying could cut the link we are talking over. */
const risk = computed(() => pendingRisk(changes.value, interfaces.value, host.value));

const candidates = computed(() => slaveCandidates(interfaces.value, form.value.iface));

/** Anything a VLAN can sit on. */
const rawDevices = computed(() =>
  interfaces.value
    .filter((i) => i.type !== "vlan" && i.iface !== form.value.iface)
    .map((i) => i.iface)
    .sort((a, b) => a.localeCompare(b)),
);

/** Bridge ports and bond slaves are one space-separated string on the wire
 * and a multi-select in the form. */
const portList = computed<string[]>({
  get: () => {
    const raw = form.value.kind === "bond" ? form.value.slaves : form.value.bridgePorts;
    return normalizePorts(raw).split(" ").filter(Boolean);
  },
  set: (v) => {
    if (form.value.kind === "bond") form.value.slaves = v.join(" ");
    else form.value.bridgePorts = v.join(" ");
  },
});

function resetConfirms() {
  confirmApply.value = confirmRevert.value = false;
  confirmDelete.value = "";
}

async function refreshNetwork() {
  if (!activeId.value || !node.value) {
    interfaces.value = [];
    changes.value = null;
    return;
  }
  loading.value = true;
  error.value = "";
  resetConfirms();
  try {
    const net = await api.nodeNetwork(activeId.value, node.value);
    interfaces.value = net.interfaces.sort((a, b) => a.iface.localeCompare(b.iface));
    changes.value = net.changes;
  } catch (e) {
    error.value = String(e);
    interfaces.value = [];
    changes.value = null;
  } finally {
    loading.value = false;
  }
}

function startCreate(kind: IfaceKind) {
  form.value = blankForm(kind);
  editing.value = null;
  formError.value = "";
  formOpen.value = true;
}

function startEdit(i: NetworkInterface) {
  form.value = formFromIface(i);
  editing.value = i.iface;
  formError.value = "";
  formOpen.value = true;
}

function closeForm() {
  formOpen.value = false;
  formError.value = "";
}

async function submit() {
  if (!activeId.value || !node.value) return;
  const problem = validateIface(form.value);
  if (problem) {
    formError.value = problem;
    return;
  }
  formError.value = "";
  busy.value = true;
  try {
    const params = buildIfaceParams(form.value);
    if (editing.value) {
      await api.updateNetworkIface(activeId.value, node.value, editing.value, params);
      toast(`${editing.value} staged for update`);
    } else {
      await api.createNetworkIface(activeId.value, node.value, params);
      toast(`${form.value.iface} staged for creation`);
    }
    closeForm();
    await refreshNetwork();
  } catch (e) {
    formError.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function remove(name: string) {
  if (confirmDelete.value !== name) {
    confirmDelete.value = name;
    return;
  }
  confirmDelete.value = "";
  if (!activeId.value || !node.value) return;
  busy.value = true;
  try {
    await api.deleteNetworkIface(activeId.value, node.value, name);
    toast(`${name} staged for removal`);
    if (editing.value === name) closeForm();
    await refreshNetwork();
  } catch (e) {
    toast(String(e), "error");
  } finally {
    busy.value = false;
  }
}

async function apply() {
  if (!confirmApply.value) {
    confirmApply.value = true;
    return;
  }
  confirmApply.value = false;
  if (!activeId.value || !node.value) return;
  busy.value = true;
  try {
    const upid = await api.applyNetwork(activeId.value, node.value);
    toast(`Applying on ${node.value} — ${upid}`);
    await refreshNetwork();
  } catch (e) {
    toast(String(e), "error");
  } finally {
    busy.value = false;
  }
}

async function revert() {
  if (!confirmRevert.value) {
    confirmRevert.value = true;
    return;
  }
  confirmRevert.value = false;
  if (!activeId.value || !node.value) return;
  busy.value = true;
  try {
    await api.revertNetwork(activeId.value, node.value);
    toast("Staged changes discarded");
    await refreshNetwork();
  } catch (e) {
    toast(String(e), "error");
  } finally {
    busy.value = false;
  }
}

onMounted(async () => {
  if (nodes.value.length === 0) await refreshCluster();
  if (!node.value && nodes.value.length > 0) node.value = nodes.value[0].node ?? "";
});

watch(node, refreshNetwork);
watch(activeId, () => {
  node.value = "";
  interfaces.value = [];
  changes.value = null;
  closeForm();
});
</script>

<template>
  <div>
    <div class="head">
      <h1>Network</h1>
      <label>
        Node
        <select v-model="node">
          <option
            v-for="n in nodes"
            :key="n.id"
            :value="n.node"
          >
            {{ n.node }}
          </option>
        </select>
      </label>
      <button @click="refreshNetwork">
        Refresh
      </button>
    </div>

    <p v-if="!activeId">
      No active connection. Add one under Connections.
    </p>
    <p v-else-if="loading">
      Loading…
    </p>

    <template v-else>
      <p
        v-if="error"
        class="error"
      >
        {{ error }}
      </p>

      <div
        v-if="changes"
        class="card pending"
      >
        <div class="row spread">
          <strong>Pending changes on {{ node }}</strong>
          <span class="row">
            <button
              class="danger"
              :disabled="busy"
              @click="apply"
            >
              {{ confirmApply ? "Confirm apply?" : "Apply configuration" }}
            </button>
            <button
              :disabled="busy"
              @click="revert"
            >
              {{ confirmRevert ? "Confirm discard?" : "Discard" }}
            </button>
          </span>
        </div>
        <p class="warn">
          Applying runs <code>ifreload -a</code> on {{ node }}. A wrong bridge port,
          a missing gateway or a slave that is already in use takes the node's
          networking down with it — and there is no way to undo it from here,
          only from the console.
        </p>
        <p
          v-if="risk"
          class="warn danger"
        >
          <strong>{{ risk }} carries this connection.</strong> The staged diff
          changes it, so applying will very likely drop this app's link to
          {{ host }}. Make sure you have console or physical access first.
        </p>
        <pre>{{ changes }}</pre>
      </div>
      <p
        v-else-if="node"
        class="hint"
      >
        No staged changes — edits below are written to
        <code>/etc/network/interfaces.new</code> and do nothing until applied.
      </p>

      <div class="head">
        <h2>Interfaces</h2>
        <button
          v-for="k in CREATABLE_KINDS"
          :key="k"
          :disabled="!node || busy"
          @click="startCreate(k)"
        >
          New {{ k }}
        </button>
      </div>

      <div
        v-if="formOpen"
        class="card"
      >
        <div class="row spread">
          <strong>
            {{ editing ? `Edit ${editing}` : `New ${form.kind}` }}
          </strong>
          <button @click="closeForm">
            Cancel
          </button>
        </div>
        <p
          v-if="editing"
          class="hint"
        >
          Saving replaces this interface's whole definition with the fields
          below — Proxmox drops anything left blank.
        </p>

        <div class="row">
          <label>
            Name
            <input
              v-model="form.iface"
              :disabled="!!editing"
              :placeholder="form.kind === 'bond' ? 'bond0' : form.kind === 'vlan' ? 'eno1.100' : 'vmbr1'"
            >
          </label>
          <label>
            IPv4 CIDR
            <input
              v-model="form.cidr"
              class="wide"
              placeholder="10.0.0.2/24"
            >
          </label>
          <label>
            IPv4 gateway
            <input
              v-model="form.gateway"
              class="wide"
              placeholder="10.0.0.1"
            >
          </label>
          <label>
            IPv6 CIDR
            <input
              v-model="form.cidr6"
              class="wide"
              placeholder="fd00::2/64"
            >
          </label>
          <label>
            IPv6 gateway
            <input
              v-model="form.gateway6"
              class="wide"
            >
          </label>
          <label>
            MTU
            <input
              v-model="form.mtu"
              type="number"
              placeholder="1500"
            >
          </label>
          <label class="check">
            <input
              v-model="form.autostart"
              type="checkbox"
            >
            autostart
          </label>
        </div>

        <div
          v-if="form.kind === 'bridge'"
          class="row"
        >
          <label>
            Bridge ports
            <select
              v-model="portList"
              multiple
              size="4"
            >
              <option
                v-for="c in candidates"
                :key="c"
                :value="c"
              >{{ c }}</option>
            </select>
          </label>
          <label class="check">
            <input
              v-model="form.vlanAware"
              type="checkbox"
            >
            VLAN aware
          </label>
        </div>

        <div
          v-if="form.kind === 'bond'"
          class="row"
        >
          <label>
            Slaves
            <select
              v-model="portList"
              multiple
              size="4"
            >
              <option
                v-for="c in candidates"
                :key="c"
                :value="c"
              >{{ c }}</option>
            </select>
          </label>
          <label>
            Mode
            <select v-model="form.bondMode">
              <option
                v-for="m in BOND_MODES"
                :key="m"
                :value="m"
              >{{ m }}</option>
            </select>
          </label>
          <label v-if="needsHashPolicy(form.bondMode)">
            Hash policy
            <select v-model="form.hashPolicy">
              <option
                v-for="p in HASH_POLICIES"
                :key="p"
                :value="p"
              >{{ p }}</option>
            </select>
          </label>
        </div>

        <div
          v-if="form.kind === 'vlan'"
          class="row"
        >
          <label>
            Raw device
            <select v-model="form.vlanRawDevice">
              <option value="">
                from the name
              </option>
              <option
                v-for="d in rawDevices"
                :key="d"
                :value="d"
              >{{ d }}</option>
            </select>
          </label>
          <label>
            VLAN tag
            <input
              v-model="form.vlanTag"
              type="number"
              min="1"
              max="4094"
              placeholder="100"
            >
          </label>
          <span class="hint">
            A <code>device.tag</code> name sets both on its own.
          </span>
        </div>

        <div class="row">
          <label class="grow">
            Comment
            <input
              v-model="form.comments"
              class="wide"
            >
          </label>
          <button
            :disabled="busy"
            @click="submit"
          >
            {{ editing ? "Stage update" : "Stage interface" }}
          </button>
        </div>
        <p
          v-if="formError"
          class="error"
        >
          {{ formError }}
        </p>
      </div>

      <table
        v-if="interfaces.length > 0"
        v-cards
      >
        <thead>
          <tr>
            <th>Interface</th>
            <th>Type</th>
            <th>Active</th>
            <th>Autostart</th>
            <th>Method</th>
            <th>CIDR</th>
            <th>Gateway</th>
            <th>Ports / slaves</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="i in interfaces"
            :key="i.iface"
          >
            <td>{{ i.iface }}</td>
            <td>{{ i.type }}</td>
            <td>
              <span :class="i.active ? 'ok' : 'off'">{{ i.active ? "yes" : "no" }}</span>
            </td>
            <td>{{ i.autostart ? "yes" : "no" }}</td>
            <td>{{ i.method ?? "—" }}</td>
            <td>{{ i.cidr ?? (i.address ? `${i.address}/${i.netmask ?? "?"}` : "—") }}</td>
            <td>{{ i.gateway ?? "—" }}</td>
            <td>{{ i.bridge_ports ?? i.slaves ?? i["vlan-raw-device"] ?? "—" }}</td>
            <td class="row">
              <button
                v-if="canEdit(i)"
                :disabled="busy"
                @click="startEdit(i)"
              >
                Edit
              </button>
              <span
                v-else
                class="hint"
              >read-only</span>
              <button
                class="danger"
                :disabled="busy"
                @click="remove(i.iface)"
              >
                {{ confirmDelete === i.iface ? "Confirm?" : "Delete" }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>

      <p v-else-if="node && !loading">
        No interfaces reported on {{ node }}.
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

.head h1,
.head h2 {
  margin-right: auto;
}

h2 {
  margin-top: 28px;
}

.card {
  border: 1px solid #ccc3;
  border-radius: 8px;
  padding: 12px 16px;
  margin: 12px 0;
}

.card.pending {
  border-color: #e57000;
}

.row {
  display: flex;
  gap: 12px;
  align-items: flex-end;
  flex-wrap: wrap;
}

.row.spread {
  justify-content: space-between;
  align-items: center;
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
}

label.grow {
  flex: 1;
}

.row input {
  width: 110px;
}

.row input.wide {
  width: 160px;
}

.row label.grow input {
  width: 100%;
}

pre {
  max-height: 260px;
  overflow: auto;
  font-size: 0.8em;
  background: #8881;
  padding: 8px 10px;
  border-radius: 6px;
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

.ok {
  color: #2a7;
}

.off {
  opacity: 0.6;
}

.danger {
  color: #c33;
}

.warn {
  font-size: 0.85em;
}

.hint {
  font-size: 0.85em;
  opacity: 0.7;
}

.error {
  color: #c33;
}
</style>
