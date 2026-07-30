import type { ClusterResource } from "./api";

/** Why a page is blank when the connection plainly works.
 *
 * Proxmox does not refuse a listing the token lacks privileges for — it
 * answers 200 and silently drops whatever the token may not see. Verified
 * live against PVE 9.2.4 with a privilege-separated token holding no ACLs:
 *
 *   /cluster/resources  -> the node row, `status: "online"`, with cpu, mem,
 *                          maxmem, disk, maxdisk and uptime all null, and no
 *                          guest and no storage rows at all
 *   /nodes/proxmox/tasks    -> []
 *   /nodes/proxmox/storage  -> []
 *
 * The same calls with a full-privilege token return cpu 0.0116, maxmem
 * 16385695744, maxdisk 100861726720, 18 guests. So a permissions gap and an
 * idle cluster are byte-identical apart from the nulls, which is exactly what
 * #87 reports. Nothing here can detect a 403, because no 403 is ever sent.
 */

/** An online node whose usage figures are all missing. `cpu` is 0 on a truly
 * idle node, never null, so `== null` is the test — not falsiness. */
export function statsHidden(node: ClusterResource): boolean {
  return node.status === "online" && node.cpu == null && node.maxmem == null;
}

const GRANT =
  "Grant it under Datacenter → Permissions → Add → API Token Permission, " +
  'or clear the token\'s "Privilege Separation" so it inherits its user\'s rights.';

export function hiddenStatsHint(node?: string): string {
  const path = node ? `/nodes/${node}` : "/nodes";
  return (
    `Proxmox returned no usage figures for this node. It hides what a token may ` +
    `not read instead of refusing the call, so the API token is almost certainly ` +
    `missing Sys.Audit on ${path}. ${GRANT}`
  );
}

/** Shown when a cluster that answered successfully lists no guests at all.
 * Legitimate on a fresh install, so the wording offers both readings. */
export const hiddenGuestsHint =
  "No guests came back. If this cluster does have VMs or containers, the API " +
  `token is missing VM.Audit on /vms — Proxmox omits guests a token may not see. ${GRANT}`;

export const hiddenTasksHint =
  "No tasks came back. If this node has run tasks, the API token is missing " +
  `Sys.Audit on this node — Proxmox omits the log rather than refusing it. ${GRANT}`;
