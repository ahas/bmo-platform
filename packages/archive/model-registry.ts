import declaredRegistry from "@agency/policy/registry.toml";

import type { Database } from "bun:sqlite";
import type { ArchiveSql } from "./archive-sql";

// The models the owner registers for the chat and the workers to run on (docs/own-llm.md,
// 「Registered models」): each a name, a label, the model its addresses serve, its sizes, its wait,
// its thinking default, whether the automatic walk asks it and whether it keeps its research — a flag
// the caller that sends research out reads — with one or more addresses, each a node
// the same model is served on. A row holds where a key is kept and never a key. What the rows are
// read for — the provider that offers them and the gateway that serves them — is the agent base's;
// this store keeps them and holds them to their shape.

export interface RegisteredAddress {
  // The node's label, unique within its registration.
  node: string;
  // The OpenAI-compatible chat completions address.
  url: string;
  // The key's file under the data root's `env/`, or null where the address takes no key.
  keyFile: string | null;
  // Whether agency wrote that file, and so may take it away.
  keyOwned: boolean;
  // When the address was last given a new session, in seconds; null where it never was.
  placedAt: number | null;
}

export interface Registration {
  name: string;
  label: string;
  model: string;
  contextTokens: number;
  answerTokens: number;
  thinking: boolean;
  walked: boolean;
  // Whether it keeps its research: a flag the caller that sends a search or a browse out reads.
  keepsResearch: boolean;
  addresses: RegisteredAddress[];
}

// What a registration is made with, and what a change may name.
export type RegistrationFields = Omit<Registration, "name" | "addresses">;
export type RegistrationChange = Partial<RegistrationFields>;
export type AddressInput = Omit<RegisteredAddress, "placedAt">;

// A registration the deployment starts an archive with, its window taken from the setting it held
// before there was a registry where that holds one the registration takes.
export interface RegistrationSeed extends Omit<Registration, "addresses"> {
  addresses: AddressInput[];
  formerWindowSetting?: string;
}

// The bounds of the sizes, as the shared policy declares them.
export const registrationBounds: Record<"contextTokens" | "answerTokens", { min: number; max: number }> = {
  contextTokens: declaredRegistry.context_tokens,
  answerTokens: declaredRegistry.answer_tokens,
};

// A refusal the owner reads: the field it is about, and why.
export class RegistrationRefused extends Error {
  constructor(readonly field: string, message: string) {
    super(message);
  }
}

// A name a registration, and a label a node, may take; a model as a command line and a tier mapping
// take it; a key's file under `env/`; and how long a label may be: the shared policy's.
const namePattern = new RegExp(declaredRegistry.name_pattern as string);
const modelPattern = new RegExp(declaredRegistry.model_pattern as string);
const keyFilePattern = new RegExp(declaredRegistry.key_file_pattern as string);
const labelMost: number = declaredRegistry.label_most;

const count = (value: number) => value.toLocaleString("en-US");

function whole(field: keyof typeof registrationBounds, value: unknown, label: string): number {
  const { min, max } = registrationBounds[field];

  if (typeof value !== "number" || !Number.isInteger(value) || value < min || value > max) {
    throw new RegistrationRefused(field, `${label} is a whole number from ${count(min)} to ${count(max)}.`);
  }

  return value;
}

export function checkRegistrationName(name: unknown): string {
  if (typeof name !== "string" || !namePattern.test(name)) {
    throw new RegistrationRefused("name", "A name opens with a lowercase letter and holds only lowercase letters, digits and hyphens, at most 40 of them.");
  }

  return name;
}

export function checkNodeLabel(node: unknown): string {
  if (typeof node !== "string" || !namePattern.test(node)) {
    throw new RegistrationRefused("node", "A node's name opens with a lowercase letter and holds only lowercase letters, digits and hyphens, at most 40 of them.");
  }

  return node;
}

