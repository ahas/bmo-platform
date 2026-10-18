import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

// The tables the console's tests stand on are the server's own (docs/accounts.md, 「The tables」): every table
// statement of the tests' fixture stands in the server's migration word for word, and the tables a user keys
// are the ones the migration keys.

const fixture = readFileSync(new URL("../fixtures/web-tables.sql", import.meta.url), "utf8");
const migration = readFileSync(new URL("../../../server/src/migration/m20261019_000001_tables.rs", import.meta.url), "utf8");

const blocks = fixture.split("\n\n").map((block) => block.trim()).filter((block) => block !== "");

test("every table the tests make is made by the server's migration as it is written there", () => {
  const tables = blocks.filter((block) => block.startsWith("CREATE TABLE"));

  expect(tables.length).toBe(7);

  for (const block of tables) {
    expect(migration).toContain(block);
  }
});

test("the tables the tests key by a user are the ones the migration keys", () => {
  const keyed = blocks.filter((block) => block.startsWith("ALTER TABLE")).map((block) => /^ALTER TABLE "([a-z_]+)"/.exec(block)![1]);

  expect(migration).toContain(`USER_KEYED: [&str; ${keyed.length}] = [${keyed.map((table) => `"${table}"`).join(", ")}]`);
  expect(migration).toContain('ADD COLUMN IF NOT EXISTS "user_id" bigint');
});
