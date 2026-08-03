// Pure logic behind the SSH-host tabs (#104, #105, #106); the views stay thin
// renderers because this project has no component tests.
import type { DockerContainer, ListeningPort, ServiceUnit, StreamEndpoint } from "./api";

/** Listening sockets in the order the question "what is on port N?" wants
 * them: by port, then by protocol, then by address. `ss` emits them grouped
 * by protocol and unsorted within the group, so the same service's IPv4 and
 * IPv6 rows arrive several lines apart. */
export function sortPorts(ports: ListeningPort[]): ListeningPort[] {
  return [...ports].sort(
    (a, b) =>
      a.port - b.port || a.proto.localeCompare(b.proto) || a.address.localeCompare(b.address),
  );
}

/** Units with failed ones first, then alphabetically.
 *
 * A failed unit is the reason to open this tab at all — the 2026-08-02
 * webcam outage was one dead unit among two dozen healthy ones — so it must
 * not sort into the middle of the list. */
export function sortUnits(units: ServiceUnit[]): ServiceUnit[] {
  return [...units].sort(
    (a, b) => Number(isFailed(b)) - Number(isFailed(a)) || a.name.localeCompare(b.name),
  );
}

/** Whether a unit is in a failed state. `active` carries it for a unit
 * systemd gave up on; `sub` catches one whose high-level state has not
 * caught up yet. */
export function isFailed(u: ServiceUnit): boolean {
  return u.active === "failed" || u.sub === "failed";
}

/** Containers by name rather than by creation time, so that two containers
 * with near-identical names land next to each other.
 *
 * That is the 2026-08-02 failure: a duplicate, misconfigured container was
 * masking a working one, and `docker ps` order put them lines apart. */
export function sortContainers(containers: DockerContainer[]): DockerContainer[] {
  return [...containers].sort((a, b) => (a.name || a.id).localeCompare(b.name || b.id));
}

/** A running container attached to no network — it cannot be reached, however
 * healthy its status line reads. The other half of the same outage. */
export function isDetached(c: DockerContainer): boolean {
  return c.state === "running" && c.networks.trim() === "";
}

/** The URL the viewer loads for a detected endpoint (#106).
 *
 * The backend probed over the host's *loopback*, so this is deliberately a
 * different address: the desktop dials the connection's own host. When the
 * two disagree — service alive, path to it broken — the image fails to load,
 * and that is the state the tab has to be able to show. It is the 2026-08-02
 * outage exactly.
 *
 * `nonce` is appended because a browser will happily re-serve a cached frame
 * for the same URL, which would make a retry look successful.
 */
export function streamUrl(host: string, ep: StreamEndpoint, nonce = 0): string {
  // A bare IPv6 literal needs brackets in a URL; one that already has them,
  // or any hostname or IPv4, is used as it stands.
  const authority = host.includes(":") && !host.startsWith("[") ? `[${host}]` : host;
  const sep = ep.path.includes("?") ? "&" : "?";
  return `http://${authority}:${ep.port}${ep.path}${sep}_pxx=${nonce}`;
}

/** Streams before snapshots, then by port. A live feed is what the tab is
 * for; a still is the fallback when a server ignored `?action=`. */
export function sortStreams(endpoints: StreamEndpoint[]): StreamEndpoint[] {
  return [...endpoints].sort(
    (a, b) => Number(b.kind === "stream") - Number(a.kind === "stream") || a.port - b.port,
  );
}
