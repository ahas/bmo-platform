import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { Archive } from "@agency/archive/archive";
import { addNamedClaudeAccount, chooseClaudeAccount, claudeAccountInUse, holdClaudeAccount, SEED_ACCOUNT_ID, setClaudeAccountEnabled } from "@agency/agent/claude-account";
import {
  accountFailure,
  ClaudeAccountFailed,
  ClaudeLimitError,
  ClaudeOutageError,
  loginGoneWording,
} from "@agency/agent/claude-failure";
import {
  claudeAccountOpen,
  claudeAccountWalk,
  onClaudeAccounts,
  walkingClaudeAccount,
} from "@agency/agent/claude-dispatch";
import { accountRestUntil, isAccountResting } from "@agency/agent/rest-state";
import { scratchDir } from "@agency/test-support/scratch";

import type { RestSettings } from "@agency/agent/rest-state";
import type { HarnessRun } from "@agency/agent/harness";
import { scratchArchive, scratchDatabase } from "@agency/test-support/store";

// The Claude road walked login by login. A run goes out on one account, and when that account is
// what failed — its window full, its login gone — the next open account carries the run and the
// caller never learns the first was tried. What is not the account's is not carried: the service
// being down would fail on every login alike, and the work's own failure is the work's.

const root = scratchDir("bmo-dispatch-");
const config: RestSettings = { restFallbackMs: 3_600_000 };
const hour = Math.floor(Date.now() / 1000) + 3600;

async function twoAccounts(): Promise<{ archive: Archive; second: string }> {
  const archive = await scratchArchive();
  const dir = join(root, "second-login");

  mkdirSync(dir, { recursive: true });

  const second = await addNamedClaudeAccount(archive, { name: "Second account", credential: { kind: "directory", path: dir } });

  return { archive, second: second.id };
}

function run(): HarnessRun {
  return {
    line: "gate",
    model: "claude-sonnet-5",
    effort: "medium",
    systemPromptPath: null,
    toolsShown: "",
    toolsAllowed: null,
    toolsDenied: null,
    mcpConfigPath: null,
    prompt: "judge this",
    promptDelivery: "input",
    resumeSessionId: null,
    keepSession: false,
    hostCommands: false,
    thinking: true,
    answerShape: "object",
    cwd: root,
    env: {},
  };
}

describe("the logins a work key is offered", () => {
  test("every open login, in the owner's order", async () => {
    const { archive, second } = await twoAccounts();

    expect((await claudeAccountWalk(archive)).map((one) => one.id)).toEqual([SEED_ACCOUNT_ID, second]);
    expect(await claudeAccountOpen(archive, second)).toBe(true);
  });

  test("a login turned off is not offered at all", async () => {
    const { archive, second } = await twoAccounts();

    await setClaudeAccountEnabled(archive, second, false);

    expect((await claudeAccountWalk(archive)).map((one) => one.id)).toEqual([SEED_ACCOUNT_ID]);
    expect(await claudeAccountOpen(archive, second)).toBe(false);
  });

  test("an endpoint account is open while its key is there, and passed over while it is not", async () => {
    const archive = await scratchArchive();
    const home = join(root, "endpoint-home");
    const keyFile = join(root, "endpoint.key");

    mkdirSync(home, { recursive: true });
    writeFileSync(keyFile, "sk-not-a-real-key\n");

    const added = await addNamedClaudeAccount(archive, {
      name: "DeepSeek",
      credential: { kind: "endpoint", path: home, endpointUrl: "https://api.deepseek.com/anthropic", keyFile, models: {} },
    });

    expect(await claudeAccountOpen(archive, added.id)).toBe(true);
    expect((await claudeAccountWalk(archive)).map((one) => one.id)).toEqual([SEED_ACCOUNT_ID, added.id]);

    // Nothing to hand a run is no account to run on: the walk passes it over rather than launching
    // one that cannot reach its key.
    rmSync(keyFile);
    expect(await claudeAccountOpen(archive, added.id)).toBe(false);
    expect((await claudeAccountWalk(archive)).map((one) => one.id)).toEqual([SEED_ACCOUNT_ID]);

    writeFileSync(keyFile, "");
    expect(await claudeAccountOpen(archive, added.id)).toBe(false);

    writeFileSync(keyFile, "sk-a-fresh-key\n");
    expect(await claudeAccountOpen(archive, added.id)).toBe(true);
  });

  test("a login waiting for a person is passed over by that answer too", async () => {
    const { archive, second } = await twoAccounts();

    await archive.claudeAccounts().setClaudeAccountLogin(SEED_ACCOUNT_ID, Math.floor(Date.now() / 1000));

    expect(await claudeAccountOpen(archive, SEED_ACCOUNT_ID)).toBe(false);
    expect(await walkingClaudeAccount(archive)).toBe(second);
  });

});

