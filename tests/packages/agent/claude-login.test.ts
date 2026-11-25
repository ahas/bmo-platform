import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { mkdirSync, writeFileSync, utimesSync } from "node:fs";
import { join } from "node:path";

import { Archive } from "@agency/archive/archive";
import { subscribeAgencyEvents } from "@agency/agent/events";
import { addNamedClaudeAccount, SEED_ACCOUNT_ID } from "@agency/agent/claude-account";
import { claudeAccountOpen, claudeAccountWalk } from "@agency/agent/claude-dispatch";
import { clearLoginIfBack, credentialExpiry, loginCommand, loginEnd, loginStanding, markLoginGone, noticeAheadMs, tellAboutLogin, tokenLifeMs, withRenewedLogin } from "@agency/agent/claude-login";
import { scratchDir } from "@agency/test-support/scratch";

import { scratchArchive, scratchDatabase } from "@agency/test-support/store";

// A login and its ending. What is read out of a credential is when the access token it holds
// stops and whether it holds anything to renew with, and nothing else; what those two answer is
// whether a person is needed at all — a login the CLI renews for itself never needs one — and what
// is done about the ones that do is one renewal rather than several, a mark the walk obeys, and
// one message to the owner before the end rather than one per run.

const root = scratchDir("bmo-login-");
const token = "sk-ant-oat01-never-in-a-report";

let made = 0;

// One login's own configuration directory, with the credential the CLI would have written in it.
function loginDir(expiresAt: number | null): string {
  made += 1;

  const dir = join(root, `login-${made}`);

  mkdirSync(dir, { recursive: true });
  writeFileSync(
    join(dir, ".credentials.json"),
    JSON.stringify({ claudeAiOauth: { accessToken: token, refreshToken: `${token}-refresh`, expiresAt } }),
  );

  return dir;
}

// A login whose file states a moment and holds nothing to renew itself with — a login that really
// does end when that moment comes.
function loginDirWithoutRefresh(expiresAt: number): string {
  made += 1;

  const dir = join(root, `login-${made}`);

  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, ".credentials.json"), JSON.stringify({ claudeAiOauth: { accessToken: token, expiresAt } }));

  return dir;
}

function tokenFile(writtenAt: number): string {
  made += 1;

  const path = join(root, `token-${made}.txt`);

  writeFileSync(path, `${token}\n`);
  utimesSync(path, new Date(writtenAt), new Date(writtenAt));

  return path;
}

