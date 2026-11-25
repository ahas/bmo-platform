import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { failureText, failureTextLimit } from "@agency/agent/failure-text";

// What the CLI prints before it has done anything: the greeting, a row of dots, the drawing, and
// a rule across the terminal. This is what used to fill a record all by itself.
const banner = [
  "Welcome to Claude Code v2.1.276",
  "...................................................",
  "",
  "     *",
  "                                 *         ███▓░",
  "            ░░░░░░                        ███▓░",
  "   ░░░░░░░░░░░░░░░░░░░    *                ██▓░░",
  "       █████████",
  "      ██▄█████▄██                        *",
  "─".repeat(200),
].join("\n");

describe("what a failed run leaves", () => {
  test("keeps the cause even when the banner is longer than the record", () => {
    const long = `${banner}\n${"filler line that says words\n".repeat(200)}Error: ENOENT: no such file or directory, open '/etc/nope'`;
    const kept = failureText(long);

    expect(kept.endsWith("Error: ENOENT: no such file or directory, open '/etc/nope'")).toBe(true);
    expect(kept).not.toContain("Welcome to Claude Code");
    expect(kept.length).toBeLessThanOrEqual(failureTextLimit + 2);
  });

  test("drops the drawing and the rules while the words around them stay", () => {
    const kept = failureText(`${banner}\nAPIError: 400 invalid request`);

    expect(kept).toContain("Welcome to Claude Code v2.1.276");
    expect(kept).toContain("APIError: 400 invalid request");
    expect(kept).not.toContain("█");
    expect(kept).not.toContain("░");
    expect(kept).not.toContain("─");
    expect(kept).not.toContain("....");
  });

  test("says with a leading ellipsis that a head was dropped", () => {
    const kept = failureText(`${"a".repeat(failureTextLimit)}\nthe last word`);

    expect(kept.startsWith("…")).toBe(true);
    expect(kept.endsWith("the last word")).toBe(true);
  });

  test("never cuts a line in half: the half line at the cut goes", () => {
    const kept = failureText(`${"a".repeat(failureTextLimit)}bbbb\ncccc`, 10);

    expect(kept).toBe("…\ncccc");
  });

  test("keeps a single unbroken line whole rather than dropping all of it", () => {
    expect(failureText("abcdefghijklmnop", 8)).toBe("…ijklmnop");
  });

  test("leaves short output exactly as it was said", () => {
    expect(failureText("Error: connection refused")).toBe("Error: connection refused");
  });
});

describe("what is stripped before the record is measured", () => {
  test("colour and cursor escapes", () => {
    expect(failureText("[31mError: red[0m[2K")).toBe("Error: red");
  });

  test("an operating-system escape that sets the terminal title", () => {
    expect(failureText("]0;claudeError: titled")).toBe("Error: titled");
  });

  test("a progress line redrawn over itself with a carriage return", () => {
    expect(failureText("loading 1%\rloading 2%\rError: gave up")).toBe("loading 1%\nloading 2%\nError: gave up");
  });

  test("trailing spaces a terminal padded its rows with", () => {
    expect(failureText("Error: padded    \nsecond line  ")).toBe("Error: padded\nsecond line");
  });

  test("blank lines, so the room is spent on words", () => {
    expect(failureText("first\n\n\n\nsecond")).toBe("first\nsecond");
  });
});

describe("output with nothing to say", () => {
  test("keeps its own tail rather than leaving the record empty", () => {
    expect(failureText("░░░\n███")).toBe("░░░\n███");
  });

  test("is empty where there was nothing at all", () => {
    expect(failureText("")).toBe("");
    expect(failureText("   \n\n  ")).toBe("");
  });
});