// The account the owner chose to run on now is where the walk starts, and the order after it is the owner's order
// as it stands: the choice moves automatic runs and rewrites nothing else.
describe("the account chosen to run on now", () => {
  async function threeAccounts(): Promise<{ archive: Archive; second: string; third: string }> {
    const { archive, second } = await twoAccounts();
    const dir = join(root, "third-login");

    mkdirSync(dir, { recursive: true });

    return { archive, second, third: (await addNamedClaudeAccount(archive, { name: "Third", credential: { kind: "directory", path: dir } })).id };
  }

  test("is walked first, and the rest of the order follows from the top", async () => {
    const { archive, second, third } = await threeAccounts();

    await chooseClaudeAccount(archive, third);

    expect((await claudeAccountWalk(archive)).map((one) => one.id)).toEqual([third, SEED_ACCOUNT_ID, second]);
    expect(await walkingClaudeAccount(archive)).toBe(third);
  });

  test("carries a run it cannot answer on to the next login in the order", async () => {
    const { archive, second } = await twoAccounts();
    const asked: string[] = [];

    await chooseClaudeAccount(archive, second);

    const answer = await onClaudeAccounts({ archive, config }, "bmo", run(), async (attempt) => {
      asked.push(attempt.account?.id ?? "none");

      if (attempt.account?.id === second) {
        throw new ClaudeLimitError(hour);
      }

      return "an answer";
    });

    expect(answer).toBe("an answer");
    expect(asked).toEqual([second, SEED_ACCOUNT_ID]);
    expect(await isAccountResting(archive, second)).toBe(true);
    expect(await walkingClaudeAccount(archive)).toBe(SEED_ACCOUNT_ID);
  });
});

// A login in use is used to the end: once the road went on to the next login while the one ahead of
// it was out, every run stays there when that one answers again — the first account of the order and
// the chosen one alike — and moves only when the login in use stops answering or the owner chooses.
describe("the login in use", () => {
  async function answered(archive: Archive, key: string, failing: string | null = null): Promise<string[]> {
    const asked: string[] = [];

    await onClaudeAccounts({ archive, config }, key, run(), async (attempt) => {
      asked.push(attempt.account?.id ?? "none");

      if (attempt.account?.id === failing) {
        throw new ClaudeLimitError(hour);
      }

      return "an answer";
    });

    return asked;
  }

  async function carriedPastSpent(archive: Archive, second: string): Promise<void> {
    expect(await answered(archive, "bmo", SEED_ACCOUNT_ID)).toEqual([SEED_ACCOUNT_ID, second]);
    expect(await claudeAccountInUse(archive)).toBe(second);
  }

  test("is left for the next login when its login goes", async () => {
    const { archive, second } = await twoAccounts();

    await holdClaudeAccount(archive, second);
    await archive.claudeAccounts().setClaudeAccountLogin(second, Math.floor(Date.now() / 1000));

    expect(await answered(archive, "bmo")).toEqual([SEED_ACCOUNT_ID]);
    expect(await claudeAccountInUse(archive)).toBe(SEED_ACCOUNT_ID);
  });

  test("is not moved back by a run that went out on the login before the road moved", async () => {
    const { archive, second } = await twoAccounts();
    let release = (): void => {};
    const held = new Promise<void>((resolve) => { release = resolve; });
    const asked: string[] = [];
    let wentOut = (): void => {};
    const out = new Promise<void>((resolve) => { wentOut = resolve; });

    // A run that went out on the first login and is still going when another run spends it: the road
    // moves only once the run has gone out, however long its login takes to be read.
    const early = onClaudeAccounts({ archive, config }, "bmo", run(), async (attempt) => {
      asked.push(attempt.account?.id ?? "none");
      wentOut();
      await held;

      return "an answer";
    });

    await out;
    await carriedPastSpent(archive, second);
    release();
    await early;

    expect(asked).toEqual([SEED_ACCOUNT_ID]);
    expect(await claudeAccountInUse(archive)).toBe(second);
  });

});

