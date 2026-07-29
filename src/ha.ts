import type { GuestKind } from "./api";

/** HA addresses guests by service id, `qemu:100` / `lxc:101` — not by vmid. */
export function guestSid(kind: GuestKind, vmid: number): string {
  return `${kind}:${vmid}`;
}

/** Pull the vmid back out of a sid. Returns null for anything else (the
 * status list mixes service ids in with `quorum`, `master:pve1`, ...). */
export function sidVmid(sid: string): number | null {
  const m = /^(?:qemu|lxc):(\d+)$/.exec(sid);
  return m ? Number(m[1]) : null;
}

/** Group node lists are `node[:priority]`, comma-separated, no spaces.
 * People type them with spaces and trailing commas; Proxmox rejects both. */
export function normalizeHaNodes(input: string): string {
  return input
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)
    .join(",");
}

/** Display form of a node list: "pve1:2,pve2" -> "pve1 (prio 2), pve2". */
export function formatHaNodes(nodes?: string): string {
  if (!nodes) return "—";
  return normalizeHaNodes(nodes)
    .split(",")
    .map((n) => {
      const [name, prio] = n.split(":");
      return prio ? `${name} (prio ${prio})` : name;
    })
    .join(", ");
}
