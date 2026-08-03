import type { ChatMessage, LlmEndpoint, ModelFile } from "./api";
import { api } from "./api";
import type { GuestKind } from "./api";

/** Rough token count for a string.
 *
 * chars/4 is the usual approximation. It is deliberately an estimate: the exact
 * count depends on the model's tokeniser, and asking `/tokenize` per keystroke
 * would be a round trip per character to render a warning. The panel corrects
 * it against the server's own count once a turn.
 */
export function tokenEstimate(text: string): number {
  return Math.ceil(text.length / 4);
}

/** Estimated tokens across a whole conversation.
 *
 * Includes a small per-message allowance for the role framing the server wraps
 * each turn in — small per message, not nothing over a long conversation.
 */
export function conversationTokens(messages: ChatMessage[]): number {
  return messages.reduce((sum, m) => sum + tokenEstimate(m.content) + 4, 0);
}

/** How many turns stay verbatim when a conversation is compacted. */
export const KEEP_VERBATIM = 4;

/** Warn while there is still room to act, not once the wall is hit. */
export const BUDGET_WARN_AT = 0.75;

/** What to tell the user about the context budget, or `null` while there is
 * plenty left.
 *
 * Names the consequence rather than just the number: the server silently drops
 * the oldest turns when the window fills, so "you are at 92%" without "and then
 * the start of this conversation goes" is not actionable.
 */
export function budgetWarning(used: number, nCtx: number): string | null {
  if (nCtx <= 0 || used < nCtx * BUDGET_WARN_AT) return null;
  if (used >= nCtx) {
    return "Past the context window — the server is dropping the oldest turns. Clear or compact.";
  }
  return `${Math.round((used / nCtx) * 100)}% of the context window. At 100% the oldest turns start being dropped — compact to keep the gist of them.`;
}

/** Replace the middle of a conversation with a summary, keeping the last
 * `keepLast` messages verbatim.
 *
 * Purely client-side, because context in an OpenAI-compatible setup is: the
 * client resends the whole array every turn, so nothing on the server has to
 * cooperate.
 *
 * A system message is never dropped — it is the instruction the whole
 * conversation runs under, and summarising it away silently changes behaviour.
 */
export function compactMessages(
  messages: ChatMessage[],
  summary: string,
  keepLast = KEEP_VERBATIM,
): ChatMessage[] {
  const system = messages.filter((m) => m.role === "system");
  const rest = messages.filter((m) => m.role !== "system");
  const tail = keepLast > 0 ? rest.slice(-keepLast) : [];
  const folded = rest.slice(0, rest.length - tail.length);
  if (folded.length === 0) return messages;
  return [
    ...system,
    { role: "user" as const, content: `Summary of the conversation so far:\n${summary}` },
    ...tail,
  ];
}

/** The prompt used to produce that summary. */
export const COMPACT_PROMPT =
  "Summarise the conversation so far in under 200 words. Keep decisions, facts " +
  "and open questions; drop pleasantries. Write it as notes, not prose.";

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
