import { watch } from "vue";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { api } from "../api";
import { activeId } from "./connections";
import { nodes } from "./cluster";
import { toast } from "./toast";

// Task-failure alerts: poll recent tasks on every node, baseline on the
// first pass, then raise a toast + native notification for each task that
// finishes with a non-OK status afterwards.

const POLL_MS = 15000;
const seen = new Set<string>();
let baselined = false;
let timer: number | undefined;

async function notify(title: string, body: string) {
  try {
    let granted = await isPermissionGranted();
    if (!granted) granted = (await requestPermission()) === "granted";
    if (granted) sendNotification({ title, body });
  } catch {
    // Toast already shown; native notification is best-effort.
  }
}

async function poll() {
  if (!activeId.value || nodes.value.length === 0) return;
  const lists = await Promise.all(
    nodes.value.map((n) =>
      api.nodeTasks(activeId.value!, n.node ?? "").catch(() => []),
    ),
  );
  const finished = lists.flat().filter((t) => t.endtime);
  if (!baselined) {
    finished.forEach((t) => seen.add(t.upid));
    baselined = true;
    return;
  }
  for (const t of finished) {
    if (seen.has(t.upid)) continue;
    seen.add(t.upid);
    if (t.status && t.status !== "OK") {
      const label = `${t.type}${t.id ? ` ${t.id}` : ""} on ${t.node}`;
      toast(`Task failed: ${label} — ${t.status}`, "error");
      // System notification only when the app is backgrounded — the toast
      // already covers the visible case, and this avoids double alerts on
      // Android where the notification would land on top of the open app.
      if (document.hidden) void notify("Proxmox task failed", `${label}: ${t.status}`);
    }
  }
}

export function startTaskAlerts() {
  if (timer) return;
  // Ask now, while foregrounded — Android 13+ can't show the permission
  // prompt later from the background when the first failure arrives.
  void isPermissionGranted()
    .then((g) => (g ? "granted" : requestPermission()))
    .catch(() => {});
  timer = window.setInterval(() => void poll(), POLL_MS);
  watch(activeId, () => {
    seen.clear();
    baselined = false;
  });
}
