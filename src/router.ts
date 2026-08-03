import { createRouter, createWebHashHistory } from "vue-router";

const routes = [
  { path: "/", redirect: "/connections" },
  { path: "/connections", component: () => import("./views/ConnectionsView.vue") },
  { path: "/dashboard", component: () => import("./views/DashboardView.vue") },
  { path: "/guests", component: () => import("./views/GuestsView.vue") },
  { path: "/guests/new", component: () => import("./views/CreateGuestView.vue") },
  { path: "/guests/:node/:kind/:vmid", component: () => import("./views/GuestDetailView.vue") },
  {
    path: "/guests/:node/:kind/:vmid/console",
    component: () => import("./views/ConsoleView.vue"),
  },
  { path: "/guests/:node/:kind/:vmid/llm", component: () => import("./views/LlmChatView.vue") },
  { path: "/nodes/:node/ssh", component: () => import("./views/SshShellView.vue") },
  // An SSH host's terminal is the same view: open_ssh_shell only ever took a
  // connection id, so the node in the PVE route was always cosmetic (#103).
  { path: "/host/terminal", component: () => import("./views/SshShellView.vue") },
  { path: "/host/services", component: () => import("./views/HostServicesView.vue") },
  { path: "/host/docker", component: () => import("./views/HostDockerView.vue") },
  { path: "/host/stream", component: () => import("./views/HostStreamView.vue") },
  { path: "/tasks", component: () => import("./views/TasksView.vue") },
  { path: "/network", component: () => import("./views/NetworkView.vue") },
  { path: "/backups", component: () => import("./views/BackupsView.vue") },
  { path: "/firewall", component: () => import("./views/FirewallView.vue") },
  { path: "/storage", component: () => import("./views/StorageView.vue") },
  { path: "/ceph", component: () => import("./views/CephView.vue") },
  { path: "/certificates", component: () => import("./views/CertificatesView.vue") },
  { path: "/access", component: () => import("./views/AccessView.vue") },
  { path: "/ha", component: () => import("./views/HaView.vue") },
];

export default createRouter({
  history: createWebHashHistory(),
  routes,
});
