// The archive's `settings` table, read and written here and nowhere else in the archive
// (docs/store.md).

import { takeLockTally, keptThroughLock } from "./archive-lock";
import type { LockTally } from "./archive-lock";
import type { ArchiveSql } from "./archive-sql";
import { mapInOrder } from "@agency/support/in-order";

// Where the count of met write locks is kept in the settings, one row a day under it.
const lockWaitsKey = "lock_waits:";

export type BotMode = "owner_only" | "everyone";

interface SettingRow {
  value: string;
}

interface SettingKeyRow {
  key: string;
}

const modeSettingKey = "mode";
// The switch for the whole voice briefing: `off` refuses every briefing, and anything else — the
// row missing included — leaves it on.
const voiceBriefingKey = "voice_briefing";
// Whether a listener plays a briefing by itself: `off` leaves it to a press, and anything else — the
// row missing included — plays it.
const voiceAutoplayKey = "voice_briefing_autoplay";

const defaultMode: BotMode = "owner_only";

function hardwareTrackingKey(machine: string): string {
  return `hardware_tracking:${machine}`;
}

function hardwareCpuLimitKey(machine: string): string {
  return `hardware_cpu_limit:${machine}`;
}

function agentGateKey(agent: string, gate: string): string {
  return `agent_gate:${agent}:${gate}`;
}

function agentSamplingKey(agent: string): string {
  return `agent_sampling:${agent}`;
}

// The settings a stored sampling holds: every entry whose value is a finite number or a switch, and
// nothing of a text that is no such object.
function samplingOf(text: string): Record<string, number | boolean> {
  try {
    const read: unknown = JSON.parse(text);

    return typeof read === "object" && read !== null && !Array.isArray(read)
      ? Object.fromEntries(Object.entries(read).filter((entry): entry is [string, number | boolean] => (typeof entry[1] === "number" && Number.isFinite(entry[1])) || typeof entry[1] === "boolean"))
      : {};
  } catch {
    return {};
  }
}

export class SettingStore {
  constructor(private readonly sql: ArchiveSql) {}

  async getSetting(key: string): Promise<string | null> {
    const row = await this.sql.get("SELECT value FROM settings WHERE key = ?", key) as SettingRow | null;

    return row?.value ?? null;
  }

  /**
   * What this process has met of the write lock, added to the day's running count and taken off
   * the process's own tally. Without it the lock is only ever known by what it broke: the writes
   * the retries carried through leave no trace at all, so a day of constant contention that cost
   * nothing reads exactly like a quiet one, and how often it happens stays a feeling.
   *
   * One row per day, so the counts can be read against the days the work was heavy.
   * A row this cannot write is a row about the lock lost to the lock,
   * which is not worth holding anything up for.
   */
  async recordLockWaits(at: number = Date.now()): Promise<LockTally | null> {
    const held = takeLockTally();

    if (held.met === 0) return null;

    const key = `${lockWaitsKey}${new Date(at).toISOString().slice(0, 10)}`;

    return await keptThroughLock("the day's lock waits", () => this.sql.wrote(async () => {
      const stored = await this.getSetting(key);
      const was = stored === null ? null : JSON.parse(stored) as LockTally;
      const day: LockTally = {
        met: (was?.met ?? 0) + held.met,
        saved: (was?.saved ?? 0) + held.saved,
        waitedMs: (was?.waitedMs ?? 0) + held.waitedMs,
        longestMs: Math.max(was?.longestMs ?? 0, held.longestMs),
      };

      await this.setSetting(key, JSON.stringify(day));

      return day;
    }));
  }

  /** The days the lock has been met on, newest first, as they were recorded. */
  async lockWaits(): Promise<{ day: string; tally: LockTally }[]> {
    return (await mapInOrder((await this.settingKeys(lockWaitsKey)), async (key) => ({ day: key.slice(lockWaitsKey.length), tally: JSON.parse(await this.getSetting(key) ?? "{}") as LockTally })))
      .sort((one, other) => other.day.localeCompare(one.day));
  }

  async setSetting(key: string, value: string): Promise<void> {
    await this.sql.run("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value", key, value);
  }

  async deleteSetting(key: string): Promise<void> {
    await this.sql.run("DELETE FROM settings WHERE key = ?", key);
  }

