import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { frontMatter } from "@agency/agent/prefix";

describe("frontMatter", () => {
  test("is the head a skill is listed by, and not the rest of the document", () => {
    expect(frontMatter("---\nname: one\ndescription: two\n---\nbody that nobody reads yet\n")).toBe(
      "name: one\ndescription: two",
    );
  });

  test("a document with no head is all head, since there is nothing to hold back", () => {
    expect(frontMatter("plain\n")).toBe("plain\n");
  });
});

