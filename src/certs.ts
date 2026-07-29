/** How close to `notafter` a certificate has to be before it is flagged. */
export const EXPIRY_WARN_DAYS = 30;

const DAY_MS = 86_400_000;

export type ExpiryState = "expired" | "expiring" | "ok" | "unknown";

/** `notafter` is unix epoch *seconds* (PVE reports every timestamp that way).
 * A cert with no reported expiry is "unknown" rather than fine — an omitted
 * field must not read as a healthy one. */
export function expiryState(notafter?: number, now = Date.now()): ExpiryState {
  if (notafter == null) return "unknown";
  const ms = notafter * 1000 - now;
  if (ms <= 0) return "expired";
  return ms <= EXPIRY_WARN_DAYS * DAY_MS ? "expiring" : "ok";
}

export function expiryLabel(notafter?: number, now = Date.now()): string {
  if (notafter == null) return "no expiry reported";
  const days = (notafter * 1000 - now) / DAY_MS;
  if (days <= 0) {
    const gone = Math.floor(-days);
    return gone < 1 ? "expired today" : `expired ${gone} ${gone === 1 ? "day" : "days"} ago`;
  }
  const left = Math.floor(days);
  return left < 1 ? "expires today" : `${left} ${left === 1 ? "day" : "days"} left`;
}
