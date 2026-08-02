import type { ConnectionInfo } from "./api";

export interface NavItem {
  to: string;
  label: string;
}

/** An SSH host has no PVE API and no API token — every PVE-only view must be
 * absent from the nav for it (#102), not merely disabled. `undefined`/`null`
 * (no connection chosen yet) is treated as PVE so the app keeps working
 * before any connection exists. */
export function isSshHost(c: ConnectionInfo | undefined | null): boolean {
  return c?.kind === "ssh";
}

/** The sidebar/tab-bar nav for the active connection.
 *
 * An SSH host only gets the Connections link back — there is no PVE API to
 * back Dashboard, Guests, Network, Backups, Firewall, Storage, Ceph,
 * Certificates, Access or HA. The Terminal / Ports & Services / Docker /
 * Stream tabs an SSH host *does* support are #103-#106; each of those PRs
 * appends its own entry here together with the route and view it needs. This
 * list is meant to grow — shipping a link to a view that does not exist yet
 * would be a broken link. */
/** Whether a route is reachable for the active connection.
 *
 * Hiding a nav entry is not enough on its own: the router keeps whatever route
 * you were already on when the cluster picker switches connections, so
 * selecting an SSH host while sitting on `/dashboard` would leave a PVE-only
 * view on screen with no PVE API behind it. `App.vue` watches this and
 * redirects.
 *
 * Matched by prefix rather than against `navFor`'s list, because the PVE nav
 * omits plenty of legitimately reachable routes (`/guests/pve1/lxc/100`, the
 * console, the per-node shell). Only an SSH host is restricted, and it is
 * restricted to exactly two prefixes: `/connections`, and the `/host/*` tabs
 * that #103-#106 add. */
export function routeAllowedFor(c: ConnectionInfo | undefined | null, path: string): boolean {
  if (!isSshHost(c)) return true;
  return path.startsWith("/connections") || path.startsWith("/host/");
}

export function navFor(
  c: ConnectionInfo | undefined | null,
  opts: { cephAvailable: boolean },
): NavItem[] {
  if (isSshHost(c)) {
    return [{ to: "/connections", label: "Connections" }];
  }
  return [
    { to: "/connections", label: "Connections" },
    { to: "/dashboard", label: "Dashboard" },
    { to: "/guests", label: "VMs & CTs" },
    { to: "/tasks", label: "Tasks" },
    { to: "/network", label: "Network" },
    { to: "/backups", label: "Backups" },
    { to: "/firewall", label: "Firewall" },
    { to: "/storage", label: "Storage" },
    ...(opts.cephAvailable ? [{ to: "/ceph", label: "Ceph" }] : []),
    { to: "/certificates", label: "Certificates" },
    { to: "/access", label: "Access" },
    { to: "/ha", label: "HA" },
  ];
}
