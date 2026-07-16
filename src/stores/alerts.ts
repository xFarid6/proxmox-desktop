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
      void notify("Proxmox task failed", `${label}: ${t.status}`);
    }
  }
}

export function startTaskAlerts() {
  if (timer) return;
  timer = window.setInterval(() => void poll(), POLL_MS);
  watch(activeId, () => {
    seen.clear();
    baselined = false;
  });
}
