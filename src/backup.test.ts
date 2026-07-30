import { describe, expect, it } from "vitest";
import type { ClusterResource, Permissions, StorageSummary } from "./api";
import { backupPreflight, hasPrivilege } from "./backup";

/** Real `/access/permissions` output from PVE 9.2.4 for a token with
 * Privilege Separation off, trimmed to the privileges these checks read. Note
 * what is absent: no /vms/100, no /storage/local. That is why the lookup walks
 * up the path. */
const fullPrivileges: Permissions = {
  "/": { "VM.Backup": 1, "Datastore.AllocateSpace": 1, "Sys.Audit": 1, "VM.Audit": 1 },
  "/nodes": { "VM.Backup": 1, "Datastore.AllocateSpace": 1, "Sys.Audit": 1, "VM.Audit": 1 },
  "/storage": { "VM.Backup": 1, "Datastore.AllocateSpace": 1, "Sys.Audit": 1, "VM.Audit": 1 },
  "/vms": { "VM.Backup": 1, "Datastore.AllocateSpace": 1, "Sys.Audit": 1, "VM.Audit": 1 },
};

/** What the same endpoint returned for a privilege-separated token with no
 * ACLs: an empty object, and HTTP 200 rather than a 403. */
const noPrivileges: Permissions = {};

/** `local` on the live node, and VM 100 (Kali-Ludo), both as reported. */
const local: StorageSummary = {
  storage: "local",
  content: "iso,backup,import,vztmpl",
  active: 1,
  avail: 43987755008,
  total: 100861726720,
};
const guest100: ClusterResource = {
  id: "qemu/100",
  type: "qemu",
  node: "proxmox",
  vmid: 100,
  name: "Kali-Ludo",
  status: "stopped",
  maxdisk: 34359738368,
};

describe("hasPrivilege", () => {
  it("finds a privilege granted on an ancestor of the asked-for path", () => {
    expect(hasPrivilege(fullPrivileges, "/vms/100", "VM.Backup")).toBe(true);
    expect(hasPrivilege(fullPrivileges, "/storage/local", "Datastore.AllocateSpace")).toBe(true);
  });

  it("finds one granted at the root", () => {
    expect(hasPrivilege({ "/": { "VM.Backup": 1 } }, "/vms/100", "VM.Backup")).toBe(true);
  });

  it("is false when nothing along the path grants it", () => {
    expect(hasPrivilege(noPrivileges, "/vms/100", "VM.Backup")).toBe(false);
    expect(hasPrivilege({ "/vms": { "VM.Audit": 1 } }, "/vms/100", "VM.Backup")).toBe(false);
  });

  it("does not let a sibling path grant it", () => {
    expect(hasPrivilege({ "/vms/101": { "VM.Backup": 1 } }, "/vms/100", "VM.Backup")).toBe(false);
  });
});

describe("backupPreflight", () => {
  it("clears a backup the live cluster would actually accept", () => {
    // 40.9 GiB free against 32 GiB of guest disk — the real numbers.
    expect(
      backupPreflight({
        permissions: fullPrivileges,
        vmid: 100,
        guest: guest100,
        storage: local,
      }),
    ).toEqual({ blockers: [], warnings: [] });
  });

  it("blocks both privileges for a token that holds none", () => {
    const { blockers } = backupPreflight({
      permissions: noPrivileges,
      vmid: 100,
      guest: guest100,
      storage: local,
    });
    expect(blockers).toHaveLength(2);
    expect(blockers[0]).toContain("VM.Backup");
    expect(blockers[1]).toContain("Datastore.AllocateSpace");
  });

  it("blocks an inactive storage", () => {
    const { blockers } = backupPreflight({
      permissions: fullPrivileges,
      vmid: 100,
      storage: { ...local, active: 0 },
    });
    expect(blockers).toEqual(["Storage local is not active on this node."]);
  });

  it("warns, but does not block, when the raw disks exceed the free space", () => {
    const { blockers, warnings } = backupPreflight({
      permissions: fullPrivileges,
      vmid: 100,
      guest: { ...guest100, maxdisk: 500 * 1024 ** 3 },
      storage: local,
    });
    expect(blockers).toEqual([]);
    expect(warnings[0]).toContain("would not fit");
  });

  it("says so when the privilege list could not be read, rather than guessing", () => {
    const { blockers, warnings } = backupPreflight({
      permissions: null,
      vmid: 100,
      guest: guest100,
      storage: local,
    });
    expect(blockers).toEqual([]);
    expect(warnings[0]).toContain("skipped");
  });
});