describe("when a login ends, read off its own credential", () => {
  test("the moment the credential states, and nothing else out of the file", () => {
    const ends = Date.now() + 60 * 60_000;

    expect(credentialExpiry({ kind: "directory", path: loginDir(ends) })).toBe(ends);
  });

  test("a moment stated in seconds is read as the moment it is", () => {
    const seconds = Math.floor((Date.now() + 7_200_000) / 1000);
    const dir = loginDir(null);

    writeFileSync(join(dir, ".credentials.json"), JSON.stringify({ oauth: { expires_at: seconds, token } }));

    expect(credentialExpiry({ kind: "directory", path: dir })).toBe(seconds * 1000);
  });

  test("a token says nothing about its own end, so its year is counted from the day it was written", () => {
    const written = Date.now() - 300 * 24 * 60 * 60_000;
    const path = tokenFile(written);
    const expiry = credentialExpiry({ kind: "token", path });

    expect(expiry).not.toBeNull();
    expect(Math.abs((expiry as number) - (written + tokenLifeMs))).toBeLessThan(2_000);
  });

  test("a credential nothing can read is a login whose end is unknown, not one that has ended", () => {
    expect(credentialExpiry({ kind: "directory", path: join(root, "nowhere") })).toBeNull();
    expect(loginStanding({
      id: "x",
      name: "x",
      place: 1,
      enabled: true,
      credentialKind: "directory",
      credentialPath: join(root, "nowhere"),
      endpointUrl: null,
      keyFile: null,
      endpointModels: null,
      basePrompt: null,
      addedAt: 1,
      needsLoginAt: null,
      userId: null,
    })).toBe("standing");
  });

  // The whole of why the two readings are kept apart: a subscription login's file states an hour
  // from now at any moment of its life, and the CLI renews it out of the refresh beside it. Read as
  // an ending, every such login would be called gone within the hour, every hour.
  test("a login that holds something to renew with has no end anybody here can read", () => {
    const soon = loginDir(Date.now() + 8 * 60_000);
    const lapsed = loginDir(Date.now() - 6 * 60 * 60_000);

    expect(credentialExpiry({ kind: "directory", path: soon })).toBeGreaterThan(Date.now());
    expect(loginEnd({ kind: "directory", path: soon })).toBeNull();
    // Even one whose access token lapsed hours ago: the next run renews it, and nothing here knows
    // better than that run does.
    expect(loginEnd({ kind: "directory", path: lapsed })).toBeNull();
  });

  test("one that holds nothing to renew with ends when its own moment says", () => {
    const ends = Date.now() + 90 * 60_000;

    expect(loginEnd({ kind: "directory", path: loginDirWithoutRefresh(ends) })).toBe(ends);
  });

  test("a token's end is its year, since a token holds no refresh at all", () => {
    const written = Date.now() - 300 * 24 * 60 * 60_000;
    const ends = loginEnd({ kind: "token", path: tokenFile(written) });

    expect(ends).not.toBeNull();
    expect(Math.abs((ends as number) - (written + tokenLifeMs))).toBeLessThan(2_000);
  });

  test("standing, ending and gone are the three a login's own end decides", () => {
    const row = (path: string) => ({
      id: "x",
      name: "x",
      place: 1,
      enabled: true,
      credentialKind: "directory" as const,
      credentialPath: path,
      endpointUrl: null,
      keyFile: null,
      endpointModels: null,
      basePrompt: null,
      addedAt: 1,
      needsLoginAt: null,
      userId: null,
    });

    expect(loginStanding(row(loginDirWithoutRefresh(Date.now() + 30 * 24 * 60 * 60_000)))).toBe("standing");
    expect(loginStanding(row(loginDirWithoutRefresh(Date.now() + noticeAheadMs - 60_000)))).toBe("ending");
    expect(loginStanding(row(loginDirWithoutRefresh(Date.now() - 60_000)))).toBe("gone");
    // And a login that renews itself stands whatever hour its access token is in.
    expect(loginStanding(row(loginDir(Date.now() - 6 * 60 * 60_000)))).toBe("standing");
  });
});

describe("a login that is gone", () => {
  async function twoAccounts(): Promise<{ archive: Archive; second: string }> {
    const archive = await scratchArchive();
    const second = await addNamedClaudeAccount(archive, {
      name: "Second account",
      credential: { kind: "directory", path: loginDir(Date.now() + 30 * 24 * 60 * 60_000) },
    });

    return { archive, second: second.id };
  }

  test("is passed over by the walk, and its work goes to the login that is left", async () => {
    const { archive, second } = await twoAccounts();

    expect((await claudeAccountWalk(archive)).map((one) => one.id)).toEqual([SEED_ACCOUNT_ID, second]);

    await markLoginGone(archive, SEED_ACCOUNT_ID);

    expect(await claudeAccountOpen(archive, SEED_ACCOUNT_ID)).toBe(false);
    expect((await claudeAccountWalk(archive)).map((one) => one.id)).toEqual([second]);
  });

  test("is offered again by the walk itself the moment its credential says somebody signed in", async () => {
    const archive = await scratchArchive();
    const dir = loginDir(Date.now() - 60_000);
    const gone = await addNamedClaudeAccount(archive, { name: "Second account", credential: { kind: "directory", path: dir } });

    await markLoginGone(archive, gone.id);

    // While its own credential says the same thing the mark stands, and the walk passes it over.
    expect(await clearLoginIfBack(archive, gone.id)).toBe(false);
    expect(await claudeAccountOpen(archive, gone.id)).toBe(false);
    expect((await claudeAccountWalk(archive)).map((one) => one.id)).toEqual([SEED_ACCOUNT_ID]);

    // Signing in again writes a moment in the future, and nothing else has to be told: the walk's
    // own reading of that credential lifts the mark.
    writeFileSync(
      join(dir, ".credentials.json"),
      JSON.stringify({ claudeAiOauth: { accessToken: token, expiresAt: Date.now() + 30 * 24 * 60 * 60_000 } }),
    );

    expect((await claudeAccountWalk(archive)).map((one) => one.id)).toEqual([SEED_ACCOUNT_ID, gone.id]);
    expect((await archive.claudeAccounts().getClaudeAccount(gone.id))?.needsLoginAt).toBeNull();
  });
});

