<script setup lang="ts">
import { ref } from "vue";
import ToastList from "./components/ToastList.vue";
import { startTaskAlerts } from "./stores/alerts";
import { refreshCluster } from "./stores/cluster";

startTaskAlerts();

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

const nav = [
  { to: "/connections", label: "Connections" },
  { to: "/dashboard", label: "Dashboard" },
  { to: "/guests", label: "VMs & CTs" },
  { to: "/tasks", label: "Tasks" },
  { to: "/network", label: "Network" },
  { to: "/backups", label: "Backups" },
  { to: "/firewall", label: "Firewall" },
  { to: "/storage", label: "Storage" },
  { to: "/access", label: "Access" },
];
</script>

<template>
  <div class="layout">
    <nav class="sidebar">
      <div class="brand">
        Proxmox Desktop
      </div>
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