// The address, read by the URL parser the runtime carries: an http or https address whose path ends
// in the chat completions path.
export function checkAddressUrl(url: unknown): string {
  let parsed: URL;

  try {
    parsed = new URL(String(url));
  } catch {
    throw new RegistrationRefused("url", "The address cannot be read.");
  }

  if ((parsed.protocol !== "http:" && parsed.protocol !== "https:") || !parsed.pathname.endsWith("/chat/completions") || parsed.search !== "" || parsed.hash !== "" || parsed.username !== "" || parsed.password !== "") {
    throw new RegistrationRefused("url", "The address is an http(s) address ending in /chat/completions.");
  }

  return parsed.href;
}

function checkKeyFile(keyFile: unknown): string | null {
  if (keyFile === null) return null;
  if (typeof keyFile !== "string" || !keyFilePattern.test(keyFile) || keyFile.split("/").some((part) => part === "." || part === "..")) {
    throw new RegistrationRefused("key", "The key file's name cannot be read.");
  }

  return keyFile;
}

// Every field a registration holds apart from its name and its addresses, checked together, since
// the answer size is held to the window.
export function checkRegistrationFields(fields: RegistrationFields): RegistrationFields {
  if (typeof fields.label !== "string" || fields.label.trim() === "" || fields.label.length > labelMost) {
    throw new RegistrationRefused("label", `A label is 1 to ${labelMost} characters.`);
  }

  if (typeof fields.model !== "string" || !modelPattern.test(fields.model)) {
    throw new RegistrationRefused("model", "The model's name cannot be read.");
  }

  const contextTokens = whole("contextTokens", fields.contextTokens, "The context window");
  const answerTokens = whole("answerTokens", fields.answerTokens, "The answer size");

  if (answerTokens >= contextTokens) {
    throw new RegistrationRefused("answerTokens", "The answer size is smaller than the context window.");
  }

  if (typeof fields.thinking !== "boolean" || typeof fields.walked !== "boolean") {
    throw new RegistrationRefused("thinking", "The thinking default and the automatic walk are each on or off.");
  }

  if (typeof fields.keepsResearch !== "boolean") {
    throw new RegistrationRefused("keepsResearch", "Research on this model is on or off.");
  }

  return {
    label: fields.label.trim(),
    model: fields.model,
    contextTokens,
    answerTokens,
    thinking: fields.thinking,
    walked: fields.walked,
    keepsResearch: fields.keepsResearch,
  };
}

interface RegistrationRow {
  name: string;
  label: string;
  model: string;
  contextTokens: number;
  answerTokens: number;
  thinking: number;
  walked: number;
  keepsResearch: number;
}

interface AddressRow {
  registration: string;
  node: string;
  url: string;
  keyFile: string | null;
  keyOwned: number;
  placedAt: number | null;
}

/**
 * The registry's tables made over an archive being settled and, the first time, its seeds given.
 * Only the archive's settling calls it; an ordinary open reads the tables as they stand
 * (docs/store.md).
 */
export function settleModelRegistry(db: Database, seeds: readonly RegistrationSeed[], now: number = Date.now()): void {
  db.transaction(() => {
    const made = db.query("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'registered_models'").get() !== null;

    db.run(`
      CREATE TABLE IF NOT EXISTS registered_models (
        name TEXT PRIMARY KEY,
        label TEXT NOT NULL,
        model TEXT NOT NULL,
        context_tokens INTEGER NOT NULL,
        answer_tokens INTEGER NOT NULL,
        thinking INTEGER NOT NULL,
        walked INTEGER NOT NULL,
        added_at INTEGER NOT NULL,
        keeps_research INTEGER NOT NULL DEFAULT 0
      )
    `);

    // A registry made before registrations kept their research gains the column, and each seed that
    // keeps its research is given it once, on the registration of its name where it still stands.
    const columns = db.query("PRAGMA table_info(registered_models)").all() as { name: string }[];

    if (!columns.some((column) => column.name === "keeps_research")) {
      db.run("ALTER TABLE registered_models ADD COLUMN keeps_research INTEGER NOT NULL DEFAULT 0");

      for (const seed of seeds) {
        if (seed.keepsResearch) db.query("UPDATE registered_models SET keeps_research = 1 WHERE name = ?").run(seed.name);
      }
    }
    db.run(`
      CREATE TABLE IF NOT EXISTS registered_model_addresses (
        registration TEXT NOT NULL,
        node TEXT NOT NULL,
        position INTEGER NOT NULL,
        url TEXT NOT NULL,
        key_file TEXT,
        key_owned INTEGER NOT NULL DEFAULT 0,
        placed_at INTEGER,
        PRIMARY KEY (registration, node)
      )
    `);

    if (made) return;

    // The seeds are given once, as the tables are made, and are the owner's from then on: a seed
    // the owner removes stays removed. The window setting each seed replaces is read and then gone.
    for (const seed of seeds) {
      const former = seed.formerWindowSetting === undefined ? null : (db.query("SELECT value FROM settings WHERE key = ?").get(seed.formerWindowSetting) as { value: string } | null);
      const held = former === null ? Number.NaN : Number(former.value);
      const { min, max } = registrationBounds.contextTokens;
      const contextTokens = Number.isInteger(held) && held >= min && held <= max && held > seed.answerTokens ? held : seed.contextTokens;

      seedRegistration(db, { ...seed, contextTokens }, now);

      for (const address of seed.addresses) seedAddress(db, seed.name, address);

      if (seed.formerWindowSetting !== undefined) db.query("DELETE FROM settings WHERE key = ?").run(seed.formerWindowSetting);
    }
  }).immediate();
}

