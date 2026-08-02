<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { cephAvailable, probeCeph } from "./ceph";
import ToastList from "./components/ToastList.vue";
import { isSshHost, navFor, routeAllowedFor } from "./connectionkind";
import { startTaskAlerts } from "./stores/alerts";
import { nodes, refreshCluster } from "./stores/cluster";
import {
  activeConnection,
  activeId,
  connections,
  refreshConnections,
  setActive,
} from "./stores/connections";

startTaskAlerts();

// The connection list used to be loaded by whichever view needed it first.
// The aggregate views and the cluster switcher below both need it before any
// view mounts, so it is loaded here (#24).
void refreshConnections();

// Ceph is optional, so its nav entry only appears once a node answers
// /ceph/status. Probed per connection and cached in ceph.ts. An SSH host has
// no PVE API to probe (#102), so skip it rather than firing a pointless
// request that can only fail.
watch(
  [activeId, nodes],
  () => {
    if (isSshHost(activeConnection.value)) return;
    void probeCeph(activeId.value, nodes.value[0]?.node);
  },
  { immediate: true },
);

// Pull-to-refresh (#52): pulling down from the top re-keys the RouterView
// (remount refetches the view's data) and refreshes the cluster store.
const content = ref<HTMLElement | null>(null);
const viewKey = ref(0);
const pull = ref(0);
let startY = 0;
let pulling = false;

function onTouchStart(e: TouchEvent) {
  pulling = content.value?.scrollTop === 0;
  startY = e.touches[0].clientY;
  pull.value = 0;
}

function onTouchMove(e: TouchEvent) {
  if (!pulling) return;
  pull.value = Math.max(0, Math.min(100, e.touches[0].clientY - startY));
}

function onTouchEnd() {
  if (pull.value > 70) {
    viewKey.value++;
    void refreshCluster();
  }
  pull.value = 0;
  pulling = false;
}

const nav = computed(() =>
  navFor(activeConnection.value, { cephAvailable: cephAvailable.value }),
);

// Hiding the nav entry is not enough on its own (#102): the router stays on
// whatever route was open when the picker switches connections, so selecting
// an SSH host from /dashboard would leave a PVE-only view on screen with no
// PVE API behind it. Send those back to Connections.
const route = useRoute();
const router = useRouter();
watch(
  [activeConnection, () => route.path],
  ([conn, path]) => {
    if (!routeAllowedFor(conn, path)) void router.replace("/connections");
  },
  { immediate: true },
);
</script>

<template>
  <div class="layout">
    <nav class="sidebar">
      <div class="brand">
        Proxmox Desktop
      </div>
      <!-- Dashboard and the guest list span every cluster; the rest of the
           views are per-cluster, and this is what picks which one (#24). -->
      <select
        v-if="connections.length > 1"
        class="cluster-pick"
        :value="activeId ?? ''"
        @change="setActive(($event.target as HTMLSelectElement).value || null)"
      >
        <option
          v-for="c in connections"
          :key="c.id"
          :value="c.id"
        >
          {{ c.name }}{{ isSshHost(c) ? " (SSH)" : "" }}
        </option>
      </select>
      <RouterLink
        v-for="item in nav"
        :key="item.to"
        :to="item.to"
        class="nav-link"
      >
        {{ item.label }}
      </RouterLink>
    </nav>
    <main
      ref="content"
      class="content"
      @touchstart.passive="onTouchStart"
      @touchmove.passive="onTouchMove"
      @touchend.passive="onTouchEnd"
    >
      <div
        v-if="pull > 0"
        class="pull-hint"
        :style="{ height: `${pull / 2}px`, opacity: pull / 100 }"
      >
        {{ pull > 70 ? "release to refresh" : "↓ pull to refresh" }}
      </div>
      <RouterView :key="viewKey" />
    </main>
    <ToastList />
  </div>
</template>

<style scoped>
.layout {
  display: flex;
  height: 100vh;
}

.sidebar {
  width: 200px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 12px 8px;
  background: #24292e;
}

.brand {
  color: #e57000;
  font-weight: 700;
  padding: 8px 12px 16px;
}

.cluster-pick {
  margin: 0 4px 10px;
  padding: 4px 6px;
  font-size: 0.85em;
  background: #33393f;
  color: #cdd3d8;
  border: 1px solid #4a5158;
  border-radius: 6px;
}

.nav-link {
  color: #cdd3d8;
  padding: 8px 12px;
  border-radius: 6px;
}

.nav-link:hover {
  background: #33393f;
}

.nav-link.router-link-active {
  background: #e57000;
  color: #fff;
}

.content {
  flex: 1;
  overflow: auto;
  padding: 20px 24px;
  overscroll-behavior-y: contain;
}

.pull-hint {
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 0.85em;
  opacity: 0.7;
  overflow: hidden;
}

/* Mobile (#47): sidebar becomes bottom tab bar */
@media (max-width: 768px) {
  .layout {
    flex-direction: column-reverse;
  }

  .sidebar {
    width: 100%;
    flex-direction: row;
    overflow-x: auto;
    gap: 4px;
    padding: 4px 6px;
    padding-bottom: calc(4px + env(safe-area-inset-bottom));
  }

  .brand {
    display: none;
  }

  .nav-link {
    white-space: nowrap;
    padding: 12px 14px;
  }

  .content {
    padding: 14px 12px;
  }
}
</style>
