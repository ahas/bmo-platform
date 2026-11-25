import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { codeRoot } from "@agency/support/roots";
import { Archive } from "@agency/archive/archive";
import { scratchDir } from "@agency/test-support/scratch";
import { CLAUDE_ACCOUNT_NAME_MAX, accountRecordsDir, chooseClaudeAccount, chosenAccountOffReason, chosenClaudeAccount, recordedAccount, recordsDirOf, runAccountFor, setClaudeAccountEnabled, SEED_ACCOUNT_ID, SEED_ACCOUNT_NAME, addClaudeAccount, addNamedClaudeAccount, claudeAccount, claudeAccountChangeFault, claudeAccountId, claudeAccounts, claudeCredentialFault, credentialFilePath, enabledClaudeAccounts, lastAccountOffReason, machineConfigDir, newClaudeAccountFault, seedClaudeAccount, updateClaudeAccount } from "@agency/agent/claude-account";
import controlAnswers from "@agency/policy/control-words.toml";
import { fillMarks } from "@agency/support/marks";
import { scratchArchive } from "@agency/test-support/store";

const answers = controlAnswers.accounts;

// The login every session here is opened under. A run that names no account goes out on the one
// the machine already had, which is the account standing for it, and that is what a record says.
const account = SEED_ACCOUNT_ID;

// The Claude logins agency holds. What is written down is where a credential is reached, never the
// credential, so most of this is about what a row may hold and what is refused before one is
// written.

const root = scratchDir("bmo-claude-account-");
const second = join(root, "second-login");
const tokenFile = join(root, "token-account.json");

mkdirSync(second, { recursive: true });
writeFileSync(tokenFile, "not read by anything under test\n");

async function archive(): Promise<Archive> {
  return await scratchArchive();
}

describe("the login this machine already has", () => {
  test("is the one account a fresh archive holds", async () => {
    const held = await claudeAccounts(await archive());

    expect(held).toHaveLength(1);
    expect(held[0]?.id).toBe(SEED_ACCOUNT_ID);
    expect(held[0]?.name).toBe(SEED_ACCOUNT_NAME);
    expect(held[0]?.enabled).toBe(true);
    expect(held[0]?.credential).toEqual({ kind: "directory", path: machineConfigDir() });
  });

  test("is the configuration directory the CLI itself would use", () => {
    // The preload gives every test file a home and a configuration directory of its own, which
    // is exactly what the CLI would read in this process.
    expect(machineConfigDir()).toBe(process.env.CLAUDE_CONFIG_DIR as string);

    const named = process.env.CLAUDE_CONFIG_DIR;

    try {
      delete process.env.CLAUDE_CONFIG_DIR;
      expect(machineConfigDir()).toBe(join(process.env.HOME as string, ".claude"));
    } finally {
      process.env.CLAUDE_CONFIG_DIR = named;
    }
  });

  test("is where the CLI keeps that login", () => {
    expect(credentialFilePath(seedClaudeAccount().credential)).toBe(join(machineConfigDir(), ".credentials.json"));
    expect(credentialFilePath({ kind: "token", path: tokenFile })).toBe(tokenFile);
  });
});

describe("where an account's own records are", () => {
  test("a directory account's are under its configuration home, a token account's under the machine's", () => {
    expect(accountRecordsDir({ kind: "directory", path: second })).toBe(second);
    expect(accountRecordsDir({ kind: "token", path: tokenFile })).toBe(machineConfigDir());
    expect(accountRecordsDir(null)).toBe(machineConfigDir());
  });

  test("an account is found by the id a record names, and an id nothing answers to reads as the machine's", async () => {
    const store = await archive();
    const added = await addNamedClaudeAccount(store, { name: "Second account", credential: { kind: "directory", path: second } });

    expect(await recordsDirOf(store, added.id)).toBe(second);
    expect(await recordsDirOf(store, SEED_ACCOUNT_ID)).toBe(machineConfigDir());
    expect(await recordsDirOf(store, "nobody")).toBe(machineConfigDir());
    expect(await recordsDirOf(store, null)).toBe(machineConfigDir());
    expect(await runAccountFor(store, added.id)).toEqual({ id: added.id, name: added.name, credential: { kind: "directory", path: second } });
    expect(await runAccountFor(store, "nobody")).toBeNull();
  });

  test("a run that names no account is recorded under the account standing for the machine's login", () => {
    expect(recordedAccount(null)).toBe(SEED_ACCOUNT_ID);
    expect(recordedAccount(undefined)).toBe(SEED_ACCOUNT_ID);
    expect(recordedAccount({ id: "second" })).toBe("second");
  });
});