  // Every key under one prefix, for a family of rows a sweep has to read as a whole. The escape
  // is named because a prefix of agency's own carries no wildcard and a LIKE pattern that met one
  // would quietly widen the sweep.
  async settingKeys(prefix: string): Promise<string[]> {
    return (await this.sql.all("SELECT key FROM settings WHERE key LIKE ?1 || '%' ESCAPE '\\' ORDER BY key", prefix.replace(/[\\%_]/g, "\\$&")) as SettingKeyRow[]).map((row) => row.key);
  }

  async getMode(): Promise<BotMode> {
    const row = await this.sql.get("SELECT value FROM settings WHERE key = ?", modeSettingKey) as SettingRow | null;

    if (row === null || (row.value !== "owner_only" && row.value !== "everyone")) {
      return defaultMode;
    }

    return row.value;
  }

  async voiceBriefingOn(): Promise<boolean> {
    return await this.getSetting(voiceBriefingKey) !== "off";
  }

  async setVoiceBriefing(on: boolean): Promise<void> {
    await this.setSetting(voiceBriefingKey, on ? "on" : "off");
  }

  async voiceAutoplayOn(): Promise<boolean> {
    return await this.getSetting(voiceAutoplayKey) !== "off";
  }

  async setVoiceAutoplay(on: boolean): Promise<void> {
    await this.setSetting(voiceAutoplayKey, on ? "on" : "off");
  }

  // A machine's live tracking, under the name the hardware monitor knows it by: `off` stops its reading,
  // and anything else — the row missing included — leaves it read.
  async hardwareTrackingOn(machine: string): Promise<boolean> {
    return await this.getSetting(hardwareTrackingKey(machine)) !== "off";
  }

  async setHardwareTracking(machine: string, on: boolean): Promise<void> {
    await this.setSetting(hardwareTrackingKey(machine), on ? "on" : "off");
  }

  // The CPU limit set for a machine, in degrees, which stands over the one its chip names; null where
  // none is set. The control port writes it.
  async hardwareCpuLimit(machine: string): Promise<number | null> {
    const value = Number.parseFloat(await this.getSetting(hardwareCpuLimitKey(machine)) ?? "");

    return Number.isFinite(value) ? value : null;
  }

  // An agent's own switch for one gate its turns pass:
  // `off` keeps the gate out of that agent's turns, and anything else — the row missing included —
  // leaves it in.
  async agentGateOn(agent: string, gate: string): Promise<boolean> {
    return await this.getSetting(agentGateKey(agent, gate)) !== "off";
  }

  async setAgentGate(agent: string, gate: string, on: boolean): Promise<void> {
    await this.setSetting(agentGateKey(agent, gate), on ? "on" : "off");
  }

  // A switch its deployment starts the agent with, written only where none stands, so the owner's
  // own choice made since is never undone.
  async seedAgentGate(agent: string, gate: string, on: boolean): Promise<void> {
    await this.sql.run("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT DO NOTHING", agentGateKey(agent, gate), on ? "on" : "off");
  }

  // A worker's own sampling (docs/own-llm.md, 「A worker pinned to a provider」): each setting by the
  // name the engine reads it under, with its number or its switch; null where it was never written,
  // which a seed may still fill, and an empty object where every one is left to the engine.
  async agentSampling(agent: string): Promise<Record<string, number | boolean> | null> {
    const text = await this.getSetting(agentSamplingKey(agent));

    return text === null ? null : samplingOf(text);
  }

  async setAgentSampling(agent: string, sampling: Readonly<Record<string, number | boolean>>): Promise<void> {
    await this.setSetting(agentSamplingKey(agent), JSON.stringify(sampling));
  }

  // The sampling its deployment starts the worker with, written only where none was ever written, so
  // the owner's own since — an emptied one included — is never undone.
  async seedAgentSampling(agent: string, sampling: Readonly<Record<string, number | boolean>>): Promise<void> {
    await this.sql.run("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT DO NOTHING", agentSamplingKey(agent), JSON.stringify(sampling));
  }

  async setMode(mode: BotMode): Promise<void> {
    await this.sql.run("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value", modeSettingKey, mode);
  }
}
