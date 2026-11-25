import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { choiceKeys, confirmationKey, confirmationOf } from "@agency/agent/pane-confirm";

// How a confirmation is read off a pane's screen and refused; putting it to the owner and answering the
// pane is the job runner's (its own tests).

const rule = "─".repeat(60);

// The confirmation a real pane showed on Claude Code 2.1.274 (docs/agent-control.md).
const removalScreen = [
  "❯ Run exactly this one Bash command",
  "● Reading work/a.md and removing all .md files from work directory",
  '  ⎿  $ S=work; echo "$(cat $S/a.md)"; rm -f $S/*.md',
  rule,
  " Bash command",
  '   S=work; echo "$(cat $S/a.md)"; rm -f $S/*.md',
  "   Read work/a.md and remove all .md files from work directory",
  "",
  " Dangerous rm operation on possibly-empty variable path: $S/*.md",
  "",
  " Do you want to proceed?",
  " ❯ 1. Yes",
  "   2. No",
  "",
  " Esc to cancel · Tab to amend",
].join("\n");

describe("a confirmation read off the screen", () => {
  test("a numbered Yes and No below the last rule is a permission question", () => {
    const asked = confirmationOf(removalScreen);
    const text = asked?.kind === "permission" ? asked.text : "";

    expect(asked?.kind).toBe("permission");
    expect(asked?.amendable).toBe(true);
    expect(text).toContain("Dangerous rm operation on possibly-empty variable path: $S/*.md");
    expect(text).toContain("Do you want to proceed?");
    expect(text).not.toContain("1. Yes");
    expect(text).not.toContain("Esc to cancel");
    expect(text).not.toContain("Reading work/a.md");
  });

  test("the keys to each choice are counted from the cursor", () => {
    expect(choiceKeys(removalScreen, "Yes")).toEqual([]);
    expect(choiceKeys(removalScreen, "No")).toEqual(["down"]);

    const moved = removalScreen.replace(" ❯ 1. Yes", "   1. Yes").replace("   2. No", " ❯ 2. No");

    expect(choiceKeys(moved, "Yes")).toEqual(["up"]);
    expect(choiceKeys(removalScreen.replaceAll("❯", " "), "No")).toBeNull();
  });

  // The #860 job's question was put to the owner six times: each look read a different second.
  test("the countdown a question stands under is no part of it, so a ticking question is one question", () => {
    const ticking = (left: string) =>
      removalScreen.replace(
        " Do you want to proceed?",
        ` ⚠ Claude Code will automatically deny this request in ${left}, to avoid blocking progress on an unattended session\n\n Do you want to proceed?`,
      );
    const first = confirmationOf(ticking("1:58"));
    const later = confirmationOf(ticking("0:04"));

    expect(first?.kind === "permission" ? first.text : "").not.toContain("automatically deny");
    expect(first !== null && later !== null && confirmationKey(first) === confirmationKey(later)).toBe(true);
    expect(choiceKeys(ticking("0:56"), "No")).toEqual(["down"]);
    expect(confirmationOf(`${rule}\n Pick a colour\n ⚠ Claude Code will automatically deny this request in 0:30\n ❯ 1. Red`)).toEqual(
      confirmationOf(`${rule}\n Pick a colour\n ❯ 1. Red`),
    );
  });

  test("a screen with no amend hint is not amendable", () => {
    expect(confirmationOf(removalScreen.replace(" · Tab to amend", ""))?.amendable).toBe(false);
  });

  test("any other blocked screen is an unknown question carrying the screen it stood at", () => {
    const asked = confirmationOf(`${rule}\n Pick a colour\n ❯ 1. Red\n   2. Blue`);

    expect(asked).toEqual({ kind: "unknown", screen: "Pick a colour\n ❯ 1. Red\n   2. Blue", amendable: false });
    expect(confirmationOf(`\n${rule}\n  \n`)).toBeNull();
  });
});
