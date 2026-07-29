import { ref } from "vue";
import { api, type CephOsdTree, type CephPool, type CephStatus, type CrushNode } from "./api";
import { percent } from "./format";

/** One OSD, lifted out of the CRUSH tree into something a table can render.
 * `up` and `in` are Ceph's two independent axes: a daemon can be running but
 * marked out (draining), or stopped but still marked in. */
export interface OsdRow {
  id: number;
  name: string;
  host: string;
  deviceClass: string;
  up: boolean;
  in: boolean;
  percentUsed?: number;
  used?: number;
  total?: number;
}

/** Bucket nesting between root and host is not fixed — rack and datacenter
 * buckets are legal — so carry the last host seen down the walk instead of
 * assuming root -> host -> osd. */
function walk(node: CrushNode | undefined, host: string, out: OsdRow[]) {
  if (!node) return;
  if (node.type === "osd") {
    const id = node.id ?? -1;
    out.push({
      id,
      name: node.name ?? `osd.${id}`,
      host,
      deviceClass: node.device_class ?? "—",
      up: node.status === "up",
      in: node.in === 1,
      percentUsed: node.percent_used,
      used: node.bytes_used,
      total: node.total_space,
    });
    return;
  }
  const next = node.type === "host" ? (node.name ?? host) : host;
  for (const child of node.children ?? []) walk(child, next, out);
}

export function flattenOsdTree(tree?: CephOsdTree | null): OsdRow[] {
  const out: OsdRow[] = [];
  walk(tree?.root, "—", out);
  return out.sort((a, b) => a.id - b.id);
}

/** OSD usage as a whole percent. Derived from the byte counts when they are
 * there: the tree's own `percent_used` has been a 0-1 fraction in some PVE
 * versions and a 0-100 value in others, while the byte counts are stable. */
export function osdPercent(osd: OsdRow): number {
  if (osd.used != null && osd.total) return percent(osd.used, osd.total);
  return Math.round(osd.percentUsed ?? 0);
}

/** Pool usage as a whole percent. Unlike the OSD tree, the pool list's
 * `percent_used` comes straight from `ceph df` and is a 0-1 fraction. */
export function poolPercent(pool: CephPool): number {
  return Math.round((pool.percent_used ?? 0) * 100);
}

/** PG states, busiest first. Ceph reports them as "active+clean" style
 * compound names with a count each; there is no fixed set to enumerate. */
export function pgStates(status?: CephStatus | null): { state: string; count: number }[] {
  return [...(status?.pgmap?.pgs_by_state ?? [])]
    .map((p) => ({ state: p.state_name, count: p.count }))
    .sort((a, b) => b.count - a.count);
}

/** Ceph's health strings map onto the three colours the app already uses.
 * Anything unrecognised counts as a warning rather than as healthy. */
export function healthClass(health?: string): "ok" | "warn" | "bad" {
  if (health === "HEALTH_OK") return "ok";
  if (health === "HEALTH_ERR") return "bad";
  return "warn";
}

/** Whether the active connection's cluster runs Ceph. `ceph_status` fails on
 * a node without it (500 "rados_connect failed", 501 on older PVE), so any
 * error is read as "no Ceph". Cached per connection — the answer only changes
 * when Ceph is installed or removed, so the probe runs once and the Refresh
 * button forces it again. */
const probed = new Map<string, boolean>();
export const cephAvailable = ref(false);

export async function probeCeph(connectionId: string | null, node?: string, force = false) {
  if (!connectionId || !node) {
    cephAvailable.value = false;
    return;
  }
  if (force) probed.delete(connectionId);
  if (!probed.has(connectionId)) {
    try {
      await api.cephStatus(connectionId, node);
      probed.set(connectionId, true);
    } catch {
      probed.set(connectionId, false);
    }
  }
  cephAvailable.value = probed.get(connectionId) ?? false;
}
