import { describe, expect, it } from "vitest";
import type { ChatMessage, ModelFile } from "./api";
import { appendDelta, isUsable, modelSwitchProblem } from "./llm";

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
