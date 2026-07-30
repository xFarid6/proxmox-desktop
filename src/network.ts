import type { NetworkInterface } from "./api";

/** Interface types this app can round-trip safely. `eth` is editable (a
 * physical NIC can carry an address) but not creatable — the kernel decides
 * which NICs exist. Anything else PVE reports (OVS*, alias, unknown) is left
 * read-only rather than risking a lossy rewrite. */
export type IfaceKind = "bridge" | "bond" | "vlan" | "eth";

export const CREATABLE_KINDS: IfaceKind[] = ["bridge", "bond", "vlan"];

/** Linux bonding modes PVE accepts. The OVS-only modes (balance-slb,
 * lacp-balance-*) are omitted along with the rest of OVS. */
export const BOND_MODES = [
  "balance-rr",
  "active-backup",
  "balance-xor",
  "broadcast",
  "802.3ad",
  "balance-tlb",
  "balance-alb",
];

export const HASH_POLICIES = ["layer2", "layer2+3", "layer3+4"];

/** Only the hashing modes take a transmit hash policy; sending one with
 * active-backup is rejected. */
export function needsHashPolicy(mode: string): boolean {
  return mode === "802.3ad" || mode === "balance-xor";
}

/** Edit form state. Every field is a string because it is bound straight to
 * an input; conversion and validation happen in `validateIface`. */
export interface IfaceForm {
  kind: IfaceKind;
  iface: string;
  cidr: string;
  gateway: string;
  cidr6: string;
  gateway6: string;
  mtu: string;
  comments: string;
  autostart: boolean;
  bridgePorts: string;
  vlanAware: boolean;
  slaves: string;
  bondMode: string;
  hashPolicy: string;
  vlanRawDevice: string;
  vlanTag: string;
}

export function blankForm(kind: IfaceKind = "bridge"): IfaceForm {
  return {
    kind,
    iface: "",
    cidr: "",
    gateway: "",
    cidr6: "",
    gateway6: "",
    mtu: "",
    comments: "",
    autostart: true,
    bridgePorts: "",
    vlanAware: false,
    slaves: "",
    bondMode: "active-backup",
    hashPolicy: "layer2+3",
    vlanRawDevice: "",
    vlanTag: "",
  };
}

/** Load an existing interface into the form. Fields PVE reported but the
 * form does not model would be dropped on the next save, so `canEdit` gates
 * which interfaces get here at all. */
export function formFromIface(i: NetworkInterface): IfaceForm {
  const f = blankForm(i.type as IfaceKind);
  f.iface = i.iface;
  f.cidr = i.cidr ?? (i.address && i.netmask ? `${i.address}/${maskToPrefix(i.netmask)}` : "");
  f.gateway = i.gateway ?? "";
  f.cidr6 = i.cidr6 ?? "";
  f.gateway6 = i.gateway6 ?? "";
  f.mtu = i.mtu != null ? String(i.mtu) : "";
  f.comments = i.comments ?? "";
  f.autostart = i.autostart === 1;
  f.bridgePorts = i.bridge_ports ?? "";
  f.vlanAware = i.bridge_vlan_aware === 1;
  f.slaves = i.slaves ?? "";
  if (i.bond_mode) f.bondMode = i.bond_mode;
  if (i.bond_xmit_hash_policy) f.hashPolicy = i.bond_xmit_hash_policy;
  f.vlanRawDevice = i["vlan-raw-device"] ?? "";
  f.vlanTag = i["vlan-id"] != null ? String(i["vlan-id"]) : "";
  return f;
}

/** Interfaces whose whole definition the form can represent. Editing anything
 * else would silently drop the keys we cannot see. */
export function canEdit(i: NetworkInterface): boolean {
  return (["bridge", "bond", "vlan", "eth"] as string[]).includes(i.type);
}

/** Candidates for bridge ports and bond slaves: real NICs and bonds, never
 * bridges (a bridge inside a bridge is not a thing) and never the interface
 * being edited. */
