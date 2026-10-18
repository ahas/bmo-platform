import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";

import { changesView, diffSummary, diffView, shownPath } from "../../../apps/web/app/domain/patch";

import type { AgentLogPatch } from "../../../apps/web/shared/types";
import KO from "../../fixtures/app/domain/patch.test.ko.json";

// Trimmed from what Claude Code records for its file tools: an edit's hunks with no type, a
// replaced file's hunks under "update", and a created file's content under "create".
const root = "/home/user/repos/project";

const edit: AgentLogPatch = {
  filePath: `${root}/server/finished.test.ts`,
  type: null,
  by: "tool",
  hunks: [
    {
      oldStart: 172,
      oldLines: 7,
      newStart: 172,
      newLines: 8,
      lines: [
        "     for (const depth of [\"basic\", \"thorough\", \"exhaustive\"] as const) {",
        "       const args = searchArgs(depth);",
        " ",
        "-      expect(args[args.indexOf(\"--tools\") + 1]).toBe(\"Task\");",
        "+      // No built-in tool at all: branches come from the web server's pooled branch tool.",
        "+      expect(args[args.indexOf(\"--tools\") + 1]).toBe(\"\");",
        "     }",
        "\\ No newline at end of file"
      ]
    }
  ],
  content: null,
  tooLarge: false
};

const multi: AgentLogPatch = {
  filePath: `${root}/docs/web.md`,
  type: null,
  by: "tool",
  hunks: [
    { oldStart: 3, oldLines: 2, newStart: 3, newLines: 1, lines: [" a", "-b"] },
    { oldStart: 40, oldLines: 1, newStart: 39, newLines: 2, lines: [" c", "+d"] }
  ],
  content: null,
  tooLarge: false
};

const created: AgentLogPatch = {
  filePath: "/home/user/.local/share/app/data/workbench/worker-d/manual/recon/1C-uploader.md",
  type: "create",
  by: "tool",
  hunks: [],
  content: "# Uploader Reconnaissance\n\n## Build Status\n",
  tooLarge: false
};