describe("what a failure is about", () => {
  test("the account's own: its window full, its login gone", () => {
    expect(accountFailure({ error: new ClaudeLimitError(hour) })).toBe("limit");
    expect(accountFailure({ error: new ClaudeAccountFailed("login", "gone") })).toBe("login");
    expect(accountFailure({ output: { stdout: "", stderr: "Please run /login to continue", exitCode: 1 } })).toBe("login");
  });

  test("the service's own, which no other login would answer either", () => {
    expect(accountFailure({ error: new ClaudeOutageError(529, 3) })).toBe("outage");
  });

  test("and the work's own, which belongs to nobody else", () => {
    expect(accountFailure({ error: new Error("the tool refused") })).toBeNull();
    expect(accountFailure({ output: { stdout: "", stderr: "syntax error in the prompt file", exitCode: 1 } })).toBeNull();
    // A run that answered says nothing about a login.
    expect(accountFailure({ output: { stdout: "fine", stderr: "", exitCode: 0 } })).toBeNull();
  });

  test("the wording a login being gone is recognised by stands in one place", () => {
    expect(loginGoneWording.test("Invalid API key · Please run /login")).toBe(true);
    expect(loginGoneWording.test("OAuth token has expired")).toBe(true);
    expect(loginGoneWording.test("the file could not be read")).toBe(false);
  });
});

describe("one run carried by the logins", () => {
  test("a login at its ceiling is passed over, and the next one's answer is the caller's", async () => {
    const { archive, second } = await twoAccounts();
    const asked: string[] = [];

    const answer = await onClaudeAccounts({ archive, config }, "bmo", run(), async (attempt) => {
      asked.push(attempt.account?.id ?? "none");

      if (attempt.account?.id === SEED_ACCOUNT_ID) {
        throw new ClaudeLimitError(hour);
      }

      return "an answer";
    });

    expect(answer).toBe("an answer");
    expect(asked).toEqual([SEED_ACCOUNT_ID, second]);
    // The login that was passed over rests until its own window resets, so nothing asks it again.
    expect(await isAccountResting(archive, SEED_ACCOUNT_ID)).toBe(true);
    expect(await accountRestUntil(archive, SEED_ACCOUNT_ID)).toBe(hour);
    expect(await isAccountResting(archive, second)).toBe(false);
  });

  test("a login that is gone is passed over too", async () => {
    const { archive, second } = await twoAccounts();
    const asked: string[] = [];

    const answer = await onClaudeAccounts({ archive, config }, "bmo", run(), async (attempt) => {
      asked.push(attempt.account?.id ?? "none");

      if (attempt.account?.id === SEED_ACCOUNT_ID) {
        throw new ClaudeAccountFailed("login", "that login has ended");
      }

      return "an answer";
    });

    expect(answer).toBe("an answer");
    expect(asked).toEqual([SEED_ACCOUNT_ID, second]);
    // A login waiting for a person is not put to rest: its window is not what is full.
    expect(await isAccountResting(archive, SEED_ACCOUNT_ID)).toBe(false);
  });

  // What the caller reads when no login is left is what the last login threw, which is what has
  // always sent it down the provider order.
  test("a run no login answered hands the last failure back", async () => {
    const { archive } = await twoAccounts();
    let tried = 0;

    await expect(
      onClaudeAccounts({ archive, config }, "bmo", run(), async () => {
        tried += 1;

        throw new ClaudeLimitError(hour);
      }),
    ).rejects.toThrow(ClaudeLimitError);

    expect(tried).toBe(2);
  });

  test("the service being down is not carried to another login", async () => {
    const { archive } = await twoAccounts();
    let tried = 0;

    await expect(
      onClaudeAccounts({ archive, config }, "bmo", run(), async () => {
        tried += 1;

        throw new ClaudeOutageError(529, 3);
      }),
    ).rejects.toThrow(ClaudeOutageError);

    expect(tried).toBe(1);
  });

  test("the work's own failure is not carried either", async () => {
    const { archive } = await twoAccounts();
    let tried = 0;

    await expect(
      onClaudeAccounts({ archive, config }, "bmo", run(), async () => {
        tried += 1;

        throw new Error("the work itself broke");
      }),
    ).rejects.toThrow("the work itself broke");

    expect(tried).toBe(1);
  });

  test("with no store in hand the run is made once, as every run was made before", async () => {
    const asked: (string | null)[] = [];

    const answer = await onClaudeAccounts({ archive: null }, "bmo", run(), async (attempt) => {
      asked.push(attempt.account?.id ?? null);

      return "an answer";
    });

    expect(answer).toBe("an answer");
    expect(asked).toEqual([null]);
  });
});
