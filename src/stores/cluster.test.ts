import { nextTick } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ClusterResource } from "../api";

// connections.ts reads localStorage at module load and vitest runs in node,
// so the global has to exist before the store is imported.
const backing = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (k: string) => backing.get(k) ?? null,
  setItem: (k: string, v: string) => backing.set(k, v),
  removeItem: (k: string) => backing.delete(k),
} as unknown as Storage);

const { activeId, connections } = await import("./connections");
const {
  allGuests,
  allNodes,
  clusterList,
  clusters,
  guests,
  multiCluster,
  nodes,
  tagOwner,
} = await import("./cluster");

const res = (over: Partial<ClusterResource>): ClusterResource => ({
  id: "qemu/100",
  type: "qemu",
  ...over,
});

function seed(id: string, name: string, resources: ClusterResource[]) {
  clusters.value[id] = { id, name, resources, loading: false, error: "" };
}

beforeEach(() => {
  clusters.value = {};
  connections.value = [];
  activeId.value = null;
});

describe("tagOwner", () => {
  it("stamps the owning connection on the requested kinds only", () => {
    const state = {
      id: "a",
      name: "home",
      loading: false,
      error: "",
      resources: [
        res({ id: "node/pve1", type: "node", node: "pve1" }),
        res({ id: "qemu/100", type: "qemu", vmid: 100 }),
        res({ id: "storage/local", type: "storage" }),
      ],
    };
    const guests = tagOwner(state, ["qemu", "lxc"]);
    expect(guests).toHaveLength(1);
    expect(guests[0].connectionId).toBe("a");
    expect(guests[0].clusterName).toBe("home");
  });
});

describe("clusterList", () => {
  it("lists every saved connection, in connections order", () => {
    connections.value = [
      { id: "a", name: "home", host: "h", kind: "pve", acceptInvalidCerts: false },
      { id: "b", name: "work", host: "h", kind: "pve", acceptInvalidCerts: false },
    ];
    expect(clusterList.value.map((c) => c.name)).toEqual(["home", "work"]);
  });

  it("stands in an empty state for a connection nothing has been fetched for", () => {
    connections.value = [{ id: "a", name: "home", host: "h", kind: "pve", acceptInvalidCerts: false }];
    expect(clusterList.value[0]).toMatchObject({ id: "a", resources: [], error: "" });
  });

  it("excludes SSH hosts — they have no PVE API to fetch cluster state from (#102)", () => {
    connections.value = [
      { id: "a", name: "home", host: "h", kind: "pve", acceptInvalidCerts: false },
      { id: "b", name: "wyse-server", host: "h", kind: "ssh", acceptInvalidCerts: false },
    ];
    expect(clusterList.value.map((c) => c.id)).toEqual(["a"]);
  });
});

describe("allGuests / allNodes", () => {
  beforeEach(() => {
    connections.value = [
      { id: "a", name: "home", host: "h", kind: "pve", acceptInvalidCerts: false },
      { id: "b", name: "work", host: "h", kind: "pve", acceptInvalidCerts: false },
    ];
    seed("a", "home", [
      res({ id: "node/pve1", type: "node", node: "pve1" }),
      res({ id: "qemu/100", type: "qemu", vmid: 100, node: "pve1" }),
    ]);
    seed("b", "work", [
      res({ id: "node/pve1", type: "node", node: "pve1" }),
      res({ id: "qemu/100", type: "qemu", vmid: 100, node: "pve1" }),
      res({ id: "lxc/101", type: "lxc", vmid: 101, node: "pve1" }),
    ]);
  });

  it("merges guests across clusters", () => {
    expect(allGuests.value).toHaveLength(3);
    expect(allNodes.value.map((n) => n.clusterName)).toEqual(["home", "work"]);
  });

  it("keeps a colliding vmid distinguishable by its owner", () => {
    const hundreds = allGuests.value.filter((g) => g.vmid === 100);
    expect(hundreds).toHaveLength(2);
    expect(hundreds.map((g) => g.connectionId)).toEqual(["a", "b"]);
  });

  it("leaves the active-connection accessors seeing one cluster only", () => {
    activeId.value = "b";
    expect(guests.value).toHaveLength(2);
    expect(nodes.value).toHaveLength(1);
  });

  it("reports no cluster when nothing is active", () => {
    expect(guests.value).toEqual([]);
    expect(nodes.value).toEqual([]);
  });
});

describe("multiCluster", () => {
  it("is false for zero or one connection", () => {
    expect(multiCluster.value).toBe(false);
    connections.value = [{ id: "a", name: "home", host: "h", kind: "pve", acceptInvalidCerts: false }];
    expect(multiCluster.value).toBe(false);
  });

  it("is true from two connections up", () => {
    connections.value = [
      { id: "a", name: "home", host: "h", kind: "pve", acceptInvalidCerts: false },
      { id: "b", name: "work", host: "h", kind: "pve", acceptInvalidCerts: false },
    ];
    expect(multiCluster.value).toBe(true);
  });

  it("is false for one PVE cluster plus one SSH host — that isn't a multi-cluster setup (#102)", () => {
    connections.value = [
      { id: "a", name: "home", host: "h", kind: "pve", acceptInvalidCerts: false },
      { id: "b", name: "wyse-server", host: "h", kind: "ssh", acceptInvalidCerts: false },
    ];
    expect(multiCluster.value).toBe(false);
  });
});

describe("pruning", () => {
  it("drops state for a connection that has been deleted", async () => {
    connections.value = [
      { id: "a", name: "home", host: "h", kind: "pve", acceptInvalidCerts: false },
      { id: "b", name: "work", host: "h", kind: "pve", acceptInvalidCerts: false },
    ];
    seed("a", "home", [res({})]);
    seed("b", "work", [res({})]);
    connections.value = [{ id: "a", name: "home", host: "h", kind: "pve", acceptInvalidCerts: false }];
    await nextTick();
    expect(Object.keys(clusters.value)).toEqual(["a"]);
  });
});
