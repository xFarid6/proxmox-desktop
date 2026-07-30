import { describe, expect, it } from "vitest";
import type { NetworkInterface } from "./api";
import {
  blankForm,
  buildIfaceParams,
  canEdit,
  connectionIface,
  formFromIface,
  hostAddress,
  isIpv4,
  isIpv4Cidr,
  isIpv6Cidr,
  maskToPrefix,
  needsHashPolicy,
  normalizePorts,
  pendingRisk,
  slaveCandidates,
  validateIface,
} from "./network";

const iface = (over: Partial<NetworkInterface>): NetworkInterface => ({
  iface: "vmbr0",
  type: "bridge",
  ...over,
});

describe("isIpv4 / isIpv4Cidr", () => {
  it("accepts dotted quads and rejects out-of-range octets", () => {
    expect(isIpv4("192.168.1.10")).toBe(true);
    expect(isIpv4("0.0.0.0")).toBe(true);
    expect(isIpv4("192.168.1.256")).toBe(false);
    expect(isIpv4("192.168.1")).toBe(false);
  });

  it("rejects leading zeros, which Proxmox also rejects", () => {
    expect(isIpv4("192.168.01.10")).toBe(false);
  });

  it("requires exactly one prefix of 0–32", () => {
    expect(isIpv4Cidr("10.0.0.2/24")).toBe(true);
    expect(isIpv4Cidr("10.0.0.2/32")).toBe(true);
    expect(isIpv4Cidr("10.0.0.2/33")).toBe(false);
    expect(isIpv4Cidr("10.0.0.2")).toBe(false);
    expect(isIpv4Cidr("10.0.0.2/24/8")).toBe(false);
  });
});

describe("isIpv6Cidr", () => {
  it("wants a colon and a prefix of 0–128", () => {
    expect(isIpv6Cidr("fd00::2/64")).toBe(true);
    expect(isIpv6Cidr("fd00::2/129")).toBe(false);
    expect(isIpv6Cidr("10.0.0.2/24")).toBe(false);
  });
});

describe("maskToPrefix", () => {
  it("converts contiguous masks", () => {
    expect(maskToPrefix("255.255.255.0")).toBe("24");
    expect(maskToPrefix("255.255.0.0")).toBe("16");
    expect(maskToPrefix("0.0.0.0")).toBe("0");
  });

  it("refuses a non-contiguous mask rather than inventing a prefix", () => {
    expect(maskToPrefix("255.0.255.0")).toBe("");
    expect(maskToPrefix("nonsense")).toBe("");
  });
});

describe("normalizePorts", () => {
  it("collapses commas and stray whitespace to Proxmox's space-separated list", () => {
    expect(normalizePorts("eno1, eno2")).toBe("eno1 eno2");
    expect(normalizePorts("  eno1   eno2 ")).toBe("eno1 eno2");
    expect(normalizePorts("")).toBe("");
  });
});

describe("needsHashPolicy", () => {
  it("is true only for the hashing bond modes", () => {
    expect(needsHashPolicy("802.3ad")).toBe(true);
    expect(needsHashPolicy("balance-xor")).toBe(true);
    expect(needsHashPolicy("active-backup")).toBe(false);
  });
});

