import { describe, expect, it } from "vitest";
import type { ChatMessage } from "./api";
import { appendDelta, isUsable } from "./llm";

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
