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
  { path: "/tasks", component: () => import("./views/TasksView.vue") },
  { path: "/network", component: () => import("./views/NetworkView.vue") },
  { path: "/backups", component: () => import("./views/BackupsView.vue") },
  { path: "/firewall", component: () => import("./views/FirewallView.vue") },
  { path: "/storage", component: () => import("./views/StorageView.vue") },
];

export default createRouter({
  history: createWebHashHistory(),
  routes,
});
