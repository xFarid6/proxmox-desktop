import { describe, expect, it } from "vitest";
import type { ClusterResource } from "./api";
import { hiddenStatsHint, statsHidden } from "./privileges";

/** Both rows below are the real `/cluster/resources` node entry from PVE
 * 9.2.4, captured twice against the same host: once with a full-privilege
 * token, once with a privilege-separated token holding no ACLs. */
const withPrivileges: ClusterResource = {
  id: "node/proxmox",
  type: "node",
  node: "proxmox",
  status: "online",
  cpu: 0.0116064238002748,
  maxcpu: 20,
  mem: 4985950208,
  maxmem: 16385695744,
  disk: 51703259136,
  maxdisk: 100861726720,
  uptime: 86896,
};

const withoutPrivileges: ClusterResource = {
  id: "node/proxmox",
  type: "node",
  node: "proxmox",
  status: "online",
};

describe("statsHidden", () => {
  it("is true for the online node a no-privilege token sees", () => {
    expect(statsHidden(withoutPrivileges)).toBe(true);
  });

  it("is false once the figures are actually there", () => {
    expect(statsHidden(withPrivileges)).toBe(false);
  });

  it("does not mistake a genuinely idle node for a hidden one", () => {
    expect(statsHidden({ ...withPrivileges, cpu: 0, mem: 0, disk: 0, uptime: 0 })).toBe(false);
  });

  it("stays quiet about a node that is simply offline", () => {
    expect(statsHidden({ ...withoutPrivileges, status: "offline" })).toBe(false);
  });
});

describe("hiddenStatsHint", () => {
  it("names the privilege and the node path", () => {
    const hint = hiddenStatsHint("proxmox");
    expect(hint).toContain("Sys.Audit");
    expect(hint).toContain("/nodes/proxmox");
  });

  it("degrades to the bare path without a node name", () => {
    expect(hiddenStatsHint()).toContain("/nodes");
  });
});
