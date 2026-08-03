import type { ConnectionInfo, ConnectionKind } from "./api";

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

/** Whether the connection form is collecting SSH credentials.
 *
 * SSH is optional for a PVE connection and mandatory for an SSH host, which has
 * no other way in. #102 enforced that by *writing* `true` into the checkbox when
 * the type changed, and never put the old value back — so switching to SSH host
 * and back left a PVE connection with the SSH shell silently enabled (#114).
 *
 * Derive it instead. The checkbox then only ever holds what the user chose, and
 * switching type back cannot lose it. */
export function sshEnabledFor(kind: ConnectionKind, checkbox: boolean): boolean {
  return kind === "ssh" || checkbox;
}

/** The sidebar/tab-bar nav for the active connection.
 *
 * An SSH host gets Connections, Terminal, Ports & Services, Docker and Stream
 * — there is no PVE API to back Dashboard, Guests, Network, Backups, Firewall,
 * Storage, Ceph, Certificates, Access or HA. With #106 that list is complete;
 * anything added later ships its route and view in the same change, because a
 * link to a view that does not exist is a broken link.
 *
 * Docker and Stream are listed unconditionally rather than probed at connect
 * time: each tab reports a host that cannot serve it, the same way Ports &
 * Services reports a host with no `ss`. Gating the entries would mean extra
 * SSH round trips every time the connection changes, to hide a tab — and the
 * stream probe in particular takes seconds. */
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
    return [
      { to: "/connections", label: "Connections" },
      { to: "/host/terminal", label: "Terminal" },
      { to: "/host/services", label: "Ports & Services" },
      { to: "/host/docker", label: "Docker" },
      { to: "/host/stream", label: "Stream" },
    ];
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
