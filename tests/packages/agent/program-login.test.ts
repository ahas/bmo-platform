import "@agency/test-support/setup";

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "bun:test";

import { machineConfigDir } from "@agency/agent/claude-account";
import { accountOfLogin, programLogin, tokenFingerprint } from "@agency/agent/program-login";
import { scratchDir } from "@agency/test-support/scratch";

import type { ClaudeAccount } from "@agency/agent/claude-account";

const root = scratchDir("bmo-program-login");
const tokenFile = join(root, "token");
const secret = "sk-ant-oat01-a-token-of-its-own";

mkdirSync(root, { recursive: true });
writeFileSync(tokenFile, `${secret}\n`);

const account = (id: string, kind: "directory" | "token", path: string): ClaudeAccount => ({
  id,
  name: id,
  place: 1,
  enabled: true,
  credential: { kind, path },
  basePrompt: null,
  addedAt: 0,
  userId: null,
});

// An endpoint account's home holds no login, but it is still the account's own: a program running
// under it answers for that account, not for whichever directory the machine happens to keep.
const endpoint = (id: string, path: string): ClaudeAccount => ({
  ...account(id, "directory", path),
  credential: { kind: "endpoint", path, endpointUrl: "https://api.deepseek.com/anthropic", keyFile: "/logins/main/.key", models: { opus: "deepseek-v4-pro" } },
});

const accounts = [
  account("default", "directory", "/home/someone/.claude"),
  account("main", "directory", "/logins/main"),
  account("tok", "token", tokenFile),
  endpoint("deep", "/endpoints/deepseek"),
];

describe("the login a program runs under", () => {
  test("is the configuration home its environment names, or the machine's where it names none", () => {
    expect(programLogin({ CLAUDE_CONFIG_DIR: "/logins/main" })).toEqual({ configDir: "/logins/main", token: null });
    expect(programLogin({})).toEqual({ configDir: machineConfigDir(), token: null });
  });

  test("names a token by a fingerprint that is not the token", () => {
    const login = programLogin({ CLAUDE_CODE_OAUTH_TOKEN: secret });

    expect(login.token).toBe(tokenFingerprint(secret));
    expect(JSON.stringify(login)).not.toContain(secret);
  });
});

describe("the account a login belongs to", () => {
  test("is the directory account whose home it is, however the path is spelled", () => {
    expect(accountOfLogin(accounts, { configDir: "/logins/main/", token: null })?.id).toBe("main");
    expect(accountOfLogin(accounts, { configDir: "/home/someone/.claude", token: null })?.id).toBe("default");
  });

  test("is the endpoint account whose home it is, which holds no login of its own", () => {
    expect(accountOfLogin(accounts, { configDir: "/endpoints/deepseek/", token: null })?.id).toBe("deep");
  });

  test("is the token account holding that token, whatever home the program keeps its records in", () => {
    expect(accountOfLogin(accounts, { configDir: "/home/someone/.claude", token: tokenFingerprint(secret) })?.id).toBe("tok");
  });

  test("is nobody where no registered account holds the login", () => {
    expect(accountOfLogin(accounts, { configDir: "/logins/unknown", token: null })).toBeNull();
    expect(accountOfLogin(accounts, { configDir: "/logins/main", token: tokenFingerprint("another") })).toBeNull();
  });
});