export function slaveCandidates(ifaces: NetworkInterface[], self: string): string[] {
  return ifaces
    .filter((i) => (i.type === "eth" || i.type === "bond") && i.iface !== self)
    .map((i) => i.iface)
    .sort((a, b) => a.localeCompare(b));
}

/** PVE wants a space-separated port list; people type commas. */
export function normalizePorts(input: string): string {
  return input
    .split(/[\s,]+/)
    .filter(Boolean)
    .join(" ");
}

/** Strict dotted-quad. Leading zeros are rejected because PVE rejects them. */
export function isIpv4(s: string): boolean {
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(s.trim());
  return !!m && m.slice(1).every((o) => Number(o) <= 255 && String(Number(o)) === o);
}

export function isIpv4Cidr(s: string): boolean {
  const parts = s.trim().split("/");
  if (parts.length !== 2) return false;
  const [ip, prefix] = parts;
  return isIpv4(ip) && /^\d{1,2}$/.test(prefix) && Number(prefix) <= 32;
}

/** ponytail: shape check only — a colon and a legal prefix. Full IPv6 literal
 * validation lives in PVE, which rejects the rest with a clear message. */
export function isIpv6Cidr(s: string): boolean {
  const parts = s.trim().split("/");
  if (parts.length !== 2 || !parts[0].includes(":")) return false;
  return /^\d{1,3}$/.test(parts[1]) && Number(parts[1]) <= 128;
}

/** Netmask to prefix length, for older PVE versions that report
 * address+netmask without a cidr. Returns "" if the mask is not contiguous. */
export function maskToPrefix(netmask: string): string {
  if (!isIpv4(netmask)) return "";
  const bits = netmask
    .split(".")
    .map((o) => Number(o).toString(2).padStart(8, "0"))
    .join("");
  return /^1*0*$/.test(bits) ? String(bits.replace(/0+$/, "").length) : "";
}

/** First problem with the form, or null when it is good to submit. */
export function validateIface(f: IfaceForm): string | null {
  const name = f.iface.trim();
  if (!name) return "Name is required.";
  if (f.kind === "bridge" && !/^vmbr\d{1,4}$/.test(name)) {
    return "Bridge names must be vmbr0–vmbr9999.";
  }
  if (f.kind === "bond" && !/^bond\d{1,4}$/.test(name)) {
    return "Bond names must be bond0–bond9999.";
  }
  if (f.kind === "vlan" && !/^(?:\S+\.\d+|vlan\d+)$/.test(name)) {
    return "VLAN names must be <device>.<tag> (eno1.100) or vlan<tag> (vlan100).";
  }
  if (f.cidr.trim() && !isIpv4Cidr(f.cidr)) return "IPv4 address must be CIDR, e.g. 10.0.0.2/24.";
  if (f.gateway.trim() && !isIpv4(f.gateway)) return "IPv4 gateway must be a plain address.";
  if (f.gateway.trim() && !f.cidr.trim()) return "An IPv4 gateway needs an IPv4 address.";
  if (f.cidr6.trim() && !isIpv6Cidr(f.cidr6)) return "IPv6 address must be CIDR, e.g. fd00::2/64.";
  if (f.gateway6.trim() && !f.gateway6.includes(":")) return "IPv6 gateway must be an IPv6 address.";
  if (f.mtu.trim()) {
    const mtu = Number(f.mtu);
    if (!Number.isInteger(mtu) || mtu < 576 || mtu > 65520) return "MTU must be 576–65520.";
  }
  if (f.kind === "bond") {
    if (!normalizePorts(f.slaves)) return "A bond needs at least one slave interface.";
    if (!BOND_MODES.includes(f.bondMode)) return "Pick a bond mode.";
  }
  if (f.kind === "vlan") {
    // `<device>.<tag>` already encodes both, so the explicit fields are only
    // required for the `vlan<tag>` spelling.
    const dotted = /^(\S+)\.(\d+)$/.exec(name);
    const tag = dotted ? dotted[2] : f.vlanTag.trim();
    if (!tag) return "VLAN tag is required.";
    if (!/^\d{1,4}$/.test(tag) || Number(tag) < 1 || Number(tag) > 4094) {
      return "VLAN tag must be 1–4094.";
    }
    if (!dotted && !f.vlanRawDevice.trim()) return "vlan<tag> names need a raw device.";
  }
  return null;
}

