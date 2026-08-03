import { describe, expect, it } from "vitest";
import type { DockerContainer, ListeningPort, ServiceUnit, StreamEndpoint } from "./api";
import {
  isDetached,
  isFailed,
  sortContainers,
  sortPorts,
  sortStreams,
  sortUnits,
  streamUrl,
} from "./hostservices";

function endpoint(e: Partial<StreamEndpoint>): StreamEndpoint {
  return {
    port: 8082,
    path: "/?action=stream",
    contentType: "multipart/x-mixed-replace;boundary=boundarydonotcross",
    kind: "stream",
    process: "docker-proxy",
    ...e,
  };
}

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

function container(c: Partial<DockerContainer>): DockerContainer {
  return {
    id: "abc123abc123",
    name: "web",
    image: "nginx",
    state: "running",
    status: "Up 2 hours",
    ports: "",
    networks: "bridge",
    restartPolicy: null,
    ...c,
  };
}

describe("sortContainers", () => {
  it("puts near-identical names next to each other", () => {
    const sorted = sortContainers([
      container({ id: "1", name: "webcam" }),
      container({ id: "2", name: "zzz" }),
      container({ id: "3", name: "webcam-old" }),
    ]);
    expect(sorted.map((c) => c.name)).toEqual(["webcam", "webcam-old", "zzz"]);
  });

  it("falls back to the id for an unnamed container", () => {
    const sorted = sortContainers([container({ id: "b", name: "" }), container({ id: "a", name: "" })]);
    expect(sorted.map((c) => c.id)).toEqual(["a", "b"]);
  });
});

describe("isDetached", () => {
  it("flags a running container attached to no network", () => {
    expect(isDetached(container({ networks: "" }))).toBe(true);
    expect(isDetached(container({ networks: "  " }))).toBe(true);
  });

  it("does not flag an attached container, or a stopped one", () => {
    expect(isDetached(container({}))).toBe(false);
    // A stopped container has no attachment by definition — not a fault.
    expect(isDetached(container({ state: "exited", networks: "" }))).toBe(false);
  });
});

describe("streamUrl", () => {
  it("builds the URL wyse-server's webcam is actually reached at", () => {
    expect(streamUrl("100.77.208.85", endpoint({}))).toBe(
      "http://100.77.208.85:8082/?action=stream&_pxx=0",
    );
  });

  it("brackets a bare IPv6 host", () => {
    expect(streamUrl("fd7a:115c:a1e0::3b34:d055", endpoint({}), 1)).toBe(
      "http://[fd7a:115c:a1e0::3b34:d055]:8082/?action=stream&_pxx=1",
    );
    // Already bracketed: left alone rather than doubled.
    expect(streamUrl("[::1]", endpoint({ port: 80, path: "/" }))).toBe("http://[::1]:80/?_pxx=0");
  });

  it("changes with the nonce so a retry cannot be served from cache", () => {
    const ep = endpoint({});
    expect(streamUrl("host", ep, 1)).not.toBe(streamUrl("host", ep, 2));
  });

  it("keeps the probed path's own query intact", () => {
    // The path came back from a probe that proved it answers; rewriting it
    // would point the viewer somewhere nothing was verified.
    expect(streamUrl("host", endpoint({}))).toContain("/?action=stream&");
  });
});

describe("sortStreams", () => {
  it("puts a live stream above a still, then orders by port", () => {
    const sorted = sortStreams([
      endpoint({ port: 9000, kind: "snapshot" }),
      endpoint({ port: 8083 }),
      endpoint({ port: 8082 }),
    ]);
    expect(sorted.map((e) => e.port)).toEqual([8082, 8083, 9000]);
  });
});
