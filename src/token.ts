/** The connection form takes an API token in the two halves Proxmox's own
 * token-creation dialog shows — Token ID and Secret — rather than as one
 * free-text box, because the box is where `user@realm!tokenid:uuid` comes
 * from: the wrong separator, a 401 with no explanation, and nothing on screen
 * saying which half is wrong (#86).
 *
 * The wire format does not change. PVE wants `PVEAPIToken=user@realm!tokenid=uuid`,
 * so the two halves are joined with `=` here, in one place, instead of by the
 * user.
 */

/** `user@realm!tokenid` — no whitespace, exactly one `@` and one `!`. `:` and
 * `=` are excluded from every part on purpose: a Token ID that contains one is
 * a mis-joined pair, which is the whole mistake this module exists to catch. */
const TOKEN_ID = /^[^\s@!:=]+@[^\s@!:=]+![^\s@!:=]+$/;

/** The full token to send, or `""` for "leave the stored one alone" — which is
 * what both fields being empty means when editing an existing connection. */
export function joinToken(tokenId: string, secret: string): string {
  const id = tokenId.trim();
  const value = secret.trim();
  // Someone who pastes the whole `id=secret` string into the first field meant
  // to give us a complete token, so take it rather than mangling it.
  if (id.includes("=")) return id;
  if (!id || !value) return "";
  return `${id}=${value}`;
}

/** What is wrong with the two fields, or `""` if they are usable as they are.
 * Both empty is usable: it means "unchanged" on an edit, and the caller
 * decides whether that is allowed. */
export function tokenProblem(tokenId: string, secret: string): string {
  const id = tokenId.trim();
  const value = secret.trim();
  if (!id && !value) return "";
  if (id.includes("=")) return "";
  if (!id) return "Enter the Token ID too — Proxmox shows it as user@realm!tokenid.";
  if (!TOKEN_ID.test(id)) {
    return `"${id}" is not a Token ID. Proxmox shows it as user@realm!tokenid, for example root@pam!desktop.`;
  }
  if (!value) {
    return "Enter the secret too — the UUID Proxmox showed once, when the token was created.";
  }
  return "";
}