// An endpoint account holds no login: its key is the whole credential, states no end and carries no
// refresh. So it is never ending, never warned about, and the only thing that says it stands again
// after a refusal is its key file written after the mark (docs/cache-fallback.md, 「A login that is
// about to end」).
describe("an endpoint account's login", () => {
  function endpoint(path: string, keyFile: string) {
    return {
      kind: "endpoint" as const,
      path,
      endpointUrl: "https://api.deepseek.com/anthropic",
      keyFile,
      models: { opus: "deepseek-v4-pro" },
    };
  }

  test("states no ending, and a run that wrote a fresh key file after its mark lifts it", async () => {
    made += 1;

    const archive = await scratchArchive();
    const home = join(root, `endpoint-home-${made}`);
    const keyFile = join(root, `endpoint-key-${made}`);

    mkdirSync(home, { recursive: true });
    writeFileSync(keyFile, "sk-not-a-real-key\n");

    const added = await addNamedClaudeAccount(archive, { name: "DeepSeek", credential: endpoint(home, keyFile) });

    expect(loginEnd(added.credential)).toBeNull();
    expect(credentialExpiry(added.credential)).toBeNull();

    await markLoginGone(archive, added.id);

    const marked = (await archive.claudeAccounts().getClaudeAccount(added.id))?.needsLoginAt as number;

    // A key file older than the mark says nothing: it is the key that was refused.
    utimesSync(keyFile, new Date(marked * 1000 - 60_000), new Date(marked * 1000 - 60_000));
    expect(await clearLoginIfBack(archive, added.id)).toBe(false);

    // An empty file is no key at all, however recently it was written.
    writeFileSync(keyFile, "");
    utimesSync(keyFile, new Date(marked * 1000 + 60_000), new Date(marked * 1000 + 60_000));
    expect(await clearLoginIfBack(archive, added.id)).toBe(false);

    // A key put there after the mark is the owner's word that the account stands again.
    writeFileSync(keyFile, "sk-a-fresh-key\n");
    utimesSync(keyFile, new Date(marked * 1000 + 60_000), new Date(marked * 1000 + 60_000));
    expect(await clearLoginIfBack(archive, added.id)).toBe(true);
    expect((await archive.claudeAccounts().getClaudeAccount(added.id))?.needsLoginAt).toBeNull();
  });
});

