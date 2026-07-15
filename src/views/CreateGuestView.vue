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
    if (kind.value === "qemu") {
      if (name.value) params.name = name.value;
      params.scsihw = "virtio-scsi-pci";
      params.scsi0 = `${diskStorage.value}:${diskSize.value}`;
      params.net0 = `virtio,bridge=${bridge.value}`;
      params.ostype = "l26";
      if (media.value) params.ide2 = `${media.value},media=cdrom`;
    } else {
      if (name.value) params.hostname = name.value;
      params.ostemplate = media.value;
      params.rootfs = `${diskStorage.value}:${diskSize.value}`;
      params.net0 = `name=eth0,bridge=${bridge.value},ip=dhcp`;
      if (password.value) params.password = password.value;
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

.error {
  color: #c33;
}
</style>
