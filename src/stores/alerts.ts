import { watch } from "vue";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { api } from "../api";
import { connections } from "./connections";
import { allNodes, refreshAllClusters } from "./cluster";
import { toast } from "./toast";

// Task-failure alerts: poll recent tasks on every node of every saved
// connection, baseline on the first pass, then raise a toast + native
// notification for each task that finishes with a non-OK status afterwards.
// Alerts name the cluster, since two clusters can hold nodes and guests with
// identical names (#24).

const POLL_MS = 15000;
const seen = new Set<string>();
let baselined = false;
let timer: number | undefined;

/** UPIDs are unique within a cluster, not across them — key on both. */
function taskKey(connectionId: string, upid: string): string {
  return `${connectionId} ${upid}`;
}

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
  const targets = allNodes.value;
  if (targets.length === 0) {
    // No cluster has been fetched yet, so there is nothing to poll. The
    // aggregate views normally do this on mount; doing it here too means
    // alerts work even if the user never opens them.
    // ponytail: refetches node membership only while it is unknown, not on a
    // schedule. A node added to a cluster mid-session is picked up the next
    // time any view refreshes.
    if (connections.value.length > 0) await refreshAllClusters();
    return;
  }
  const lists = await Promise.all(
    targets.map((n) =>
      api
        .nodeTasks(n.connectionId, n.node ?? "")
        .then((tasks) =>
          tasks.map((t) => ({ ...t, connectionId: n.connectionId, cluster: n.clusterName })),
        )
        .catch(() => []),
    ),
  );
  const finished = lists.flat().filter((t) => t.endtime);
  if (!baselined) {
    finished.forEach((t) => seen.add(taskKey(t.connectionId, t.upid)));
    baselined = true;
    return;
  }
  for (const t of finished) {
    const key = taskKey(t.connectionId, t.upid);
    if (seen.has(key)) continue;
    seen.add(key);
    if (t.status && t.status !== "OK") {
      const label = `${t.type}${t.id ? ` ${t.id}` : ""} on ${t.cluster}/${t.node}`;
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
  // Re-baseline when the set of connections changes: a cluster that was just
  // added would otherwise report its whole task history as fresh failures.
  watch(connections, () => {
    seen.clear();
    baselined = false;
    void refreshAllClusters();
  });
}