describe("what may be written into an account", () => {
  test("refuses a relative path", () => {
    expect(claudeCredentialFault({ kind: "directory", path: "second-login" })).toBe(answers.path_relative);
    expect(claudeCredentialFault({ kind: "token", path: "./token.json" })).toBe(answers.path_relative);
  });

  test("refuses a missing file and a missing directory", () => {
    expect(claudeCredentialFault({ kind: "token", path: join(root, "nothing.json") })).toBe(answers.path_missing);
    expect(claudeCredentialFault({ kind: "directory", path: join(root, "nowhere") })).toBe(answers.path_missing);
  });

  test("refuses a directory where a token file belongs, and the other way about", () => {
    expect(claudeCredentialFault({ kind: "token", path: second })).toBe(answers.path_not_file);
    expect(claudeCredentialFault({ kind: "directory", path: tokenFile })).toBe(answers.path_not_directory);
  });

  test("takes a directory account and a token account", () => {
    expect(claudeCredentialFault({ kind: "directory", path: second })).toBeNull();
    expect(claudeCredentialFault({ kind: "token", path: tokenFile })).toBeNull();
  });

  test("refuses a duplicate name", async () => {
    const store = await archive();
    const held = await claudeAccounts(store);

    expect(
      newClaudeAccountFault(
        { id: "second", name: SEED_ACCOUNT_NAME, credential: { kind: "directory", path: second } },
        held,
      ),
    ).toBe(fillMarks(answers.name_taken, { name: SEED_ACCOUNT_NAME }));
    expect(() =>
      addClaudeAccount(store, { id: "second", name: `  ${SEED_ACCOUNT_NAME}  `, credential: { kind: "directory", path: second } }),
    ).toThrow(fillMarks(answers.name_taken, { name: SEED_ACCOUNT_NAME }));
    expect(await claudeAccounts(store)).toHaveLength(1);
  });

  test("refuses a relative path where it is written, not only where it is judged", async () => {
    const store = await archive();

    expect(() =>
      addClaudeAccount(store, { id: "second", name: "Second account", credential: { kind: "directory", path: "second-login" } }),
    ).toThrow(answers.path_relative);
    expect(() =>
      updateClaudeAccount(store, SEED_ACCOUNT_ID, { credential: { kind: "token", path: "token-account.json" } }),
    ).toThrow(answers.path_relative);
    expect(await claudeAccounts(store)).toHaveLength(1);
    expect((await claudeAccounts(store))[0]?.credential.kind).toBe("directory");
  });

  test("refuses a duplicate id", async () => {
    const store = await archive();

    expect(() =>
      addClaudeAccount(store, { id: SEED_ACCOUNT_ID, name: "Second account", credential: { kind: "directory", path: second } }),
    ).toThrow(fillMarks(answers.id_taken, { id: SEED_ACCOUNT_ID }));
  });

  test("refuses an id outside the small alphabet, and a name with nothing in it", async () => {
    const held = await claudeAccounts(await archive());

    expect(newClaudeAccountFault({ id: "Main Login", name: "Second account", credential: { kind: "directory", path: second } }, held))
      .toBe(answers.id_shape);
    expect(newClaudeAccountFault({ id: "second", name: "   ", credential: { kind: "directory", path: second } }, held))
      .toBe(answers.name_empty);
    expect(
      newClaudeAccountFault(
        { id: "second", name: "€".repeat(CLAUDE_ACCOUNT_NAME_MAX + 1), credential: { kind: "directory", path: second } },
        held,
      ),
    ).toBe(fillMarks(answers.name_long, { most: String(CLAUDE_ACCOUNT_NAME_MAX) }));
  });
});

describe("an id minted from the name the owner gave", () => {
  test("is the name where the name can be one", () => {
    expect(claudeAccountId("Main", [])).toBe("main");
    expect(claudeAccountId("dev.owner", [])).toBe("dev.owner");
  });

  test("is made anyway where the name holds nothing usable", () => {
    expect(claudeAccountId("★ ★", [])).toBe("account");
    expect(claudeAccountId("★ ★", ["account"])).toBe("account-2");
    expect(claudeAccountId("★ ★", ["account", "account-2"])).toBe("account-3");
  });

  test("never takes an id already held", () => {
    expect(claudeAccountId("main", ["main"])).toBe("main-2");
  });
});

