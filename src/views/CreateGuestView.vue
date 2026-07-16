<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { api, type GuestKind, type StorageContent, type StorageSummary } from "../api";
import { activeId } from "../stores/connections";
import { guests, nodes, refreshCluster } from "../stores/cluster";
import { toast } from "../stores/toast";

const router = useRouter();

const kind = ref<GuestKind>("qemu");
const node = ref("");
const vmid = ref(100);
const name = ref("");
const cores = ref(2);
const memory = ref(2048);
const diskStorage = ref("");
const diskSize = ref(32);
const bridge = ref("vmbr0");
// qemu: ISO to boot from; lxc: container template + root password.
const media = ref("");
const password = ref("");

// Advanced (both kinds).
const startAfter = ref(false);
const vlanTag = ref<number>();
// qemu advanced + cloud-init.
const qemuAgent = ref(true);
const ciEnabled = ref(false);
const ciUser = ref("");
const ciPassword = ref("");
const ciSshKeys = ref("");
const ciIpMode = ref<"dhcp" | "static">("dhcp");
const ciIpAddr = ref("");
const ciGw = ref("");
// lxc advanced.
const lxcUnpriv = ref(true);
const lxcNesting = ref(false);
const lxcSshKey = ref("");
const lxcIpMode = ref<"dhcp" | "static">("dhcp");
const lxcIpAddr = ref("");
const lxcGw = ref("");

const storages = ref<StorageSummary[]>([]);
const mediaVolumes = ref<StorageContent[]>([]);
const submitting = ref(false);
const error = ref("");

const diskStorages = computed(() =>
  storages.value.filter((s) =>
    (s.content ?? "").split(",").includes(kind.value === "qemu" ? "images" : "rootdir"),
  ),
);
const mediaKind = computed(() => (kind.value === "qemu" ? "iso" : "vztmpl"));

async function loadStorages() {
  if (!activeId.value || !node.value) return;
  error.value = "";
  try {
    storages.value = await api.nodeStorages(activeId.value, node.value);
    if (!diskStorages.value.some((s) => s.storage === diskStorage.value)) {
      diskStorage.value = diskStorages.value[0]?.storage ?? "";
    }
    // Media (ISO / template) can live on any storage advertising that content.
    const mediaStores = storages.value.filter((s) =>
      (s.content ?? "").split(",").includes(mediaKind.value),
    );
    const lists = await Promise.all(
      mediaStores.map((s) =>
        api.storageContent(activeId.value!, node.value, s.storage, mediaKind.value),
      ),
    );
    mediaVolumes.value = lists.flat().sort((a, b) => a.volid.localeCompare(b.volid));
    media.value = mediaVolumes.value[0]?.volid ?? "";
  } catch (e) {
    error.value = String(e);
  }
}

async function submit() {
  if (!activeId.value || !node.value) return;
  submitting.value = true;
  error.value = "";
  try {
    const params: Record<string, string> = {
      vmid: String(vmid.value),
      cores: String(cores.value),
      memory: String(memory.value),
    };
    if (startAfter.value) params.start = "1";
    const tag = vlanTag.value ? `,tag=${vlanTag.value}` : "";
    if (kind.value === "qemu") {
      if (name.value) params.name = name.value;
      params.scsihw = "virtio-scsi-pci";
      params.scsi0 = `${diskStorage.value}:${diskSize.value}`;
      params.net0 = `virtio,bridge=${bridge.value}${tag}`;
      params.ostype = "l26";
      if (qemuAgent.value) params.agent = "1";
      if (media.value) params.ide2 = `${media.value},media=cdrom`;
      if (ciEnabled.value) {
        // Cloud-init drive on ide0 — ide2 may hold the install ISO.
        params.ide0 = `${diskStorage.value}:cloudinit`;
        if (ciUser.value) params.ciuser = ciUser.value;
        if (ciPassword.value) params.cipassword = ciPassword.value;
        // PVE expects sshkeys pre-urlencoded.
        if (ciSshKeys.value.trim()) params.sshkeys = encodeURIComponent(ciSshKeys.value.trim());
        params.ipconfig0 =
          ciIpMode.value === "dhcp"
            ? "ip=dhcp"
            : `ip=${ciIpAddr.value}${ciGw.value ? `,gw=${ciGw.value}` : ""}`;
      }
    } else {
      if (name.value) params.hostname = name.value;
      params.ostemplate = media.value;
      params.rootfs = `${diskStorage.value}:${diskSize.value}`;
      const ip =
        lxcIpMode.value === "dhcp"
          ? "ip=dhcp"
          : `ip=${lxcIpAddr.value}${lxcGw.value ? `,gw=${lxcGw.value}` : ""}`;
      params.net0 = `name=eth0,bridge=${bridge.value}${tag},${ip}`;
      if (password.value) params.password = password.value;
      params.unprivileged = lxcUnpriv.value ? "1" : "0";
      if (lxcNesting.value) params.features = "nesting=1";
      if (lxcSshKey.value.trim()) params["ssh-public-keys"] = lxcSshKey.value.trim();
    }
    await api.createGuest(activeId.value, node.value, kind.value, params);
    toast(`Creation task started for ${vmid.value}`);
    await refreshCluster();
    router.push("/guests");
  } catch (e) {
    error.value = String(e);
  } finally {
    submitting.value = false;
  }
}

onMounted(async () => {
  if (nodes.value.length === 0) await refreshCluster();
  if (!node.value && nodes.value.length > 0) node.value = nodes.value[0].node ?? "";
  // Suggest the next free vmid.
  const used = guests.value.map((g) => g.vmid ?? 0);
  vmid.value = used.length > 0 ? Math.max(...used) + 1 : 100;
});

