import "@agency/test-support/setup";

import { afterAll, describe, expect, test } from "bun:test";
import { appendFileSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { Archive } from "@agency/archive/archive";
import { scratchArchive } from "@agency/test-support/store";
import { CodexFileChanges, recordedChange, unifiedHunks } from "@agency/agent/codex-file-changes";
import { RunLog } from "@agency/agent/runlog";
import { scratchDir } from "@agency/test-support/scratch";
import { OWNER_ID } from "@agency/test-support/ids";

const root = scratchDir("bmo-codex-file-changes-");

afterAll(() => rmSync(root, { recursive: true, force: true }));

// What Codex writes into its session record when a patch has been applied, trimmed to what matters.
function fileChange(changes: Record<string, unknown>): string {
  return `${JSON.stringify({
    timestamp: "2026-09-14T19:15:17.063Z",
    type: "event_msg",
    payload: { type: "item_completed", item: { type: "FileChange", id: "exec-1", changes, status: "completed" } },
  })}\n`;
}

const update = { type: "update", unified_diff: "@@ -26,2 +26,3 @@\n \n+new line\n end\n", move_path: null };

function sessions(name: string, thread: string): { dir: string; file: string } {
  const dir = join(root, name);
  const day = join(dir, "2026", "09", "14");

  mkdirSync(day, { recursive: true });

  return { dir, file: join(day, `rollout-2026-09-14T22-59-04-${thread}.jsonl`) };
}

describe("a unified diff", () => {
  test("reads as hunks with their numbers, leaving out what is no line of the file", () => {
    expect(unifiedHunks("--- a/x\n+++ b/x\n@@ -3 +3,2 @@\n-a\n+b\n+c\n\\ No newline at end of file\n@@ -10,2 +11,2 @@\n x\n\n")).toEqual([
      { oldStart: 3, oldLines: 1, newStart: 3, newLines: 2, lines: ["-a", "+b", "+c"] },
      { oldStart: 10, oldLines: 2, newStart: 11, newLines: 2, lines: [" x", " "] },
    ]);
  });
});

describe("a FileChange of the session record", () => {
  test("an update, a created, a deleted and a moved file each read as the screen draws them", () => {
    const change = recordedChange(JSON.parse(fileChange({
      "/r/a.ts": update,
      "/r/b.ts": { type: "add", content: "b\n" },
      "/r/c.ts": { type: "delete", content: "c\n" },
      "/r/d.ts": { type: "update", unified_diff: "@@ -1 +1 @@\n-d\n+e\n", move_path: "/r/e.ts" },
    })));

    expect(change?.paths).toEqual(["/r/a.ts", "/r/b.ts", "/r/c.ts", "/r/d.ts"]);
    expect(change?.diffs).toEqual([
      { filePath: "/r/a.ts", type: null, hunks: [{ oldStart: 26, oldLines: 2, newStart: 26, newLines: 3, lines: [" ", "+new line", " end"] }], content: null },
      { filePath: "/r/b.ts", type: "create", hunks: [], content: "b\n" },
      { filePath: "/r/c.ts", type: "delete", hunks: [], content: "c\n" },
      { filePath: "/r/e.ts", type: null, hunks: [{ oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: ["-d", "+e"] }], content: null },
    ]);
  });

  test("another record is none", () => {
    expect(recordedChange({ type: "event_msg", payload: { type: "token_count" } })).toBeNull();
    expect(recordedChange({ type: "response_item", payload: { item: { type: "FileChange", changes: {} } } })).toBeNull();
  });
});

describe("a thread's file changes", () => {
  test("a resumed thread is read from where it stood, and changes are matched by their paths in order", () => {
    const { dir, file } = sessions("resumed", "t-resumed");

    writeFileSync(file, fileChange({ "/r/old.ts": update }));

    const changes = new CodexFileChanges(dir);

    changes.started("t-resumed");
    appendFileSync(file, fileChange({ "/r/skipped.ts": update }) + fileChange({ "/r/a.ts": update }));

    expect(changes.take(["/r/old.ts"], () => {})).toBeNull();
    expect(changes.take(["/r/a.ts"], () => {})?.[0]?.filePath).toBe("/r/a.ts");
  });

  test("a change the record has not written yet is filled in once it has", async () => {
    const { dir, file } = sessions("late", "t-late");
    const changes = new CodexFileChanges(dir);
    const filled: string[] = [];

    changes.started("t-late");
    expect(changes.take(["/r/a.ts"], (diffs) => void filled.push(diffs[0]!.filePath))).toBeNull();
    expect(changes.pending).toBe(1);

    // Half a line is not read until the rest of it is written.
    const line = fileChange({ "/r/a.ts": update });

    writeFileSync(file, line.slice(0, 40));
    await changes.settle();
    expect(filled).toEqual([]);

    appendFileSync(file, line.slice(40));
    await changes.settle();
    expect(filled).toEqual(["/r/a.ts"]);
    expect(changes.pending).toBe(0);
  });
});

describe("a Codex job's log", () => {
  test("a file_change result carries its diffs, whether the record had them at once or only later", async () => {
    const { dir, file } = sessions("job", "t-job");
    const archive = await scratchArchive();
    const jobId = 485;
    const log = new RunLog({ archive, agent: "worker", run: "bmo-task-codex", chatId: OWNER_ID, jobId });
    const event = (id: string, path: string) =>
      `${JSON.stringify({ type: "item.completed", item: { id, type: "file_change", changes: [{ path, kind: "update" }], status: "completed" } })}\n`;

    writeFileSync(file, "");
    // The CodexFileChanges a job's log opens reads the machine's own record; this one is pointed at the test's.
    const original = CodexFileChanges.prototype.started;
    CodexFileChanges.prototype.started = function (thread: string) {
      (this as unknown as { root: string }).root = dir;
      original.call(this, thread);
    };

    try {
      const encode = (text: string) => new TextEncoder().encode(text);

      await log.feed(encode(`${JSON.stringify({ type: "thread.started", thread_id: "t-job" })}\n`), "codex");
      appendFileSync(file, fileChange({ "/r/a.ts": update }));
      await log.feed(encode(event("item_1", "/r/a.ts")), "codex");
      await log.feed(encode(event("item_2", "/r/b.ts")), "codex");
      appendFileSync(file, fileChange({ "/r/b.ts": { type: "add", content: "b\n" } }));
      await log.finish("codex");
    } finally {
      CodexFileChanges.prototype.started = original;
    }

    const rows = await archive.sql.all("SELECT metadata FROM agent_log WHERE job_id = ? AND kind = 'tool-result' ORDER BY id", jobId) as Array<{ metadata: string }>;

    expect(rows.map((row) => JSON.parse(row.metadata).fileDiffs?.[0]?.filePath)).toEqual(["/r/a.ts", "/r/b.ts"]);
    expect(JSON.parse(rows[1]!.metadata).item.id).toBe("item_2");
  });
});