describe("the accounts as a list", () => {
  test("keeps the order they were added in, an added account last", async () => {
    const store = await archive();

    await addNamedClaudeAccount(store, { name: "Second account", credential: { kind: "directory", path: second } });
    await addNamedClaudeAccount(store, { name: "Token account", credential: { kind: "token", path: tokenFile } });

    expect((await claudeAccounts(store)).map((one) => one.id)).toEqual([SEED_ACCOUNT_ID, "second-account", "token-account"]);
    expect((await claudeAccounts(store)).map((one) => one.place)).toEqual([1, 2, 3]);
    expect((await claudeAccounts(store)).at(-1)?.credential).toEqual({ kind: "token", path: tokenFile });
  });

  test("is asked by id or by the name the owner calls it", async () => {
    const store = await archive();

    expect((await claudeAccount(store, SEED_ACCOUNT_ID))?.id).toBe(SEED_ACCOUNT_ID);
    expect((await claudeAccount(store, SEED_ACCOUNT_NAME))?.id).toBe(SEED_ACCOUNT_ID);
    expect(await claudeAccount(store, "nobody")).toBeNull();
  });

  test("holds only the accounts that are on when that is what is asked", async () => {
    const store = await archive();

    await addNamedClaudeAccount(store, { name: "Second account", credential: { kind: "directory", path: second } });
    await updateClaudeAccount(store, SEED_ACCOUNT_ID, { enabled: false });

    expect((await enabledClaudeAccounts(store)).map((one) => one.name)).toEqual(["Second account"]);
    // A turned-off account keeps its place, so turning it back on returns it to where it was.
    expect((await claudeAccounts(store)).map((one) => one.id)).toEqual([SEED_ACCOUNT_ID, "second-account"]);
  });
});

describe("changing an account", () => {
  test("moves what was named and stores a path as an absolute one", async () => {
    const store = await archive();
    const changed = await updateClaudeAccount(store, SEED_ACCOUNT_ID, {
      name: "  Main account  ",
      credential: { kind: "token", path: tokenFile },
    });

    expect(changed.name).toBe("Main account");
    expect(changed.credential).toEqual({ kind: "token", path: tokenFile });
    expect(changed.id).toBe(SEED_ACCOUNT_ID);
  });

  test("refuses an account nobody registered", async () => {
    expect(claudeAccountChangeFault("nobody", { enabled: false }, await claudeAccounts(await archive()))).toBe(fillMarks(answers.unknown, { id: "nobody" }));
  });

  test("refuses turning off the last one that is on", async () => {
    const store = await archive();

    expect(() => updateClaudeAccount(store, SEED_ACCOUNT_ID, { enabled: false })).toThrow(lastAccountOffReason);

    await addNamedClaudeAccount(store, { name: "Second account", credential: { kind: "directory", path: second } });
    await updateClaudeAccount(store, SEED_ACCOUNT_ID, { enabled: false });

    expect(() => updateClaudeAccount(store, "second-account", { enabled: false })).toThrow(lastAccountOffReason);
  });

  test("refuses a name another account already holds, and takes its own back", async () => {
    const store = await archive();

    await addNamedClaudeAccount(store, { name: "Second account", credential: { kind: "directory", path: second } });

    expect(() => updateClaudeAccount(store, "second-account", { name: SEED_ACCOUNT_NAME })).toThrow(SEED_ACCOUNT_NAME);
    expect((await updateClaudeAccount(store, "second-account", { name: "Second account" })).name).toBe("Second account");
  });
});

// The order the Claude Code entry walks its accounts in. It is the accounts' own places and
// switches held together, so what is tested here is mostly what cannot be written: an order that
// does not hold every account once, and one that leaves nothing on.

async function twoAccounts(): Promise<Archive> {
  const store = await archive();

  await addNamedClaudeAccount(store, { name: "Second account", credential: { kind: "directory", path: second } });
  await addNamedClaudeAccount(store, { name: "Token account", credential: { kind: "token", path: tokenFile } });

  return store;
}

// The account the owner chose to run on now: one setting beside the rows, only ever naming an account that
// is on, and let go with that account the moment it is turned off, removed or the order is reset.
describe("the account chosen to run on now", () => {
  test("is none with nothing said, and one that is on once chosen", async () => {
    const store = await twoAccounts();

    expect(await chosenClaudeAccount(store)).toBeNull();

    await chooseClaudeAccount(store, "second-account");

    expect((await chosenClaudeAccount(store))?.id).toBe("second-account");
    // The order the owner dragged is not touched by it.
    expect((await claudeAccounts(store)).map((one) => one.id)).toEqual([SEED_ACCOUNT_ID, "second-account", "token-account"]);

    await chooseClaudeAccount(store, null);
    expect(await chosenClaudeAccount(store)).toBeNull();
  });

  test("refuses an account that is off or not there", async () => {
    const store = await twoAccounts();

    await setClaudeAccountEnabled(store, "second-account", false);

    expect(() => chooseClaudeAccount(store, "second-account")).toThrow(chosenAccountOffReason);
    expect(() => chooseClaudeAccount(store, "nobody")).toThrow(fillMarks(answers.unknown, { id: "nobody" }));
    expect(await chosenClaudeAccount(store)).toBeNull();
  });
});