// A seed's registration, checked and refused as the store's own `create` checks and refuses one.
function seedRegistration(db: Database, registration: Omit<Registration, "addresses">, now: number): void {
  const name = checkRegistrationName(registration.name);
  const fields = checkRegistrationFields(registration);

  if (db.query("SELECT 1 FROM registered_models WHERE name = ?").get(name) !== null) throw new RegistrationRefused("name", `The name is already registered: ${name}`);

  db
    .query(`
      INSERT INTO registered_models (name, label, model, context_tokens, answer_tokens, thinking, walked, keeps_research, added_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    `)
    .run(name, fields.label, fields.model, fields.contextTokens, fields.answerTokens, fields.thinking ? 1 : 0, fields.walked ? 1 : 0, fields.keepsResearch ? 1 : 0, Math.floor(now / 1000));
}

// A seed's address, checked and placed last as the store's own `setAddress` does, on the
// registration the same seed has just written.
function seedAddress(db: Database, name: string, address: AddressInput): void {
  const node = checkNodeLabel(address.node);
  const url = checkAddressUrl(address.url);
  const keyFile = checkKeyFile(address.keyFile);
  const last = db.query("SELECT MAX(position) AS position FROM registered_model_addresses WHERE registration = ?").get(name) as { position: number | null };

  db
    .query(`
      INSERT INTO registered_model_addresses (registration, node, position, url, key_file, key_owned)
      VALUES (?, ?, ?, ?, ?, ?)
      ON CONFLICT (registration, node) DO UPDATE SET url = excluded.url, key_file = excluded.key_file, key_owned = excluded.key_owned
    `)
    .run(name, node, (last.position ?? 0) + 1, url, keyFile, keyFile !== null && address.keyOwned ? 1 : 0);
}

/**
 * The registry. Its tables and its seeds are written only by `settleModelRegistry`, which the
 * archive's settling calls; an ordinary open reads them as they stand (docs/store.md).
 */
export class ModelRegistryStore {
  constructor(private readonly sql: ArchiveSql) {}

  async list(): Promise<Registration[]> {
    const rows = await this.sql.all(`
        SELECT name, label, model, context_tokens AS "contextTokens", answer_tokens AS "answerTokens",
               thinking, walked, keeps_research AS "keepsResearch"
        FROM registered_models ORDER BY added_at, name
      `) as RegistrationRow[];
    const addresses = await this.sql.all(`
        SELECT registration, node, url, key_file AS "keyFile", key_owned AS "keyOwned", placed_at AS "placedAt"
        FROM registered_model_addresses ORDER BY registration, position, node
      `) as AddressRow[];

    return rows.map((row) => ({
      name: row.name,
      label: row.label,
      model: row.model,
      contextTokens: row.contextTokens,
      answerTokens: row.answerTokens,
      thinking: row.thinking === 1,
      walked: row.walked === 1,
      keepsResearch: row.keepsResearch === 1,
      addresses: addresses
        .filter((address) => address.registration === row.name)
        .map((address) => ({ node: address.node, url: address.url, keyFile: address.keyFile, keyOwned: address.keyOwned === 1, placedAt: address.placedAt })),
    }));
  }

