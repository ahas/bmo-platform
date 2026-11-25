import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { SEED_ACCOUNT_ID } from "@agency/agent/claude-account";
import { accountRestUntil, epochSeconds, fullWindowUntil, isAccountResting, writeRest } from "@agency/agent/rest-state";
import { scratchArchive } from "@agency/test-support/store";

import type { Archive } from "@agency/archive/archive";

// The rest as the TypeScript side judges it reads a login's windows as the Rust side does: a window kept full
// closes the login whether or not a rest was written for it, and a moment written in milliseconds is read as the
// second it names rather than as a rest that never ends.

const now = Date.now();
const nowSeconds = Math.floor(now / 1000);
const hour = nowSeconds + 3600;

async function keepWindow(archive: Archive, window: { utilization: number; resetsAt: number | null; seenAt: number }): Promise<void> {
  await archive.sql.run(
    "INSERT INTO usage_windows (mode, account, name, utilization, resets_at, seen_at) VALUES (?, ?, ?, ?, ?, ?)",
    "claude-code",
    SEED_ACCOUNT_ID,
    "five_hour",
    window.utilization,
    window.resetsAt,
    window.seenAt,
  );
}

describe("a login's windows", () => {
  test("a window kept full closes the login until its reset, with no rest written", async () => {
    const archive = await scratchArchive();

    await keepWindow(archive, { utilization: 1, resetsAt: hour, seenAt: nowSeconds - 60 });

    expect(await fullWindowUntil(archive, SEED_ACCOUNT_ID, now)).toBe(hour);
    expect(await accountRestUntil(archive, SEED_ACCOUNT_ID, now)).toBe(hour);
    expect(await isAccountResting(archive, SEED_ACCOUNT_ID, now)).toBe(true);
  });

  test("a window short of full closes nothing", async () => {
    const archive = await scratchArchive();

    await keepWindow(archive, { utilization: 0.99, resetsAt: hour, seenAt: nowSeconds - 60 });

    expect(await isAccountResting(archive, SEED_ACCOUNT_ID, now)).toBe(false);
  });

  test("a full window that names no reset closes the login only while its reading stands", async () => {
    const fresh = await scratchArchive();

    await keepWindow(fresh, { utilization: 1, resetsAt: null, seenAt: nowSeconds - 60 });
    expect(await isAccountResting(fresh, SEED_ACCOUNT_ID, now)).toBe(true);

    const stale = await scratchArchive();

    await keepWindow(stale, { utilization: 1, resetsAt: null, seenAt: nowSeconds - 86_400 });
    expect(await isAccountResting(stale, SEED_ACCOUNT_ID, now)).toBe(false);
  });

  test("the later of the written rest and the full window stands", async () => {
    const archive = await scratchArchive();

    await writeRest(archive, nowSeconds + 600, "limit", SEED_ACCOUNT_ID);
    await keepWindow(archive, { utilization: 1, resetsAt: hour, seenAt: nowSeconds - 60 });

    expect(await accountRestUntil(archive, SEED_ACCOUNT_ID, now)).toBe(hour);
  });
});

describe("a moment written in milliseconds", () => {
  test("is read as the second it names", () => {
    expect(epochSeconds(hour * 1000)).toBe(hour);
    expect(epochSeconds(hour)).toBe(hour);
  });

  test("a rest written in milliseconds ends when it names, not never", async () => {
    const archive = await scratchArchive();

    await writeRest(archive, hour * 1000, "limit", SEED_ACCOUNT_ID);

    expect(await accountRestUntil(archive, SEED_ACCOUNT_ID, now)).toBe(hour);
  });

  test("a rest kept in milliseconds by an older writer is read in seconds", async () => {
    const archive = await scratchArchive();

    await archive.settings().setSetting(`rest_until:${SEED_ACCOUNT_ID}`, String(hour * 1000));

    expect(await accountRestUntil(archive, SEED_ACCOUNT_ID, now)).toBe(hour);
  });
});