describe("a change drawn the way the terminal draws it", () => {
  test("an edit heads as an update of the path under the checkout, and says what moved", () => {
    const view = diffView(wording, edit, root);

    expect(view?.head).toBe("Update(server/finished.test.ts)");
    expect(view?.summary).toBe(KO.line_65);
  });

  test("a removed line keeps the number it had, an added one the number it has, a kept one the new one", () => {
    const view = diffView(wording, edit, root);

    expect(view?.rows).toEqual([
      { kind: "line", mark: " ", old: 172, new: 172, text: "    for (const depth of [\"basic\", \"thorough\", \"exhaustive\"] as const) {" },
      { kind: "line", mark: " ", old: 173, new: 173, text: "      const args = searchArgs(depth);" },
      { kind: "line", mark: " ", old: 174, new: 174, text: "" },
      { kind: "line", mark: "-", old: 175, new: null, text: "      expect(args[args.indexOf(\"--tools\") + 1]).toBe(\"Task\");" },
      { kind: "line", mark: "+", old: null, new: 175, text: "      // No built-in tool at all: branches come from the web server's pooled branch tool." },
      { kind: "line", mark: "+", old: null, new: 176, text: "      expect(args[args.indexOf(\"--tools\") + 1]).toBe(\"\");" },
      { kind: "line", mark: " ", old: 176, new: 177, text: "    }" }
    ]);
  });

  test("several hunks stand apart, each numbered from its own start", () => {
    const view = diffView(wording, multi, root);

    expect(view?.rows).toEqual([
      { kind: "line", mark: " ", old: 3, new: 3, text: "a" },
      { kind: "line", mark: "-", old: 4, new: null, text: "b" },
      { kind: "gap" },
      { kind: "line", mark: " ", old: 40, new: 39, text: "c" },
      { kind: "line", mark: "+", old: null, new: 40, text: "d" }
    ]);
    expect(view?.summary).toBe(KO.line_92);
  });

  test("a created file is a write of every line of it, and a path outside the checkout stands whole", () => {
    const view = diffView(wording, created, root);

    expect(view?.head).toBe(`Write(${created.filePath})`);
    expect(view?.summary).toBe(wording("agents.label.diff_added", { added: "3" }));
    expect(view?.rows.map((row) => (row.kind === "line" ? [row.mark, row.new, row.text] : row.kind))).toEqual([
      ["+", 1, "# Uploader Reconnaissance"],
      ["+", 2, ""],
      ["+", 3, "## Build Status"]
    ]);
  });

  test("a replaced file heads as a write", () => {
    expect(diffView(wording, { ...edit, type: "update" }, null)?.head).toBe(`Write(${edit.filePath})`);
  });

  test("a deleted file is every line of it removed", () => {
    const view = diffView(wording, { ...created, type: "delete", content: "a\nb\n" }, null);

    expect(view?.head).toBe(`Delete(${created.filePath})`);
    expect(view?.summary).toBe(wording("agents.label.diff_removed", { removed: "2" }));
    expect(view?.rows.map((row) => (row.kind === "line" ? [row.mark, row.old, row.new] : row.kind))).toEqual([
      ["-", 1, null],
      ["-", 2, null]
    ]);
  });

  test("a file a shell command changed heads as the terminal heads it under the command", () => {
    const view = diffView(wording, { ...edit, by: "command" }, root);

    expect(view?.head).toBe("Updated server/finished.test.ts (+2 -1)");
    expect(view?.byCommand).toBe(true);
    expect(diffView(wording, edit, root)?.byCommand).toBe(false);
  });

  test("nothing to draw is no view", () => {
    expect(diffView(wording, null, root)).toBeNull();
    expect(diffView(wording, { ...edit, hunks: [], tooLarge: true }, root)).toBeNull();
    expect(diffView(wording, { ...created, content: null }, root)).toBeNull();
  });
});

describe("every file one result changed", () => {
  test("each file stands on its own, and the fold line counts them all", () => {
    const view = changesView(wording, [{ ...edit, by: "command" }, { ...multi, by: "command" }], 0, root);

    expect(view?.files.map((file) => file.head)).toEqual([
      "Updated server/finished.test.ts (+2 -1)",
      "Updated docs/web.md (+1 -1)"
    ]);
    expect(view?.summary).toBe(KO.line_145);
    expect(view?.rows).toBe(12);
    expect(view?.more).toBeNull();
  });

  test("files only counted are said as a count, and a result with nothing to draw is no view", () => {
    expect(changesView(wording, [edit], 3, root)?.more).toBe(wording("agents.label.diff_more_files", { n: "3" }));
    expect(changesView(wording, [], 3, root)).toBeNull();
    expect(changesView(wording, [{ ...edit, hunks: [], tooLarge: true }], 0, root)).toBeNull();
  });
});

describe("the words and the path around it", () => {
  test("only the side that moved is named, and numbers carry separators", () => {
    expect(diffSummary(wording, 1_200, 0)).toBe(wording("agents.label.diff_added", { added: "1,200" }));
    expect(diffSummary(wording, 0, 3)).toBe(wording("agents.label.diff_removed", { removed: "3" }));
    expect(diffSummary(wording, 0, 0)).toBe(wording("agents.label.diff_none"));
  });

  test("a path reads from the checkout only when it lies inside it", () => {
    expect(shownPath(`${root}/a.ts`, root)).toBe("a.ts");
    expect(shownPath(`${root}/a.ts`, `${root}/`)).toBe("a.ts");
    expect(shownPath(`${root}-other/a.ts`, root)).toBe(`${root}-other/a.ts`);
    expect(shownPath("/etc/hosts", null)).toBe("/etc/hosts");
  });
});
