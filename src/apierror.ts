/** Rewrites a rejected backend call into something a user can act on.
 *
 * Every Tauri command rejects with the Rust `Error`'s string form, and for an
 * API failure that string carries the PVE response body verbatim:
 *
 *   Proxmox API error (403): {"message":"Permission check failed (/, Sys.Audit)\n","data":null}
 *
 * which tells a Proxmox admin everything and everyone else nothing. The shapes
 * below get replaced with plain language; anything unrecognised is passed
 * through untouched, since a raw error still beats a swallowed one.
 */

const API_ERROR = /^Proxmox API error \((\d+)\): ([\s\S]*)$/;

/** PVE's own wording, e.g. `Permission check failed (/vms/100, VM.Backup)`.
 * The privilege is sometimes a comma-separated list. */
const PERM_CHECK = /Permission check failed \(([^,)]*),\s*([^)]*)\)/;

/** The path and privilege PVE refused a call over, or null if the error is not
 * a permission failure. Callers that need to *react* to a missing privilege
 * (rather than just print it) use this. */
export function missingPrivilege(e: unknown): { path: string; privilege: string } | null {
  const api = API_ERROR.exec(String(e));
  if (!api || api[1] !== "403") return null;
  const perm = PERM_CHECK.exec(api[2]);
  if (!perm) return null;
  return { path: perm[1].trim(), privilege: perm[2].trim() };
}

function whereToGrant(path: string, privilege: string): string {
  const where = path === "/" ? "the whole datacenter (/)" : path;
  return (
    `Your API token is missing the ${privilege} privilege on ${where}. ` +
    "Grant it in Proxmox under Datacenter → Permissions → Add → API Token Permission, " +
    `picking a role that includes ${privilege} — or clear the token's ` +
    '"Privilege Separation" checkbox so it inherits its user\'s rights.'
  );
}

export function explainError(e: unknown): string {
  const text = String(e);
  const api = API_ERROR.exec(text);
  if (!api) return text;

  const status = Number(api[1]);
  if (status === 403) {
    const perm = missingPrivilege(e);
    return perm
      ? whereToGrant(perm.path, perm.privilege)
      : "Proxmox refused this call as unauthorised (403). The API token most likely " +
          "lacks a privilege this page needs; check its permissions under " +
          "Datacenter → Permissions.";
  }
  if (status === 401) {
    return (
      "Proxmox rejected the API token (401). Check the Token ID " +
      "(user@realm!tokenid) and the secret — the secret is the UUID shown once, " +
      "when the token was created, and cannot be read back afterwards."
    );
  }
  return text;
}
