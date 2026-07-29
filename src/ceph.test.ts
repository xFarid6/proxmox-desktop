import { describe, expect, it } from "vitest";
import { flattenOsdTree, healthClass, osdPercent, pgStates, poolPercent } from "./ceph";

describe("flattenOsdTree", () => {
  it("flattens osds and attributes each to its host bucket", () => {
    const rows = flattenOsdTree({
      root: {
        name: "default",
        type: "root",
        children: [
          {
            name: "pve2",
            type: "host",
            children: [{ id: 1, name: "osd.1", type: "osd", status: "down", in: 1 }],
          },
          {
            name: "pve1",
            type: "host",
            children: [
              {
                id: 0,
                name: "osd.0",
                type: "osd",
                status: "up",
                in: 1,
                device_class: "ssd",
                percent_used: 41.5,
              },
            ],
          },
        ],
      },
    });
    expect(rows.map((r) => r.id)).toEqual([0, 1]);
    expect(rows[0]).toMatchObject({ host: "pve1", deviceClass: "ssd", up: true, in: true });
    expect(rows[1]).toMatchObject({ host: "pve2", deviceClass: "—", up: false, in: true });
  });

  it("descends through intermediate buckets, keeping the host", () => {
    const rows = flattenOsdTree({
      root: {
        type: "root",
        children: [
          {
            name: "rack1",
            type: "rack",
            children: [
              {
                name: "pve3",
                type: "host",
                children: [{ id: 7, type: "osd", status: "up", in: 0 }],
              },
            ],
          },
        ],
      },
    });
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ id: 7, name: "osd.7", host: "pve3", in: false });
  });

  it("is empty for a missing tree", () => {
    expect(flattenOsdTree(undefined)).toEqual([]);
    expect(flattenOsdTree({})).toEqual([]);
  });
});

describe("osdPercent", () => {
  const base = { id: 0, name: "osd.0", host: "pve1", deviceClass: "ssd", up: true, in: true };

  it("prefers the byte counts over the reported percentage", () => {
    expect(osdPercent({ ...base, used: 250, total: 1000, percentUsed: 0.25 })).toBe(25);
  });

  it("falls back to the reported percentage when bytes are missing", () => {
    expect(osdPercent({ ...base, percentUsed: 41.6 })).toBe(42);
    expect(osdPercent(base)).toBe(0);
  });
});

describe("poolPercent", () => {
  it("scales ceph df's fraction up to a whole percent", () => {
    expect(poolPercent({ pool_name: "vmdata", percent_used: 0.1234 })).toBe(12);
    expect(poolPercent({ pool_name: "empty" })).toBe(0);
  });
});

describe("pgStates", () => {
  it("sorts states by count, busiest first", () => {
    expect(
      pgStates({
        pgmap: {
          pgs_by_state: [
            { state_name: "active+recovering", count: 3 },
            { state_name: "active+clean", count: 125 },
          ],
        },
      }),
    ).toEqual([
      { state: "active+clean", count: 125 },
      { state: "active+recovering", count: 3 },
    ]);
  });

  it("is empty when the cluster reports no pgmap", () => {
    expect(pgStates(undefined)).toEqual([]);
    expect(pgStates({ pgmap: {} })).toEqual([]);
  });
});

describe("healthClass", () => {
  it("maps ceph health strings onto the app's three colours", () => {
    expect(healthClass("HEALTH_OK")).toBe("ok");
    expect(healthClass("HEALTH_WARN")).toBe("warn");
    expect(healthClass("HEALTH_ERR")).toBe("bad");
  });

  it("treats unknown or missing health as a warning, not as healthy", () => {
    expect(healthClass(undefined)).toBe("warn");
    expect(healthClass("HEALTH_SOMETHING_NEW")).toBe("warn");
  });
});
