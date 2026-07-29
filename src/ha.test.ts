import { describe, expect, it } from "vitest";
import { formatHaNodes, guestSid, normalizeHaNodes, sidVmid } from "./ha";

describe("guestSid", () => {
  it("builds the Proxmox service id for both guest kinds", () => {
    expect(guestSid("qemu", 100)).toBe("qemu:100");
    expect(guestSid("lxc", 101)).toBe("lxc:101");
  });
});

describe("sidVmid", () => {
  it("extracts the vmid from a guest sid", () => {
    expect(sidVmid("qemu:100")).toBe(100);
    expect(sidVmid("lxc:101")).toBe(101);
  });

  it("returns null for non-guest status ids", () => {
    expect(sidVmid("quorum")).toBeNull();
    expect(sidVmid("master:pve1")).toBeNull();
    expect(sidVmid("qemu:abc")).toBeNull();
  });
});

describe("normalizeHaNodes", () => {
  it("strips spaces and empty entries", () => {
    expect(normalizeHaNodes(" pve1:2 , pve2 ,, pve3 , ")).toBe("pve1:2,pve2,pve3");
  });

  it("is empty for an empty input", () => {
    expect(normalizeHaNodes("  ")).toBe("");
  });
});

describe("formatHaNodes", () => {
  it("returns em dash for a missing list", () => {
    expect(formatHaNodes(undefined)).toBe("—");
  });

  it("spells out priorities and leaves bare nodes alone", () => {
    expect(formatHaNodes("pve1:2,pve2")).toBe("pve1 (prio 2), pve2");
  });
});
