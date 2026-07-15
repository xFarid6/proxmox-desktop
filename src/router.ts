import { createRouter, createWebHashHistory } from "vue-router";

const routes = [
  { path: "/", redirect: "/connections" },
  { path: "/connections", component: () => import("./views/ConnectionsView.vue") },
  { path: "/dashboard", component: () => import("./views/DashboardView.vue") },
  { path: "/guests", component: () => import("./views/GuestsView.vue") },
  { path: "/guests/:node/:kind/:vmid", component: () => import("./views/GuestDetailView.vue") },
  { path: "/tasks", component: () => import("./views/TasksView.vue") },
  { path: "/network", component: () => import("./views/NetworkView.vue") },
];

export default createRouter({
  history: createWebHashHistory(),
  routes,
});