  async get(name: string): Promise<Registration | null> {
    return (await this.list()).find((one) => one.name === name) ?? null;
  }

  async create(registration: Omit<Registration, "addresses"> & { addresses?: unknown }, now: number = Date.now()): Promise<void> {
    return await this.sql.wrote(async () => {
      const name = checkRegistrationName(registration.name);
      const fields = checkRegistrationFields(registration);

      if (await this.exists(name)) throw new RegistrationRefused("name", `The name is already registered: ${name}`);

      await this.sql.run(`
          INSERT INTO registered_models (name, label, model, context_tokens, answer_tokens, thinking, walked, keeps_research, added_at)
          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        `, name, fields.label, fields.model, fields.contextTokens, fields.answerTokens, fields.thinking ? 1 : 0, fields.walked ? 1 : 0, fields.keepsResearch ? 1 : 0, Math.floor(now / 1000));
    });
  }

  async change(name: string, change: RegistrationChange): Promise<void> {
    return await this.sql.wrote(async () => {
      const held = await this.required(name);
      const fields = checkRegistrationFields({ ...held, ...change });

      await this.sql.run(`
          UPDATE registered_models
          SET label = ?, model = ?, context_tokens = ?, answer_tokens = ?, thinking = ?, walked = ?, keeps_research = ?
          WHERE name = ?
        `, fields.label, fields.model, fields.contextTokens, fields.answerTokens, fields.thinking ? 1 : 0, fields.walked ? 1 : 0, fields.keepsResearch ? 1 : 0, name);
    });
  }

  // The registration and every address of it. Whether anything still names it is asked by the
  // caller of those that do, since this store knows none of them.
  async remove(name: string): Promise<Registration> {
    const held = await this.required(name);

    await this.sql.transaction(async () => {
      await this.sql.run("DELETE FROM registered_model_addresses WHERE registration = ?", name);
      await this.sql.run("DELETE FROM registered_models WHERE name = ?", name);
    });

    return held;
  }

  // An address added, or one of the same node changed in place.
  async setAddress(name: string, address: AddressInput): Promise<void> {
    return await this.sql.wrote(async () => {
      await this.required(name);
      const node = checkNodeLabel(address.node);
      const url = checkAddressUrl(address.url);
      const keyFile = checkKeyFile(address.keyFile);
      const last = await this.sql.get("SELECT MAX(position) AS position FROM registered_model_addresses WHERE registration = ?", name) as { position: number | null };

      await this.sql.run(`
          INSERT INTO registered_model_addresses (registration, node, position, url, key_file, key_owned)
          VALUES (?, ?, ?, ?, ?, ?)
          ON CONFLICT (registration, node) DO UPDATE SET url = excluded.url, key_file = excluded.key_file, key_owned = excluded.key_owned
        `, name, node, (last.position ?? 0) + 1, url, keyFile, keyFile !== null && address.keyOwned ? 1 : 0);
    });
  }

  async removeAddress(name: string, node: string): Promise<RegisteredAddress> {
    return await this.sql.wrote(async () => {
      const held = (await this.required(name)).addresses.find((one) => one.node === node);

      if (held === undefined) throw new RegistrationRefused("node", `${name} has no address on ${node}.`);

      await this.sql.run("DELETE FROM registered_model_addresses WHERE registration = ? AND node = ?", name, node);

      return held;
    });
  }

  // When an address was last given a new session.
  async markPlaced(name: string, node: string, at: number): Promise<void> {
    await this.sql.run("UPDATE registered_model_addresses SET placed_at = ? WHERE registration = ? AND node = ?", Math.floor(at / 1000), name, node);
  }

  private async exists(name: string): Promise<boolean> {
    return await this.sql.get("SELECT 1 FROM registered_models WHERE name = ?", name) !== null;
  }

  private async required(name: string): Promise<Registration> {
    const held = await this.get(name);

    if (held === null) throw new RegistrationRefused("name", `Not a registered model: ${name}`);

    return held;
  }
}
