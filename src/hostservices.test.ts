import { describe, expect, it } from "vitest";
import type { ListeningPort, ServiceUnit } from "./api";
import { isFailed, sortPorts, sortUnits } from "./hostservices";

function port(p: Partial<ListeningPort>): ListeningPort {
  return { proto: "tcp", address: "0.0.0.0", port: 22, process: null, pid: null, ...p };
}

function unit(u: Partial<ServiceUnit>): ServiceUnit {
  return {
    name: "ssh.service",
    load: "loaded",
    active: "active",
    sub: "running",
    description: "",
    ...u,
  };
}

describe("sortPorts", () => {
  it("orders by port number, not by the order ss emitted them", () => {
    // ss groups udp before tcp, so 41641/udp arrives before 22/tcp.
    const sorted = sortPorts([port({ proto: "udp", port: 41641 }), port({ port: 22 })]);
    expect(sorted.map((p) => p.port)).toEqual([22, 41641]);
  });

  it("keeps a service's IPv4 and IPv6 rows adjacent", () => {
    // Which of the two comes first is left to localeCompare; that they are
    // not separated by another service's row is the point.
    const sorted = sortPorts([
      port({ port: 22, address: "[::]" }),
      port({ port: 8082, address: "0.0.0.0" }),
      port({ port: 22, address: "0.0.0.0" }),
    ]);
    expect(sorted.map((p) => p.port)).toEqual([22, 22, 8082]);
  });

  it("orders the two protocols on one port deterministically", () => {
    const sorted = sortPorts([port({ proto: "udp", port: 53 }), port({ proto: "tcp", port: 53 })]);
    expect(sorted.map((p) => p.proto)).toEqual(["tcp", "udp"]);
  });

  it("does not mutate its input", () => {
    const input = [port({ port: 90 }), port({ port: 80 })];
    sortPorts(input);
    expect(input[0].port).toBe(90);
  });
});

describe("sortUnits", () => {
  it("puts failed units first, whichever column reports the failure", () => {
    const sorted = sortUnits([
      unit({ name: "containerd.service" }),
      unit({ name: "zz-late.service", active: "active", sub: "failed" }),
      unit({ name: "mjpg-streamer.service", active: "failed", sub: "failed" }),
    ]);
    expect(sorted.map((u) => u.name)).toEqual([
      "mjpg-streamer.service",
      "zz-late.service",
      "containerd.service",
    ]);
  });

  it("sorts the healthy remainder alphabetically", () => {
    const sorted = sortUnits([unit({ name: "ssh.service" }), unit({ name: "cron.service" })]);
    expect(sorted.map((u) => u.name)).toEqual(["cron.service", "ssh.service"]);
  });
});

describe("isFailed", () => {
  it("is false for a running unit", () => {
    expect(isFailed(unit({}))).toBe(false);
  });
});
