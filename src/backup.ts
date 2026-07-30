import type { ClusterResource, Permissions, StorageSummary } from "./api";
import { formatBytes } from "./format";

/** Whether the connection's token holds `privilege` at `path`.
 *
 * `/access/permissions` lists only paths an ACL actually names. Verified live
 * against PVE 9.2.4: a full-privilege token reported exactly `/`, `/access`,
 * `/access/groups`, `/nodes`, `/pool`, `/sdn`, `/storage` and `/vms` — nothing
 * per-guest and nothing per-storage. So a question about `/vms/100` is only
 * answerable by walking up to `/vms` and then to `/`, and a lookup of the
 * exact path alone would report every privilege as missing.
 */
export function hasPrivilege(perms: Permissions, path: string, privilege: string): boolean {
  const parts = path.split("/").filter(Boolean);
  for (let i = parts.length; i >= 0; i--) {
    if (perms[`/${parts.slice(0, i).join("/")}`]?.[privilege]) return true;
  }
  return false;
}

/** `blockers` are things Proxmox will refuse outright, so the button is
 * disabled; `warnings` are things worth knowing that may still succeed. */
export interface BackupPreflight {
  blockers: string[];
  warnings: string[];
}

/**
 * What would stop this backup, checked before the user commits to it (#89).
 *
 * A vzdump failure only surfaces in the task log minutes later, and the two
 * commonest causes are knowable up front: the token lacking VM.Backup or
 * Datastore.AllocateSpace, and the target storage not having room. Space is a
 * warning rather than a blocker because compression normally brings an archive
 * far below the guest's raw disk size — the check can only say the pessimistic
 * case would not fit.
 *
 * `permissions: null` means the privilege list could not be read, which is
 * reported rather than assumed either way.
 */
export function backupPreflight(opts: {
  permissions: Permissions | null;
  vmid?: number;
  guest?: ClusterResource;
  storage?: StorageSummary;
}): BackupPreflight {
  const { permissions, vmid, guest, storage } = opts;
  const blockers: string[] = [];
  const warnings: string[] = [];

  if (!permissions) {
    warnings.push(
      "Could not read this token's privileges, so the privilege checks below were skipped.",
    );
  } else {
    if (vmid != null && !hasPrivilege(permissions, `/vms/${vmid}`, "VM.Backup")) {
      blockers.push(
        `The API token has no VM.Backup privilege on /vms/${vmid}, so Proxmox will refuse the backup.`,
      );
    }
    if (storage && !hasPrivilege(permissions, `/storage/${storage.storage}`, "Datastore.AllocateSpace")) {
      blockers.push(
        `The API token has no Datastore.AllocateSpace privilege on /storage/${storage.storage}, ` +
          "so it may not write the archive there.",
      );
    }
  }

  if (storage && storage.active === 0) {
    blockers.push(`Storage ${storage.storage} is not active on this node.`);
  }

  if (storage?.avail != null && guest?.maxdisk != null && storage.avail < guest.maxdisk) {
    warnings.push(
      `${storage.storage} has ${formatBytes(storage.avail)} free and this guest's disks total ` +
        `${formatBytes(guest.maxdisk)}. Compression usually brings the archive well below that, ` +
        "but an uncompressed copy would not fit.",
    );
  }

  return { blockers, warnings };
}
