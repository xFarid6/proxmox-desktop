<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { api, type FirewallRule, type FirewallScope, type GuestKind } from "../api";
import { activeId } from "../stores/connections";
import { guests, nodes, refreshCluster } from "../stores/cluster";
import { toast } from "../stores/toast";

type ScopeKind = "cluster" | "node" | "guest";

const scopeKind = ref<ScopeKind>("cluster");
const node = ref("");
const guestId = ref(""); // ClusterResource.id of the selected guest

const rules = ref<FirewallRule[]>([]);
const fwEnabled = ref(false);
const hasOptions = ref(false);
const loading = ref(false);
const error = ref("");
const confirmDelete = ref(-1);

// Add-rule form.
const newType = ref("in");
const newAction = ref("ACCEPT");
const newProto = ref("");
const newDport = ref("");
const newSource = ref("");
const newComment = ref("");
const adding = ref(false);

const scope = computed<FirewallScope | null>(() => {
  if (scopeKind.value === "cluster") return {};
  if (scopeKind.value === "node") return node.value ? { node: node.value } : null;
  const g = guests.value.find((x) => x.id === guestId.value);
  if (!g?.node || !g.vmid) return null;
  return { node: g.node, kind: g.type as GuestKind, vmid: g.vmid };
});

async function refresh() {
  if (!activeId.value || !scope.value) {
    rules.value = [];
    return;
  }
  loading.value = true;
  error.value = "";
  confirmDelete.value = -1;
  try {
    rules.value = await api.firewallRules(activeId.value, scope.value);
    // Node scope has no enable flag — the node firewall follows the cluster.
    if (scopeKind.value !== "node") {
      const opts = await api.firewallOptions(activeId.value, scope.value);
      fwEnabled.value = opts.enable === 1;
      hasOptions.value = true;
    } else {
      hasOptions.value = false;
    }
  } catch (e) {
    error.value = String(e);
    rules.value = [];
  } finally {
    loading.value = false;
  }
}

async function toggleEnabled() {
  if (!activeId.value || !scope.value) return;
  try {
    await api.setFirewallOptions(activeId.value, scope.value, {
      enable: fwEnabled.value ? "0" : "1",
    });
    toast(`Firewall ${fwEnabled.value ? "disabled" : "enabled"}`);
    await refresh();
  } catch (e) {
    toast(String(e), "error");
  }
}

async function addRule() {
  if (!activeId.value || !scope.value) return;
  adding.value = true;
  try {
    const params: Record<string, string> = { type: newType.value, action: newAction.value };
    if (newProto.value) params.proto = newProto.value;
    if (newDport.value) params.dport = newDport.value;
    if (newSource.value) params.source = newSource.value;
    if (newComment.value) params.comment = newComment.value;
    await api.addFirewallRule(activeId.value, scope.value, params);
    toast("Rule added");
    newProto.value = newDport.value = newSource.value = newComment.value = "";
    await refresh();
  } catch (e) {
    toast(String(e), "error");
  } finally {
    adding.value = false;
  }
}

async function removeRule(pos: number) {
  if (confirmDelete.value !== pos) {
    confirmDelete.value = pos;
    return;
  }
  confirmDelete.value = -1;
  if (!activeId.value || !scope.value) return;
  try {
    await api.deleteFirewallRule(activeId.value, scope.value, pos);
    toast(`Rule ${pos} deleted`);
    await refresh();
  } catch (e) {
    toast(String(e), "error");
  }
}

onMounted(async () => {
  if (nodes.value.length === 0) await refreshCluster();
  if (!node.value && nodes.value.length > 0) node.value = nodes.value[0].node ?? "";
  if (!guestId.value && guests.value.length > 0) guestId.value = guests.value[0].id;
  await refresh();
});

watch([scopeKind, node, guestId], refresh);
watch(activeId, () => {
  rules.value = [];
});
</script>

<template>
  <div>
    <div class="head">
      <h1>Firewall</h1>
      <label>
        Scope
        <select v-model="scopeKind">
          <option value="cluster">Cluster</option>
          <option value="node">Node</option>
          <option value="guest">Guest</option>
        </select>
      </label>
      <label v-if="scopeKind === 'node'">
        Node
        <select v-model="node">
          <option
            v-for="n in nodes"
            :key="n.id"
            :value="n.node"
          >{{ n.node }}</option>
        </select>
      </label>
      <label v-if="scopeKind === 'guest'">
        Guest
        <select v-model="guestId">
          <option
            v-for="g in guests"
            :key="g.id"
            :value="g.id"
          >{{ g.vmid }} — {{ g.name ?? g.id }}</option>
        </select>
      </label>
      <label
        v-if="hasOptions"
        class="inline"
      >
        <input
          type="checkbox"
          :checked="fwEnabled"
          @change="toggleEnabled"
        >
        Enabled
      </label>
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
          Direction
          <select v-model="newType">
            <option value="in">in</option>
            <option value="out">out</option>
          </select>
        </label>
        <label>
          Action
          <select v-model="newAction">
            <option>ACCEPT</option>
            <option>DROP</option>
            <option>REJECT</option>
          </select>
        </label>
        <label>
          Protocol
          <select v-model="newProto">
            <option value="">any</option>
            <option value="tcp">tcp</option>
            <option value="udp">udp</option>
            <option value="icmp">icmp</option>
          </select>
        </label>
        <label>
          Dest. port
          <input
            v-model="newDport"
            placeholder="e.g. 22,80"
          >
        </label>
        <label>
          Source
          <input
            v-model="newSource"
            placeholder="CIDR / IP"
          >
        </label>
        <label>
          Comment
          <input v-model="newComment">
        </label>
        <button
          :disabled="adding"
          @click="addRule"
        >
          {{ adding ? "Adding…" : "Add rule" }}
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

      <table v-else-if="rules.length > 0">
        <thead>
          <tr>
            <th>Pos</th>
            <th>On</th>
            <th>Type</th>
            <th>Action</th>
            <th>Proto</th>
            <th>Dport</th>
            <th>Source</th>
            <th>Dest</th>
            <th>Comment</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="r in rules"
            :key="r.pos"
          >
            <td>{{ r.pos }}</td>
            <td>
              <span :class="r.enable ? 'ok' : 'off'">{{ r.enable ? "yes" : "no" }}</span>
            </td>
            <td>{{ r.type }}</td>
            <td>{{ r.action }}</td>
            <td>{{ r.proto ?? "—" }}</td>
            <td>{{ r.dport ?? "—" }}</td>
            <td>{{ r.source ?? "—" }}</td>
            <td>{{ r.dest ?? "—" }}</td>
            <td>{{ r.comment ?? "—" }}</td>
            <td>
              <button
                class="danger"
                @click="removeRule(r.pos)"
              >
                {{ confirmDelete === r.pos ? "Confirm?" : "Delete" }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <p v-else-if="!loading">
        No rules in this scope.
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

label.inline {
  flex-direction: row;
  align-items: center;
  gap: 6px;
}

.row input {
  width: 110px;
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

.error {
  color: #c33;
}
</style>
