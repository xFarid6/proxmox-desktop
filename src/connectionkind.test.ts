import { describe, expect, it } from "vitest";
import type { ConnectionInfo } from "./api";
import { isSshHost, navFor, routeAllowedFor, sshEnabledFor } from "./connectionkind";

const pveConn: ConnectionInfo = {
  id: "1",
  name: "cluster",
  host: "https://pve.example.com:8006",
  kind: "pve",
  acceptInvalidCerts: false,
};

const sshConn: ConnectionInfo = {
  id: "2",
  name: "wyse-server",
  host: "wyse-server",
  kind: "ssh",
  acceptInvalidCerts: false,
  ssh: { user: "root", port: 22, useAgent: true },
};

const PVE_ONLY_ROUTES = [
  "/dashboard",
  "/guests",
  "/network",
  "/backups",
  "/firewall",
  "/storage",
  "/ceph",
  "/certificates",
  "/access",
  "/ha",
];

describe("navFor", () => {
  it("gives an SSH host none of the PVE-only routes", () => {
    const routes = navFor(sshConn, { cephAvailable: true }).map((i) => i.to);
    for (const route of PVE_ONLY_ROUTES) {
      expect(routes).not.toContain(route);
    }
  });

  it("still gives an SSH host a way back to Connections", () => {
    const routes = navFor(sshConn, { cephAvailable: true }).map((i) => i.to);
    expect(routes).toContain("/connections");
  });

  it("gives an SSH host its terminal tab", () => {
    const routes = navFor(sshConn, { cephAvailable: true }).map((i) => i.to);
    expect(routes).toContain("/host/terminal");
  });

  it("gives an SSH host its ports & services tab", () => {
    const routes = navFor(sshConn, { cephAvailable: true }).map((i) => i.to);
    expect(routes).toContain("/host/services");
  });

  it("gives an SSH host its docker tab", () => {
    const routes = navFor(sshConn, { cephAvailable: true }).map((i) => i.to);
    expect(routes).toContain("/host/docker");
  });

  it("gives an SSH host its stream tab", () => {
    const routes = navFor(sshConn, { cephAvailable: true }).map((i) => i.to);
    expect(routes).toContain("/host/stream");
  });

  it("does not offer the SSH-host terminal tab to a PVE connection", () => {
    // A PVE node's shell is reached per-node from the dashboard, not from a
    // connection-wide tab — /host/* belongs to SSH hosts only.
    const routes = navFor(pveConn, { cephAvailable: true }).map((i) => i.to);
    expect(routes).not.toContain("/host/terminal");
  });

  it("gives a PVE connection today's full nav, minus Ceph when unavailable", () => {
    const routes = navFor(pveConn, { cephAvailable: false }).map((i) => i.to);
    expect(routes).toEqual([
      "/connections",
      "/dashboard",
      "/guests",
      "/tasks",
      "/network",
      "/backups",
      "/firewall",
      "/storage",
      "/certificates",
      "/access",
      "/ha",
    ]);
  });

  it("includes Ceph for a PVE connection once it is available", () => {
    const routes = navFor(pveConn, { cephAvailable: true }).map((i) => i.to);
    expect(routes).toContain("/ceph");
  });

  it("treats no connection selected as PVE, so the sidebar isn't blank", () => {
    const routes = navFor(undefined, { cephAvailable: false }).map((i) => i.to);
    expect(routes).toEqual(navFor(pveConn, { cephAvailable: false }).map((i) => i.to));
  });
});

describe("routeAllowedFor", () => {
  it("never restricts a PVE connection, including its deep routes", () => {
    for (const path of ["/dashboard", "/guests/pve1/lxc/100", "/nodes/pve1/ssh", "/ha"]) {
      expect(routeAllowedFor(pveConn, path)).toBe(true);
    }
  });

  it("keeps an SSH host off every PVE-only route", () => {
    for (const path of PVE_ONLY_ROUTES) {
      expect(routeAllowedFor(sshConn, path)).toBe(false);
    }
  });

  it("lets an SSH host reach Connections and its own /host tabs", () => {
    // /host/* is where #103-#106 hang the terminal, ports, docker and stream
    // tabs — they must not need this guard revisited when they land.
    for (const path of [
      "/connections",
      "/host/terminal",
      "/host/services",
      "/host/docker",
      "/host/stream",
    ]) {
      expect(routeAllowedFor(sshConn, path)).toBe(true);
    }
  });
});

describe("sshEnabledFor", () => {
  it("is on for an SSH host whatever the checkbox says", () => {
    expect(sshEnabledFor("ssh", false)).toBe(true);
    expect(sshEnabledFor("ssh", true)).toBe(true);
  });

  it("follows the checkbox for a PVE connection", () => {
    expect(sshEnabledFor("pve", false)).toBe(false);
    expect(sshEnabledFor("pve", true)).toBe(true);
  });

  // #114: the old code wrote `true` into the checkbox on switching to SSH host
  // and never restored it, so coming back gave a PVE connection an SSH shell
  // the user never asked for. Deriving means the checkbox is untouched, so the
  // round trip has to end where it started.
  it("survives a pve -> ssh -> pve round trip with the checkbox off", () => {
    const checkbox = false;
    expect(sshEnabledFor("ssh", checkbox)).toBe(true);
    expect(sshEnabledFor("pve", checkbox)).toBe(false);
  });
});

describe("isSshHost", () => {
  it("is true for an ssh-kind connection", () => {
    expect(isSshHost(sshConn)).toBe(true);
  });

  it("is false for a pve-kind connection", () => {
    expect(isSshHost(pveConn)).toBe(false);
  });

  it("is false when there is no connection yet", () => {
    expect(isSshHost(undefined)).toBe(false);
    expect(isSshHost(null)).toBe(false);
  });
});