describe("validateIface", () => {
  it("passes a plain bridge", () => {
    const f = blankForm("bridge");
    f.iface = "vmbr1";
    f.bridgePorts = "eno1";
    expect(validateIface(f)).toBeNull();
  });

  it("enforces the vmbr/bond naming Proxmox requires", () => {
    const bridge = { ...blankForm("bridge"), iface: "br1" };
    expect(validateIface(bridge)).toMatch(/vmbr/);
    const bond = { ...blankForm("bond"), iface: "lag0", slaves: "eno1" };
    expect(validateIface(bond)).toMatch(/bond0/);
  });

  it("rejects a malformed CIDR and a bare gateway with no address", () => {
    const f = { ...blankForm("bridge"), iface: "vmbr1", cidr: "10.0.0.2" };
    expect(validateIface(f)).toMatch(/CIDR/);
    const g = { ...blankForm("bridge"), iface: "vmbr1", gateway: "10.0.0.1" };
    expect(validateIface(g)).toMatch(/needs an IPv4 address/);
  });

  it("bounds the MTU", () => {
    const f = { ...blankForm("bridge"), iface: "vmbr1" };
    expect(validateIface({ ...f, mtu: "9000" })).toBeNull();
    expect(validateIface({ ...f, mtu: "10" })).toMatch(/MTU/);
    expect(validateIface({ ...f, mtu: "99999" })).toMatch(/MTU/);
  });

  it("needs at least one slave on a bond", () => {
    const f = { ...blankForm("bond"), iface: "bond0" };
    expect(validateIface(f)).toMatch(/slave/);
    expect(validateIface({ ...f, slaves: "eno1 eno2" })).toBeNull();
  });

  it("bounds the VLAN tag and takes it from a dotted name", () => {
    expect(validateIface({ ...blankForm("vlan"), iface: "eno1.100" })).toBeNull();
    expect(validateIface({ ...blankForm("vlan"), iface: "eno1.4095" })).toMatch(/1–4094/);
    // The vlan<tag> spelling carries no device, so one has to be named.
    const bare = { ...blankForm("vlan"), iface: "vlan100", vlanTag: "100" };
    expect(validateIface(bare)).toMatch(/raw device/);
    expect(validateIface({ ...bare, vlanRawDevice: "eno1" })).toBeNull();
  });
});

describe("buildIfaceParams", () => {
  it("emits Proxmox keys for a bridge", () => {
    const f = blankForm("bridge");
    f.iface = "vmbr1";
    f.bridgePorts = "eno1, eno2";
    f.cidr = "10.0.0.2/24";
    f.gateway = "10.0.0.1";
    f.vlanAware = true;
    expect(buildIfaceParams(f)).toEqual({
      iface: "vmbr1",
      type: "bridge",
      autostart: "1",
      cidr: "10.0.0.2/24",
      gateway: "10.0.0.1",
      bridge_ports: "eno1 eno2",
      bridge_vlan_aware: "1",
    });
  });

  it("only sends a hash policy for the modes that take one", () => {
    const f = { ...blankForm("bond"), iface: "bond0", slaves: "eno1 eno2" };
    expect(buildIfaceParams({ ...f, bondMode: "802.3ad" }).bond_xmit_hash_policy).toBe("layer2+3");
    expect(
      buildIfaceParams({ ...f, bondMode: "active-backup" }).bond_xmit_hash_policy,
    ).toBeUndefined();
  });

  it("derives vlan-id and vlan-raw-device from a dotted name", () => {
    const p = buildIfaceParams({ ...blankForm("vlan"), iface: "eno1.100" });
    expect(p["vlan-raw-device"]).toBe("eno1");
    expect(p["vlan-id"]).toBe("100");
  });

  it("omits autostart when off, since an absent key is an unset key", () => {
    const f = { ...blankForm("bridge"), iface: "vmbr1", autostart: false };
    expect(buildIfaceParams(f).autostart).toBeUndefined();
  });
});

