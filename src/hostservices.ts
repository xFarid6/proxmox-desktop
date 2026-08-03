// Pure logic behind the SSH-host tabs (#104, #105); the views stay thin
// renderers because this project has no component tests.
import type { DockerContainer, ListeningPort, ServiceUnit } from "./api";

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