/** Proxmox form params for create or update. `update` replaces the whole
 * definition, so this always emits every field the form holds — an omitted
 * key is a deleted key on PVE's side. */
export function buildIfaceParams(f: IfaceForm): Record<string, string> {
  const p: Record<string, string> = { iface: f.iface.trim(), type: f.kind };
  if (f.autostart) p.autostart = "1";
  if (f.cidr.trim()) p.cidr = f.cidr.trim();
  if (f.gateway.trim()) p.gateway = f.gateway.trim();
  if (f.cidr6.trim()) p.cidr6 = f.cidr6.trim();
  if (f.gateway6.trim()) p.gateway6 = f.gateway6.trim();
  if (f.mtu.trim()) p.mtu = f.mtu.trim();
  if (f.comments.trim()) p.comments = f.comments.trim();
  if (f.kind === "bridge") {
    if (normalizePorts(f.bridgePorts)) p.bridge_ports = normalizePorts(f.bridgePorts);
    if (f.vlanAware) p.bridge_vlan_aware = "1";
  }
  if (f.kind === "bond") {
    p.slaves = normalizePorts(f.slaves);
    p.bond_mode = f.bondMode;
    if (needsHashPolicy(f.bondMode)) p.bond_xmit_hash_policy = f.hashPolicy;
  }
  if (f.kind === "vlan") {
    const dotted = /^(\S+)\.(\d+)$/.exec(f.iface.trim());
    if (dotted) {
      p["vlan-raw-device"] = dotted[1];
      p["vlan-id"] = dotted[2];
    } else {
      p["vlan-raw-device"] = f.vlanRawDevice.trim();
      p["vlan-id"] = f.vlanTag.trim();
    }
  }
  return p;
}

/** The bare address out of a connection host: drops any scheme, path and
 * port, and unwraps a bracketed IPv6 literal. */
export function hostAddress(host: string): string {
  const h = host
    .trim()
    .replace(/^\w+:\/\//, "")
    .replace(/\/.*$/, "");
  if (h.startsWith("[")) {
    const end = h.indexOf("]");
    return end === -1 ? h.slice(1) : h.slice(1, end);
  }
  const parts = h.split(":");
  // More than one colon means a bare IPv6 literal, which has no port to strip.
  return parts.length === 2 ? parts[0] : h;
}

/** The interface holding the address this app is talking to PVE over, if it
 * is one of the node's own. Null for a hostname, a NAT'd address, or a
 * tailnet address on an interface PVE does not manage. */
export function connectionIface(
  ifaces: NetworkInterface[],
  host: string,
): NetworkInterface | null {
  const ip = hostAddress(host);
  if (!ip) return null;
  return (
    ifaces.find((i) => i.address === ip || (i.cidr ?? "").split("/")[0] === ip) ?? null
  );
}

/** Name of the interface carrying the current connection when the staged diff
 * touches it — the case where applying can strand the app mid-request.
 *
 * ponytail: substring match over the diff's changed lines, not a parse of it.
 * A false positive costs one extra confirmation; a miss costs the link, so it
 * errs loud. Apply always warns anyway — this only names the interface. */
export function pendingRisk(
  changes: string | null | undefined,
  ifaces: NetworkInterface[],
  host: string,
): string | null {
  if (!changes) return null;
  const iface = connectionIface(ifaces, host);
  if (!iface) return null;
  const touched = changes
    .split("\n")
    .filter((l) => /^[+-]/.test(l) && !/^(\+\+\+|---)/.test(l))
    .some((l) => l.includes(iface.iface));
  return touched ? iface.iface : null;
}