watch([node, kind], loadStorages);
</script>

<template>
  <div>
    <div class="head">
      <h1>Create {{ kind === "qemu" ? "VM" : "container" }}</h1>
      <router-link to="/guests">
        Back to list
      </router-link>
    </div>

    <p v-if="!activeId">
      No active connection. Add one under Connections.
    </p>

    <form
      v-else
      class="card"
      @submit.prevent="submit"
    >
      <label>
        Type
        <select v-model="kind">
          <option value="qemu">VM (qemu)</option>
          <option value="lxc">Container (lxc)</option>
        </select>
      </label>
      <label>
        Node
        <select v-model="node">
          <option
            v-for="n in nodes"
            :key="n.id"
            :value="n.node"
          >{{ n.node }}</option>
        </select>
      </label>
      <label>
        VMID
        <input
          v-model.number="vmid"
          type="number"
          min="100"
          required
        >
      </label>
      <label>
        {{ kind === "qemu" ? "Name" : "Hostname" }}
        <input v-model="name">
      </label>
      <label>
        {{ kind === "qemu" ? "Install ISO" : "Template" }}
        <select
          v-model="media"
          :required="kind === 'lxc'"
        >
          <option
            v-if="kind === 'qemu'"
            value=""
          >(none)</option>
          <option
            v-for="v in mediaVolumes"
            :key="v.volid"
            :value="v.volid"
          >{{ v.volid }}</option>
        </select>
      </label>
      <label v-if="kind === 'lxc'">
        Root password
        <input
          v-model="password"
          type="password"
          autocomplete="new-password"
        >
      </label>
      <label>
        Cores
        <input
          v-model.number="cores"
          type="number"
          min="1"
          required
        >
      </label>
      <label>
        Memory (MiB)
        <input
          v-model.number="memory"
          type="number"
          min="16"
          required
        >
      </label>
      <label>
        Disk
        <span class="row">
          <select
            v-model="diskStorage"
            required
          >
            <option
              v-for="s in diskStorages"
              :key="s.storage"
              :value="s.storage"
            >{{ s.storage }}</option>
          </select>
          <input
            v-model.number="diskSize"
            type="number"
            min="1"
            required
          >
          <span>GiB</span>
        </span>
      </label>
      <label>
        Network bridge
        <input
          v-model="bridge"
          required
        >
      </label>

      <details>
        <summary>Advanced</summary>
        <div class="section">
          <label class="inline">
            <input
              v-model="startAfter"
              type="checkbox"
            >
            Start after creation
          </label>
          <label>
            VLAN tag
            <input
              v-model.number="vlanTag"
              type="number"
              min="1"
              max="4094"
              placeholder="none"
            >
          </label>
          <template v-if="kind === 'qemu'">
            <label class="inline">
              <input
                v-model="qemuAgent"
                type="checkbox"
              >
              QEMU guest agent
            </label>
          </template>
          <template v-else>
            <label class="inline">
              <input
                v-model="lxcUnpriv"
                type="checkbox"
              >
              Unprivileged container
            </label>
            <label class="inline">
              <input
                v-model="lxcNesting"
                type="checkbox"
              >
              Nesting
            </label>
            <label>
              SSH public key
              <textarea
                v-model="lxcSshKey"
                rows="2"
                placeholder="ssh-ed25519 …"
              />
            </label>
            <label>
              IPv4
              <span class="row">
                <select v-model="lxcIpMode">
                  <option value="dhcp">DHCP</option>
                  <option value="static">Static</option>
                </select>
                <template v-if="lxcIpMode === 'static'">
                  <input
                    v-model="lxcIpAddr"
                    placeholder="10.0.0.50/24"
                  >
                  <input
                    v-model="lxcGw"
                    placeholder="gateway"
                  >
                </template>
              </span>
            </label>
          </template>
        </div>
      </details>

      <details v-if="kind === 'qemu'">
        <summary>Cloud-init</summary>
        <div class="section">
          <label class="inline">
            <input
              v-model="ciEnabled"
              type="checkbox"
            >
            Add cloud-init drive
          </label>
          <template v-if="ciEnabled">
            <label>
              User
              <input v-model="ciUser">
            </label>
            <label>
              Password
              <input
                v-model="ciPassword"
                type="password"
                autocomplete="new-password"
              >
            </label>
            <label>
              SSH public keys
              <textarea
                v-model="ciSshKeys"
                rows="2"
                placeholder="ssh-ed25519 …"
              />
            </label>
            <label>
              IPv4
              <span class="row">
                <select v-model="ciIpMode">
                  <option value="dhcp">DHCP</option>
                  <option value="static">Static</option>
                </select>
                <template v-if="ciIpMode === 'static'">
                  <input
                    v-model="ciIpAddr"
                    placeholder="10.0.0.51/24"
                  >
                  <input
                    v-model="ciGw"
                    placeholder="gateway"
                  >
                </template>
              </span>
            </label>
          </template>
        </div>
      </details>

      <p
        v-if="error"
        class="error"
      >
        {{ error }}
      </p>

      <button
        type="submit"
        :disabled="submitting"
      >
        {{ submitting ? "Creating…" : "Create" }}
      </button>
    </form>
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
  max-width: 420px;
  border: 1px solid #ccc3;
  border-radius: 8px;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
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
  width: 80px;
}

.section {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 10px 0 0 8px;
}

label.inline {
  flex-direction: row;
  align-items: center;
  gap: 6px;
}

.section .row input {
  width: 120px;
}

textarea {
  font-family: inherit;
  font-size: inherit;
}

.error {
  color: #c33;
}
</style>
