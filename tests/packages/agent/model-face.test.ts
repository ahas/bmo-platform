import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { modelFace } from "@agency/agent/model-face";

describe("a model by the name a person reads it under", () => {
  // Every model the CLI's catalogue names today, read the way a person says it. None of them is
  // written down: the name comes out of the identifier, so a model named tomorrow reads tomorrow.
  test("is the family and its generation, worked out of the identifier", () => {
    expect(modelFace("claude-opus-5-5")).toBe("Opus 5.5");
    expect(modelFace("claude-fable-5-1")).toBe("Fable 5.1");
    expect(modelFace("claude-sonnet-5")).toBe("Sonnet 5");
    expect(modelFace("claude-opus-5")).toBe("Opus 5");
    expect(modelFace("claude-opus-4-8")).toBe("Opus 4.8");
    expect(modelFace("claude-sonnet-4-6")).toBe("Sonnet 4.6");
  });

  test("reads a model no list has ever met, which is the whole point of deriving it", () => {
    expect(modelFace("claude-opus-9")).toBe("Opus 9");
    expect(modelFace("claude-quartet-7-2")).toBe("Quartet 7.2");
  });

  // A pinned build carries the date it was cut. That says nothing about which generation it is,
  // so it is left off and the two ways of naming one model read alike.
  test("leaves a pinned build's date off the name", () => {
    expect(modelFace("claude-haiku-4-5-20251001")).toBe("Haiku 4.5");
    expect(modelFace("claude-haiku-4-5")).toBe("Haiku 4.5");
    expect(modelFace("claude-opus-4-20250514")).toBe("Opus 4");
  });

  // Nothing is hidden behind a guess: an identifier the rule cannot read stands as it came.
  test("is the identifier itself where the rule cannot read it", () => {
    expect(modelFace("gpt-5.6-sol")).toBe("gpt-5.6-sol");
    expect(modelFace("qwen3.8-27b")).toBe("qwen3.8-27b");
    expect(modelFace("google/gemma-4-31b-it:free")).toBe("google/gemma-4-31b-it:free");
    expect(modelFace("opus")).toBe("opus");
    expect(modelFace("claude-3-5-sonnet-20241022")).toBe("claude-3-5-sonnet-20241022");
    expect(modelFace("")).toBe("");
  });
});
