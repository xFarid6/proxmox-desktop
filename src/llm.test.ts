import { describe, expect, it } from "vitest";
import type { ChatMessage, ModelFile } from "./api";
import {
  appendDelta,
  budgetWarning,
  compactMessages,
  conversationTokens,
  isUsable,
  modelSwitchProblem,
  tokenEstimate,
} from "./llm";

const model = (over: Partial<ModelFile> = {}): ModelFile => ({
  file: "Qwen3-14B-UD-Q4_K_XL.gguf",
  path: "/opt/models/Qwen3-14B-UD-Q4_K_XL.gguf",
  bytes: 9_159_818_624,
  loaded: false,
  fits: true,
  ...over,
});

describe("modelSwitchProblem", () => {
  it("allows a model that fits and is not already loaded", () => {
    expect(modelSwitchProblem(model())).toBeNull();
  });

  it("blocks the model already being served", () => {
    expect(modelSwitchProblem(model({ loaded: true }))).toBe("Already loaded.");
  });

  // The switch has to be refused before it is issued: an oversized model does
  // not fail cleanly on the guest, it OOM-loops, and the endpoint that worked
  // a minute ago never comes back.
  it("blocks a model too large for the guest's RAM", () => {
    expect(modelSwitchProblem(model({ fits: false }))).toMatch(/OOM/);
  });
});

describe("tokenEstimate / conversationTokens", () => {
  it("counts roughly a token per four characters", () => {
    expect(tokenEstimate("")).toBe(0);
    expect(tokenEstimate("abcd")).toBe(1);
    expect(tokenEstimate("abcde")).toBe(2);
  });

  it("charges each message for its role framing as well as its text", () => {
    const one: ChatMessage[] = [{ role: "user", content: "abcd" }];
    expect(conversationTokens(one)).toBe(5);
    expect(conversationTokens([...one, { role: "assistant", content: "abcd" }])).toBe(10);
  });
});

describe("budgetWarning", () => {
  it("says nothing while there is room", () => {
    expect(budgetWarning(100, 16384)).toBeNull();
  });

  // The warning has to arrive before truncation, not after: once the window is
  // full the oldest turns are already gone.
  it("warns past three quarters, naming what happens next", () => {
    const warning = budgetWarning(12500, 16384);
    expect(warning).toMatch(/76%/);
    expect(warning).toMatch(/dropped/);
  });

  it("changes its tense once the window is full", () => {
    expect(budgetWarning(17000, 16384)).toMatch(/is dropping/);
  });

  it("says nothing when the window is unknown", () => {
    expect(budgetWarning(999999, 0)).toBeNull();
  });
});

describe("compactMessages", () => {
  const convo = (n: number): ChatMessage[] =>
    Array.from({ length: n }, (_, i) => ({
      role: i % 2 === 0 ? ("user" as const) : ("assistant" as const),
      content: `m${i}`,
    }));

  it("keeps the last N messages verbatim", () => {
    const out = compactMessages(convo(10), "the gist", 4);
    expect(out.slice(-4).map((m) => m.content)).toEqual(["m6", "m7", "m8", "m9"]);
  });

  it("puts the summary first, in place of everything it folded", () => {
    const out = compactMessages(convo(10), "the gist", 4);
    expect(out).toHaveLength(5);
    expect(out[0].content).toContain("the gist");
  });

  // The system message is the instruction the whole conversation runs under;
  // summarising it away would silently change how the model behaves.
  it("never drops the system message", () => {
    const withSystem: ChatMessage[] = [{ role: "system", content: "be terse" }, ...convo(10)];
    const out = compactMessages(withSystem, "the gist", 4);
    expect(out[0]).toEqual({ role: "system", content: "be terse" });
    expect(out[1].content).toContain("the gist");
  });

  it("leaves a conversation short enough to keep alone", () => {
    const short = convo(3);
    expect(compactMessages(short, "the gist", 4)).toBe(short);
  });
});

describe("appendDelta", () => {
  // The bubble is created by the first delta, so a request that dies before
  // producing any text leaves nothing behind to clean up.
  it("starts an assistant message on the first delta", () => {
    const out = appendDelta([{ role: "user", content: "hi" }], "He");
    expect(out).toHaveLength(2);
    expect(out[1]).toEqual({ role: "assistant", content: "He" });
  });

  it("extends the assistant message on later deltas", () => {
    let out: ChatMessage[] = [{ role: "user", content: "hi" }];
    out = appendDelta(out, "He");
    out = appendDelta(out, "llo");
    expect(out).toHaveLength(2);
    expect(out[1].content).toBe("Hello");
  });

  it("returns a new array so the view re-renders", () => {
    const before: ChatMessage[] = [{ role: "user", content: "hi" }];
    expect(appendDelta(before, "x")).not.toBe(before);
  });

  it("ignores an empty delta", () => {
    const before: ChatMessage[] = [{ role: "user", content: "hi" }];
    expect(appendDelta(before, "")).toBe(before);
  });
});

describe("isUsable", () => {
  it("rejects an endpoint that is up but serving nothing", () => {
    expect(isUsable({ baseUrl: "http://x:8080", models: [], manual: false })).toBe(false);
  });

  it("accepts an endpoint with a model loaded", () => {
    expect(isUsable({ baseUrl: "http://x:8080", models: ["qwen3-30b-a3b"], manual: false })).toBe(
      true,
    );
  });

  it("is false when nothing was found", () => {
    expect(isUsable(null)).toBe(false);
  });
});