describe("what the owner is told, and how often", () => {
  const sent: string[] = [];

  // A login's end is published as `account-login-notice`, which the consumer holding the owner's chat
  // says there; what the stream carries is caught here, the notice's words.
  let unsubscribe: (() => void) | null = null;

  function catching(): void {
    unsubscribe?.();
    sent.length = 0;
    unsubscribe = subscribeAgencyEvents((event) => {
      if (event.name === "account-login-notice") sent.push(event.body.notice);
    });
  }

  test("one message for one ending, not one per run, and the command is in it", async () => {
    const archive = await scratchArchive();

    catching();

    const dir = loginDirWithoutRefresh(Date.now() + 60 * 60_000);
    const account = await addNamedClaudeAccount(archive, { name: "Second account", credential: { kind: "directory", path: dir } });
    const held = await archive.claudeAccounts().getClaudeAccount(account.id);

    expect(await tellAboutLogin({ archive }, held!)).toBe(true);
    expect(sent).toHaveLength(1);
    expect(sent[0]).toContain("Second account");
    expect(sent[0]).toContain(loginCommand(held!));
    // What is in the message is the path and the command. The credential itself never is.
    expect(sent[0]).not.toContain(token);

    // Every run afterwards reads the same ending and says nothing more about it.
    expect(await tellAboutLogin({ archive }, held!)).toBe(false);
    expect(await tellAboutLogin({ archive }, held!)).toBe(false);
    expect(sent).toHaveLength(1);

    // A login signed in again ends at another moment, and that ending is its own to warn about.
    writeFileSync(
      join(dir, ".credentials.json"),
      JSON.stringify({ claudeAiOauth: { accessToken: token, expiresAt: Date.now() + 90 * 60_000 } }),
    );

    expect(await tellAboutLogin({ archive }, (await archive.claudeAccounts().getClaudeAccount(account.id))!)).toBe(true);
    expect(sent).toHaveLength(2);
  });

  test("a login far from its end is not mentioned at all", async () => {
    const archive = await scratchArchive();

    catching();

    const account = await addNamedClaudeAccount(archive, {
      name: "Second account",
      credential: { kind: "directory", path: loginDirWithoutRefresh(Date.now() + 30 * 24 * 60 * 60_000) },
    });

    expect(await tellAboutLogin({ archive }, (await archive.claudeAccounts().getClaudeAccount(account.id))!)).toBe(false);
    expect(sent).toEqual([]);
  });

  test("an endpoint account is never warned about, marked though it is", async () => {
    made += 1;

    const archive = await scratchArchive();
    const home = join(root, `notice-home-${made}`);
    const keyFile = join(root, `notice-key-${made}`);

    mkdirSync(home, { recursive: true });
    writeFileSync(keyFile, "sk-not-a-real-key\n");

    catching();

    const account = await addNamedClaudeAccount(archive, {
      name: "DeepSeek",
      credential: { kind: "endpoint", path: home, endpointUrl: "https://api.deepseek.com/anthropic", keyFile, models: {} },
    });

    await markLoginGone(archive, account.id);

    expect(await tellAboutLogin({ archive }, (await archive.claudeAccounts().getClaudeAccount(account.id))!)).toBe(false);
    expect(sent).toEqual([]);
  });
});

// The renewal is one run's, and the others use what it wrote. What stands in for the CLI's own
// renewal here is the first run writing a further moment into the credential, which is exactly
// what the CLI does as it starts.
describe("one renewal, not several", () => {
  test("a login nowhere near its end holds nothing at all", async () => {
    const archive = await scratchArchive();
    const credential = { kind: "directory", path: loginDir(Date.now() + 30 * 24 * 60 * 60_000) };
    let ran = 0;

    await withRenewedLogin({ archive }, { id: "far", credential }, async () => {
      ran += 1;

      return null;
    });

    expect(ran).toBe(1);
  });

  test("a second run waits for the first and then uses what it wrote", async () => {
    const archive = await scratchArchive();
    const dir = loginDir(Date.now() + 5 * 60_000);
    const credential = { kind: "directory", path: dir };
    const account = { id: "renewing", credential };
    const order: string[] = [];
    const renew = (): void => {
      writeFileSync(
        join(dir, ".credentials.json"),
        JSON.stringify({ claudeAiOauth: { accessToken: token, expiresAt: Date.now() + 8 * 60 * 60_000 } }),
      );
    };

    const first = withRenewedLogin({ archive }, account, async () => {
      order.push("first in");
      // The CLI's own renewal, as it happens while the run starts.
      renew();
      await Bun.sleep(300);
      order.push("first out");

      return credentialExpiry(credential);
    });
    const second = withRenewedLogin({ archive }, account, async () => {
      order.push("second in");

      return credentialExpiry(credential);
    });
    const [firstSaw, secondSaw] = await Promise.all([first, second]);

    // The second run did not start until the first had let the lock go.
    expect(order).toEqual(["first in", "first out", "second in"]);
    // And what it went out on is the moment the first one's renewal wrote.
    expect(secondSaw).toBe(firstSaw);
    expect((secondSaw as number) - Date.now()).toBeGreaterThan(60 * 60_000);
  });
});