// An endpoint account is Claude Code run against another provider's Anthropic-compatible endpoint
// (docs/cache-fallback.md, 「The Claude Code road is a list of accounts」): a configuration home of
// its own that holds no login, the endpoint's base URL, the file its key sits in and the model ids
// its family words stand for.
describe("an endpoint account", () => {
  const home = join(root, "deepseek-home");
  const keyFile = join(root, "deepseek.key");

  function endpoint(models: { opus?: string; sonnet?: string; haiku?: string } = { opus: "deepseek-v4-pro", sonnet: "deepseek-flash", haiku: "deepseek-flash" }) {
    return { kind: "endpoint" as const, path: home, endpointUrl: "https://api.deepseek.com/anthropic", keyFile, models };
  }

  test("is kept as its own home, base URL, key file and model map, and read back whole", async () => {
    const store = await archive();

    mkdirSync(home, { recursive: true });
    writeFileSync(keyFile, "sk-not-a-real-key\n");
    await store.sql.run(
      "INSERT INTO base_prompts (name, label, text, updated_at, updated_by) VALUES (?, ?, ?, ?, ?)",
      "dsh", "DeepSeek Harness based", "the harness's own prompt", 1, "test",
    );

    const added = await addNamedClaudeAccount(store, { name: "DeepSeek", credential: endpoint(), basePrompt: "dsh" });

    expect(added.credential).toEqual(endpoint());
    expect(added.basePrompt).toBe("dsh");
    // The key is read as a run is launched and by nothing here: no row holds any part of it.
    expect(JSON.stringify(await claudeAccounts(store))).not.toContain("sk-not-a-real-key");
    // Its home keeps the run's records, and it holds no login file for an end to be read off.
    expect(accountRecordsDir(added.credential)).toBe(home);
    expect(credentialFilePath(added.credential)).toBeNull();
  });

  test("names a base prompt only when one is kept", async () => {
    const store = await archive();

    mkdirSync(home, { recursive: true });
    writeFileSync(keyFile, "sk-not-a-real-key\n");

    await expect(addNamedClaudeAccount(store, { name: "DeepSeek", credential: endpoint(), basePrompt: "nobody-keeps-this" })).rejects.toThrow();
    expect(await claudeAccounts(store)).toHaveLength(1);
  });

  test("needs a home that is there, an http or https address and a key file", () => {
    expect(claudeCredentialFault({ ...endpoint(), path: join(root, "nowhere") })).toBe(answers.path_missing);
    expect(claudeCredentialFault({ ...endpoint(), path: keyFile })).toBe(answers.path_not_directory);
    expect(claudeCredentialFault({ ...endpoint(), endpointUrl: "api.deepseek.com" })).toBe(answers.endpoint_url);
    expect(claudeCredentialFault({ ...endpoint(), endpointUrl: "ftp://api.deepseek.com" })).toBe(answers.endpoint_url);
    expect(claudeCredentialFault({ ...endpoint(), keyFile: join(root, "missing.key") })).toBe(answers.path_missing);
    expect(claudeCredentialFault({ ...endpoint(), keyFile: home })).toBe(answers.key_not_file);
    expect(claudeCredentialFault(endpoint())).toBeNull();
  });

  test("a row this road cannot use is left out rather than read as another kind", async () => {
    const store = await archive();

    mkdirSync(home, { recursive: true });
    writeFileSync(keyFile, "sk-not-a-real-key\n");

    const row = (id: string, place: number, kind: string, url: string | null, key: string | null, models: string | null): Promise<unknown> =>
      store.sql.run(
        `INSERT INTO claude_accounts (
          id, name, place, enabled, credential_kind, credential_path,
          endpoint_url, key_file, endpoint_models, added_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
        id, id, place, 1, kind, home, url, key, models, 1,
      );

    await row("no-url", 9, "endpoint", "", keyFile, null);
    await row("unknown-kind", 10, "mystery", null, null, null);
    await row("bad-map", 11, "endpoint", "https://api.deepseek.com/anthropic", keyFile, "{\"opus\": 7}");
    await row("unknown-family", 12, "endpoint", "https://api.deepseek.com/anthropic", keyFile, "{\"gpt\": \"x\"}");
    await row("well-formed", 13, "endpoint", "https://api.deepseek.com/anthropic", keyFile, "{\"opus\": \"deepseek-v4-pro\"}");

    expect((await claudeAccounts(store)).map((one) => one.id)).toEqual([SEED_ACCOUNT_ID, "well-formed"]);
  });
});

// The rule the whole shape stands on: the archive holds where a credential is, and nothing that
// touches a credential's own bytes belongs in this module. A reader that has to know when a login
// ends opens the file for that one moment and keeps the moment, which is its own work elsewhere.
test("nothing here opens a credential file", () => {
  const source = readFileSync(join(codeRoot, "packages", "agent", "claude-account.ts"), "utf8");

  for (const forbidden of ["readFileSync", "readFile(", "Bun.file", "createReadStream", "readdirSync"]) {
    expect(source).not.toInclude(forbidden);
  }
});
