<script setup lang="ts">
const nav = [
  { to: "/connections", label: "Connections" },
  { to: "/dashboard", label: "Dashboard" },
  { to: "/guests", label: "VMs & CTs" },
  { to: "/tasks", label: "Tasks" },
  { to: "/network", label: "Network" },
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
    <main class="content">
      <RouterView />
    </main>
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
}
</style>
