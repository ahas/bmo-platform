import "@agency/test-support/setup";

import { expect, test } from "bun:test";
import { join } from "node:path";
import { Archive } from "@agency/archive/archive";
import { beginExecution, executionRoot, nativeExecution } from "@agency/agent/execution-identity";
import { mapInOrder } from "@agency/support/in-order";
import { OWNER_ID } from "@agency/test-support/ids";
import { scratchArchive, scratchDatabase } from "@agency/test-support/store";

test("independent batches have separate roots even in one bot service", async () => {
  const archive = await scratchArchive();
  const a = await beginExecution(archive, "codex", "gate", { root: executionRoot("bmo.service"), role: "translator", model: "model-a" });
  const b = await beginExecution(archive, "codex", "gate", { root: executionRoot("bmo.service"), role: "translator", model: "model-a" });
  expect(a.root).not.toBe(b.root);
  expect(a.attempt).toBe(1); expect(b.attempt).toBe(1);
});

test("retries and provider switches retain root with fresh physical identities", async () => {
  const archive = await scratchArchive();
  const root = executionRoot("bmo-task-123");
  const records = await mapInOrder(["codex", "codex", "openrouter"], provider => beginExecution(archive, provider, "worker", { root, role: "worker", model: `${provider}-model` }));
  expect(records.map(r => r.attempt)).toEqual([1, 2, 3]);
  expect(new Set(records.map(r => r.id)).size).toBe(3);
  expect(records.every(r => r.root === root)).toBe(true);
  expect(records[2]!.sessionId).toBeNull();
});

test("native opaque IDs cannot collide across parents or escape into paths", async () => {
  const archive = await scratchArchive();
  const a = await beginExecution(archive, "codex", "worker", { model: "parent-model", sessionId: "thread" });
  const b = await beginExecution(archive, "codex", "worker", {});
  const id = '../../x/\u0000日本語';
  const child = nativeExecution(a, id);
  expect(child.id).toMatch(/^bmo-native-[a-f0-9]{64}$/);
  expect(child.id).toBe(nativeExecution(a, id).id);
  expect(child.id).not.toBe(nativeExecution(b, id).id);
  expect(child.parent).toBe(a.id); expect(child.root).toBe(a.root);
  expect(child.nativeId).toBe(id); expect(child.model).toBeNull();
  await archive.executions().saveExecution(child);
  expect(() => archive.executions().saveExecution({ ...child, parent: b.id })).toThrow("identity cannot change");
  expect((await archive.executions().executions()).find(r => r.id === child.id)?.parent).toBe(a.id);
});

test("allocation persists across connections and duplicate physical IDs roll back", async () => {
  const path = await scratchDatabase();
  const first = new Archive(path);
  const a = await beginExecution(first, "codex", "worker", { root: "root" });
  expect(() => first.executions().beginExecution(a)).toThrow();
  const second = new Archive(path);
  const b = await beginExecution(second, "openrouter", "worker", { root: "root" });
  expect(b.attempt).toBe(2);
  await first.close();
  await second.close();
});

test("parallel allocators cannot reuse a root attempt", async () => {
  const path = await scratchDatabase();
  const archive = new Archive(path);
  const source = `import '@agency/test-support/registrations'; import { Archive } from '@agency/archive/archive'; import { beginExecution } from '@agency/agent/execution-identity';
    const db = new Archive(process.argv[1]);
    await Promise.all(Array.from({ length: 8 }, () => beginExecution(db, 'codex', 'worker', {root:'shared',role:'worker'})));
    await db.close();`;
  const results = await Promise.all(Array.from({ length: 3 }, async () => {
    // Each allocator is a process of its own on the test's store, resolving the workspace as the tests do.
    const proc = Bun.spawn([process.execPath, '-e', source, path], { cwd: join(import.meta.dir, '..'), env: { ...process.env }, stdout: 'pipe', stderr: 'pipe' });
    const error = new Response(proc.stderr).text();
    expect(await proc.exited).toBe(0); expect(await error).toBe('');
  }));
  expect(results).toHaveLength(3);
  const rows = await archive.executions().executions();
  expect(rows.map(r => r.attempt).sort((a,b) => a! - b!)).toEqual(Array.from({length:24},(_,i)=>i+1));
  expect(new Set(rows.map(r => r.id)).size).toBe(24);
});

test("physical history preserves the role and effective effort without inventing native settings", async () => {
  const archive = await scratchArchive();
  const record = await beginExecution(archive, "codex", "worker", { role: "builder", model: "gpt-5.6-terra", effort: "low" });
  record.state = "exited"; await archive.executions().saveExecution(record);
  expect((await archive.executions().executions())[0]).toMatchObject({ role: "builder", model: "gpt-5.6-terra", effort: "low" });
  expect(nativeExecution(record, "child")).toMatchObject({ role: "builder", model: null, effort: null });
});