describe("formFromIface", () => {
  it("round-trips the fields an update would otherwise drop", () => {
    const f = formFromIface(
      iface({
        iface: "vmbr0",
        type: "bridge",
        cidr: "192.168.1.10/24",
        gateway: "192.168.1.1",
        cidr6: "fd00::10/64",
        bridge_ports: "eno1",
        mtu: 9000,
        comments: "uplink",
        autostart: 1,
      }),
    );
    const p = buildIfaceParams(f);
    expect(p.mtu).toBe("9000");
    expect(p.comments).toBe("uplink");
    expect(p.cidr6).toBe("fd00::10/64");
    expect(p.cidr).toBe("192.168.1.10/24");
    expect(p.autostart).toBe("1");
  });

  it("rebuilds a cidr from address + netmask on older Proxmox", () => {
    const f = formFromIface(
      iface({ address: "192.168.1.10", netmask: "255.255.255.0", cidr: undefined }),
    );
    expect(f.cidr).toBe("192.168.1.10/24");
  });

  it("carries bond settings over", () => {
    const f = formFromIface(
      iface({ iface: "bond0", type: "bond", slaves: "eno1 eno2", bond_mode: "802.3ad" }),
    );
    expect(f.slaves).toBe("eno1 eno2");
    expect(f.bondMode).toBe("802.3ad");
  });
});

describe("canEdit", () => {
  it("covers the modelled kinds and leaves the rest read-only", () => {
    expect(canEdit(iface({ type: "bridge" }))).toBe(true);
    expect(canEdit(iface({ type: "eth" }))).toBe(true);
    expect(canEdit(iface({ type: "OVSBridge" }))).toBe(false);
    expect(canEdit(iface({ type: "alias" }))).toBe(false);
  });
});

describe("slaveCandidates", () => {
  it("offers NICs and bonds, never bridges or the interface itself", () => {
    const list = [
      iface({ iface: "eno1", type: "eth" }),
      iface({ iface: "eno2", type: "eth" }),
      iface({ iface: "bond0", type: "bond" }),
      iface({ iface: "vmbr0", type: "bridge" }),
    ];
    expect(slaveCandidates(list, "bond0")).toEqual(["eno1", "eno2"]);
  });
});

describe("hostAddress", () => {
  it("strips scheme, port and path", () => {
    expect(hostAddress("https://100.80.231.52:8006")).toBe("100.80.231.52");
    expect(hostAddress("100.80.231.52:8006")).toBe("100.80.231.52");
    expect(hostAddress("pve.example.com")).toBe("pve.example.com");
  });

  it("unwraps a bracketed IPv6 literal and leaves a bare one alone", () => {
    expect(hostAddress("https://[fd00::1]:8006")).toBe("fd00::1");
    expect(hostAddress("fd00::1")).toBe("fd00::1");
  });
});

describe("connectionIface / pendingRisk", () => {
  const list = [
    iface({ iface: "vmbr0", cidr: "192.168.1.10/24", address: "192.168.1.10" }),
    iface({ iface: "vmbr1", cidr: "10.0.0.1/24", address: "10.0.0.1" }),
  ];

  it("finds the interface holding the address we are connected over", () => {
    expect(connectionIface(list, "https://192.168.1.10:8006")?.iface).toBe("vmbr0");
    expect(connectionIface(list, "https://100.80.231.52:8006")).toBeNull();
  });

  it("names the interface when the staged diff touches it", () => {
    const diff = [
      "--- /etc/network/interfaces",
      "+++ /etc/network/interfaces.new",
      "-iface vmbr0 inet static",
      "+iface vmbr0 inet dhcp",
    ].join("\n");
    expect(pendingRisk(diff, list, "192.168.1.10:8006")).toBe("vmbr0");
  });

  it("stays quiet when the diff only touches other interfaces", () => {
    const diff = ["--- /etc/network/interfaces", "+auto vmbr1", "+iface vmbr1 inet manual"].join(
      "\n",
    );
    expect(pendingRisk(diff, list, "192.168.1.10:8006")).toBeNull();
  });

  it("does not read the diff's own header lines as a change", () => {
    // A diff header names the file, not an interface — but if the connection
    // rode an interface literally called "interfaces" this would false-fire.
    expect(pendingRisk("--- /etc/network/interfaces\n+++ /etc/network/interfaces.new", list, "192.168.1.10")).toBeNull();
  });

  it("is null with nothing staged", () => {
    expect(pendingRisk(null, list, "192.168.1.10")).toBeNull();
  });
});
