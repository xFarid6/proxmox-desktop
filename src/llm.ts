import type { ChatMessage, LlmEndpoint, ModelFile } from "./api";
import { api } from "./api";
import type { GuestKind } from "./api";

/** Append a streamed delta to the reply in progress.
 *
 * Returns a new array rather than mutating: the view renders straight off this
 * list, and Vue only re-renders a reactive array it was handed.
 *
 * The assistant message is created by the *first* delta rather than up front,
 * so a request that fails before producing any text leaves no empty bubble.
 */
export function appendDelta(messages: ChatMessage[], delta: string): ChatMessage[] {
  if (!delta) return messages;
  const last = messages[messages.length - 1];
  if (last?.role === "assistant") {
    return [...messages.slice(0, -1), { ...last, content: last.content + delta }];
  }
  return [...messages, { role: "assistant", content: delta }];
}

/** Whether a probe result is worth showing a chat panel for.
 *
 * A server that answers `/v1/models` with an empty list is up but has nothing
 * loaded — there is nothing to send a completion to, so the tab stays hidden
 * rather than offering a model picker with no models in it.
 */
export function isUsable(endpoint: LlmEndpoint | null): boolean {
  return !!endpoint && endpoint.models.length > 0;
}

/** How long a model reload is allowed to take before the panel stops waiting.
 *
 * A 10 GiB model takes about a minute to load on the CPU box this was built
 * for, so the cap is generous — giving up early would report a failure while
 * the guest is still doing exactly what it was asked. */
export const RELOAD_TIMEOUT_MS = 300_000;
export const RELOAD_POLL_MS = 3_000;

/** Why this model cannot be switched to, or `null` if it can.
 *
 * Refusing an oversized model up front is the point: a model that does not fit
 * does not fail cleanly on the guest, it OOM-loops, and the endpoint that was
 * working before the switch never comes back.
 */
export function modelSwitchProblem(m: ModelFile): string | null {
  if (m.loaded) return "Already loaded.";
  if (!m.fits) return "Too large for this guest's RAM — it would OOM-loop instead of loading.";
  return null;
}

/** Cached probe results, keyed per guest.
 *
 * Same shape as `ceph.ts`'s `probed`: the answer only changes when someone
 * installs or removes the service, and a probe costs seconds when it misses —
 * far too slow to repeat on every visit to a guest's page.
 */
const probed = new Map<string, LlmEndpoint | null>();

function key(connectionId: string, kind: GuestKind, vmid: number): string {
  return `${connectionId}/${kind}/${vmid}`;
}

export async function probeLlm(
  connectionId: string,
  kind: GuestKind,
  vmid: number,
  guestName: string,
  force = false,
): Promise<LlmEndpoint | null> {
  const k = key(connectionId, kind, vmid);
  if (force) probed.delete(k);
  if (!probed.has(k)) {
    try {
      probed.set(k, await api.llmProbe(connectionId, kind, vmid, guestName));
    } catch {
      // No SSH configured, guest powered off, no `pct` on the node: all
      // ordinary reasons for a guest to have no LLM, none worth an error.
      probed.set(k, null);
    }
  }
  return probed.get(k) ?? null;
}

/** Drop a guest's cached probe, so the next call goes back to the network.
 * Used after the user pins or clears a manual endpoint. */
export function forgetProbe(connectionId: string, kind: GuestKind, vmid: number): void {
  probed.delete(key(connectionId, kind, vmid));
}
