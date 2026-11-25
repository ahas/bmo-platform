import controlAnswers from "@agency/policy/control-words.toml";
import { fillMarks } from "@agency/support/marks";
import { existsSync, statSync } from "node:fs";
import { isAbsolute, join } from "node:path";

import { machineConfigDir, SEED_ACCOUNT_ID } from "@agency/archive/account-seed";

import type { Archive } from "@agency/archive/archive";
import type { ClaudeAccountRow } from "@agency/archive/claude-account-store";
import type { AgentAccount } from "./harness";

// Agency holds more than one Claude login, and each of them is an account: a name the owner gives it, a
// place in the account order, an on/off, and where its credential is reached. The Claude Code
// entry of the provider order is the list of them, walked in that order, and everything a login
// owns — its sessions, its transcripts, its usage windows, its rest — belongs to the account
// rather than to the road.
//
// **What is kept is where a credential is, never a credential.** A row carries a path and
// nothing else: no token, no cookie and no part of a credential file's body enters the archive,
// a log line, a document or a report. Whoever has to know when a login ends opens the file for
// that one moment and keeps the moment.
//
// The list is data rather than code: the owner adds and removes accounts, so it lives in the
// archive's own table, and this module is what says which names, which ids and which paths are
// allowed into it.

// How a run reaches an account's login.
//
// - `directory` — the account names a configuration home of its own, and the CLI keeps that
//   login in that directory's `.credentials.json`. Two directories are two logins with nothing
//   shared.
// - `token` — the account names a file holding the long-lived token the CLI issues, handed to a
//   run as `CLAUDE_CODE_OAUTH_TOKEN`. Such a token stands for a year, carries no refresh of its
//   own and is inference-only, so it dies rather than lapsing.
// - `endpoint` — the account names a configuration home of its own, an Anthropic-compatible base
//   URL and the file its key sits in, and may map the family words Claude Code picks a model by to
//   the endpoint's own model ids. The home holds no login: the key is the whole credential, read
//   only as a run is launched (docs/cache-fallback.md, 「The Claude Code road is a list of
//   accounts」).
export type ClaudeCredentialKind = "directory" | "token" | "endpoint";

// The family words Claude Code picks a model by, and an endpoint's own model id for each of them
// where the account maps one. A family the map leaves out goes out as Claude Code names it.
export const ENDPOINT_FAMILIES = ["opus", "sonnet", "haiku"] as const;

export type EndpointFamily = (typeof ENDPOINT_FAMILIES)[number];

export type ClaudeEndpointModels = Readonly<Partial<Record<EndpointFamily, string>>>;

export type ClaudeCredential =
  | {
      kind: "directory";
      // The absolute configuration home the login is kept in. Never the credential.
      path: string;
    }
  | {
      kind: "token";
      // The absolute file the token sits in. Never the credential.
      path: string;
    }
  | {
      kind: "endpoint";
      // The absolute configuration home of the account's own, which holds no login.
      path: string;
      endpointUrl: string;
      keyFile: string;
      models: ClaudeEndpointModels;
    };

export interface ClaudeAccount extends AgentAccount {
  // What every record of this account is keyed by, and what never moves: a session, a usage
  // window and a rest all name it. A rename leaves it alone.
  id: string;
  // What the owner calls it, which is what a screen and a report show.
  name: string;
  place: number;
  enabled: boolean;
  credential: ClaudeCredential;
  // The base prompt a worker's run on this account opens on, where it names one; it wins over the
  // agent's own choice (docs/base-prompts.md).
  basePrompt: string | null;
  // When it was registered, which is the order the list falls back to when nothing else says.
  addedAt: number;
  // The user whose account it is, once the multi-user model is in force; null where no user stands.
  userId: number | null;
}

export interface NewClaudeAccount {
  id: string;
  name: string;
  credential: ClaudeCredential;
  enabled?: boolean;
  basePrompt?: string | null;
}

// What an id may be. It stands in a work key's setting, in a CLI flag and in a session record's
// key, so it is the small alphabet all three take without quoting — the same one a worker's name
// is held to, and for the same reason.
export const CLAUDE_ACCOUNT_ID = /^[a-z0-9][a-z0-9._-]{1,31}$/;

export const CLAUDE_ACCOUNT_NAME_MAX = 40;

// The file the CLI keeps a directory account's login in, under that account's own configuration
// directory. Named here because validation and the expiry reading both ask for it; its contents
// are nobody's to keep.
export const CREDENTIAL_FILE = ".credentials.json";

// The one account a fresh archive holds, and the machine's own configuration directory it points
// at, stand with the archive, whose settling writes it (`account-seed.ts`).
export { machineConfigDir, SEED_ACCOUNT_ID, SEED_ACCOUNT_NAME, seedClaudeAccount } from "@agency/archive/account-seed";

/**
 * Which account a run is recorded under: the one it names, or — where it names none — the account
 * that stands for the login this machine already had. A run naming no account goes out on that
 * login, so that is the account its session, its windows and its rest belong to, and a record
 * written before there was more than one login belongs there too.
 */
export function recordedAccount(account: string | { id: string } | null | undefined): string {
  const named = typeof account === "string" ? account : account?.id;

  return named === undefined || named === "" ? SEED_ACCOUNT_ID : named;
}

/**
 * The configuration home of the account's own: a directory account's, which holds its login, and an
 * endpoint account's, which holds no login but keeps the run's records; none for a token account,
 * which goes out under the machine's own.
 */
export function accountHome(credential: { kind: string; path: string } | null | undefined): string | null {
  if (credential === null || credential === undefined) {
    return null;
  }

  return credential.kind === "directory" || credential.kind === "endpoint" ? credential.path : null;
}

/**
 * The directory the CLI keeps an account's own records under — its transcripts above all. It is
 * the account's configuration home where it has one; a token account has none, so the CLI keeps
 * its records where the machine keeps them, which is what the launch hands such a run.
 */
export function accountRecordsDir(credential: { kind: string; path: string } | null | undefined): string {
  return accountHome(credential) ?? machineConfigDir();
}

/**
 * Whether an endpoint account's key is there to be handed to a run: its file present and not empty.
 * The file is never opened — only its size is read — and every other kind holds no key to judge
 * (docs/cache-fallback.md, 「The account judgement」).
 */
export function endpointKeyHeld(credential: { kind: string; keyFile?: string }): boolean {
  if (credential.kind !== "endpoint") {
    return true;
  }

  if (credential.keyFile === undefined || credential.keyFile === "") {
    return false;
  }

  try {
    const held = statSync(credential.keyFile);

    return held.isFile() && held.size > 0;
  } catch {
    return false;
  }
}

/**
 * The same for the account an id names, which is what a record of a session carries. An id
 * nothing answers to — an account the owner has since removed — reads as the machine's own, so a reader
 * looking for a transcript looks somewhere rather than throwing.
 */
export async function recordsDirOf(archive: Archive, id: string | null | undefined): Promise<string> {
  const row = id === null || id === undefined ? null : await archive.claudeAccounts().getClaudeAccount(id);
  const credential = row === null ? null : credentialOf(row);

  return accountRecordsDir(credential);
}

/**
 * The account an id names, as a run carries it: its id and where its credential is reached, which
 * is what the launch needs to put the run on that login. Nothing where the id names no account —
 * a run carrying none goes out on the machine's own login, which is what every run did before
 * there was more than one.
 */
export async function runAccountFor(archive: Archive, id: string | null | undefined): Promise<AgentAccount | null> {
  const row = id === null || id === undefined ? null : await archive.claudeAccounts().getClaudeAccount(id);
  const credential = row === null ? null : credentialOf(row);

  return row === null || credential === null ? null : { id: row.id, name: row.name, credential };
}

/**
 * Where an account's login sits: a directory account's credential file and a token account's token
 * file. None for an endpoint account, which holds no login — its key file is the credential
 * itself, handed to the run and never opened here. Opened only to learn when a login ends. The
 * shape it takes is the neutral one a run carries, so a reader holding a run's own login needs no
 * translation.
 */
export function credentialFilePath(credential: { kind: string; path: string }): string | null {
  switch (credential.kind) {
    case "directory":
      return join(credential.path, CREDENTIAL_FILE);
    case "token":
      return credential.path;
    default:
      return null;
  }
}

/**
 * The id minted for a new account, from the name the owner gave it. An id is never the owner's to type: it is
 * what the records key on, so it is made once and left alone, and a name with nothing usable in
 * it still gets one.
 */
export function claudeAccountId(name: string, taken: readonly string[]): string {
  const slug = name
    .toLowerCase()
    .replaceAll(/[^a-z0-9._-]+/g, "-")
    .replace(/^[^a-z0-9]+/, "")
    .replace(/[-._]+$/, "")
    .slice(0, 32);
  const stem = CLAUDE_ACCOUNT_ID.test(slug) ? slug : "account";

  if (!taken.includes(stem)) {
    return stem;
  }

  for (let next = 2; next < 1000; next += 1) {
    const candidate = `${stem.slice(0, 28)}-${next}`;

    if (!taken.includes(candidate)) {
      return candidate;
    }
  }

  throw new Error(controlAnswers.accounts.no_id);
}

export function claudeAccountIdFault(id: string, taken: readonly string[]): string | null {
  if (!CLAUDE_ACCOUNT_ID.test(id)) {
    return controlAnswers.accounts.id_shape;
  }

  return taken.includes(id) ? fillMarks(controlAnswers.accounts.id_taken, { id }) : null;
}

// Two accounts with one name are two rows nobody can tell apart on a screen or in a report, so
// the name is taken as well as the id.
export function claudeAccountNameFault(name: string, taken: readonly string[]): string | null {
  const given = name.trim();

  if (given === "") {
    return controlAnswers.accounts.name_empty;
  }

  if (given.length > CLAUDE_ACCOUNT_NAME_MAX) {
    return fillMarks(controlAnswers.accounts.name_long, { most: String(CLAUDE_ACCOUNT_NAME_MAX) });
  }

  return taken.includes(given) ? fillMarks(controlAnswers.accounts.name_taken, { name: given }) : null;
}

// Why a path cannot be reached, or nothing: it must be absolute and there, a directory where one is
// asked and a file otherwise, with the words a path that is not one says.
function pathFault(path: string, directory: boolean, notIt: string): string | null {
  const given = path.trim();

  if (given === "") {
    return controlAnswers.accounts.path_empty;
  }

  if (!isAbsolute(given)) {
    return controlAnswers.accounts.path_relative;
  }

  if (!existsSync(given)) {
    return controlAnswers.accounts.path_missing;
  }

  return statSync(given).isDirectory() === directory ? null : notIt;
}

// Whether a text is an absolute http or https address with a host, which is what an endpoint's base
// URL has to be.
function httpUrl(text: string): boolean {
  try {
    const url = new URL(text.trim());

    return (url.protocol === "http:" || url.protocol === "https:") && url.host !== "";
  } catch {
    return false;
  }
}

/**
 * Why a credential cannot be reached, or nothing. A directory account needs a directory that is
 * there — whether a login has been made in it yet is a different question, and one the login
 * watch answers — a token account needs the file the token sits in, and an endpoint account needs
 * a configuration home that is there, an http or https base URL and the file its key sits in. No
 * file is opened here.
 */
export function claudeCredentialFault(credential: ClaudeCredential): string | null {
  const home = pathFault(
    credential.path,
    credential.kind !== "token",
    credential.kind === "token" ? controlAnswers.accounts.path_not_file : controlAnswers.accounts.path_not_directory,
  );

  if (home !== null || credential.kind !== "endpoint") {
    return home;
  }

  if (!httpUrl(credential.endpointUrl)) {
    return controlAnswers.accounts.endpoint_url;
  }

  return pathFault(credential.keyFile, false, controlAnswers.accounts.key_not_file);
}

// What a row's columns name, or null where they name no credential this road can use: an unknown
// kind, an endpoint row missing its base URL or key file, or a model map that is not an object from
// family words to model ids. Such a row is left out rather than read as another kind.
function credentialOf(row: ClaudeAccountRow): ClaudeCredential | null {
  switch (row.credentialKind) {
    case "directory":
      return { kind: "directory", path: row.credentialPath };
    case "token":
      return { kind: "token", path: row.credentialPath };
    case "endpoint": {
      const endpointUrl = row.endpointUrl ?? "";
      const keyFile = row.keyFile ?? "";
      const models = modelsOf(row.endpointModels);

      return endpointUrl === "" || keyFile === "" || models === null
        ? null
        : { kind: "endpoint", path: row.credentialPath, endpointUrl, keyFile, models };
    }
    default:
      return null;
  }
}

// The model map a row's text holds, an empty map where it holds none, or null where the text is not
// the object the column's contract says.
function modelsOf(text: string | null): ClaudeEndpointModels | null {
  if (text === null || text.trim() === "") {
    return {};
  }

  let held: unknown;

  try {
    held = JSON.parse(text);
  } catch {
    return null;
  }

  if (typeof held !== "object" || held === null || Array.isArray(held)) {
    return null;
  }

  const models: Partial<Record<EndpointFamily, string>> = {};

  for (const [family, id] of Object.entries(held as Record<string, unknown>)) {
    if (!(ENDPOINT_FAMILIES as readonly string[]).includes(family)) {
      return null;
    }

    if (id === null) {
      continue;
    }

    if (typeof id !== "string" || id.trim() === "") {
      return null;
    }

    models[family as EndpointFamily] = id;
  }

  return models;
}

// A credential as it is stored: each text it names with its edges trimmed.
function trimmedCredential(credential: ClaudeCredential): ClaudeCredential {
  switch (credential.kind) {
    case "endpoint":
      return {
        kind: "endpoint",
        path: credential.path.trim(),
        endpointUrl: credential.endpointUrl.trim(),
        keyFile: credential.keyFile.trim(),
        models: credential.models,
      };
    case "directory":
    case "token":
      return { kind: credential.kind, path: credential.path.trim() };
  }
}

// The columns a credential is kept in, an endpoint account's own set only on that kind
// (docs/store.md).
function credentialColumns(credential: ClaudeCredential): {
  credentialKind: ClaudeCredentialKind;
  credentialPath: string;
  endpointUrl: string | null;
  keyFile: string | null;
  endpointModels: string | null;
} {
  return credential.kind === "endpoint"
    ? {
        credentialKind: credential.kind,
        credentialPath: credential.path,
        endpointUrl: credential.endpointUrl,
        keyFile: credential.keyFile,
        endpointModels: JSON.stringify(credential.models),
      }
    : {
        credentialKind: credential.kind,
        credentialPath: credential.path,
        endpointUrl: null,
        keyFile: null,
        endpointModels: null,
      };
}

/**
 * What is wrong with adding an account, or nothing. The checks are one call so that the console,
 * the control API and the shell all refuse the same things for the same reasons.
 */
export function newClaudeAccountFault(
  account: NewClaudeAccount,
  held: readonly ClaudeAccount[],
): string | null {
  return (
    claudeAccountIdFault(account.id, held.map((one) => one.id)) ??
    claudeAccountNameFault(account.name, held.map((one) => one.name)) ??
    claudeCredentialFault(account.credential)
  );
}

export interface ClaudeAccountChange {
  name?: string;
  credential?: ClaudeCredential;
  enabled?: boolean;
  // Absent keeps the base the account holds; null clears it.
  basePrompt?: string | null;
}

/** What is wrong with changing an account, or nothing. Only what was named is judged. */
export function claudeAccountChangeFault(
  id: string,
  change: ClaudeAccountChange,
  held: readonly ClaudeAccount[],
): string | null {
  if (!held.some((one) => one.id === id)) {
    return fillMarks(controlAnswers.accounts.unknown, { id });
  }

  const others = held.filter((one) => one.id !== id);

  return (
    (change.name === undefined ? null : claudeAccountNameFault(change.name, others.map((one) => one.name))) ??
    (change.credential === undefined ? null : claudeCredentialFault(change.credential)) ??
    (change.enabled === false && !others.some((one) => one.enabled) ? lastAccountOffReason : null)
  );
}

export const lastAccountOffReason: string = controlAnswers.accounts.last_off;

/** The accounts as the owner ordered them, whether they are on or off. */
export async function claudeAccounts(archive: Archive): Promise<ClaudeAccount[]> {
  const held: ClaudeAccount[] = [];

  for (const row of await archive.claudeAccounts().claudeAccounts()) {
    const account = accountOf(row);

    if (account === null) {
      console.log(`claude account ${row.id}: its row names no credential this road can use, so it is left out`);
      continue;
    }

    held.push(account);
  }

  return held;
}

/** The accounts the Claude Code entry walks, in order. */
export async function enabledClaudeAccounts(archive: Archive): Promise<ClaudeAccount[]> {
  return (await claudeAccounts(archive)).filter((account) => account.enabled);
}

/**
 * One account by its id, or by the name the owner calls it. A shell asks by whichever the owner typed; the
 * records only ever ask by id.
 */
export async function claudeAccount(archive: Archive, named: string): Promise<ClaudeAccount | null> {
  const held = await claudeAccounts(archive);

  return held.find((one) => one.id === named) ?? held.find((one) => one.name === named.trim()) ?? null;
}

/**
 * An account added, or the refusal. The path is stored as it was given: a relative one is refused
 * rather than resolved, because the directory a run happens to stand in is nobody's statement
 * about where a login lives. The name is stored with its edges trimmed.
 */
export async function addClaudeAccount(archive: Archive, account: NewClaudeAccount): Promise<ClaudeAccount> {
  const asked: NewClaudeAccount = {
    ...account,
    name: account.name.trim(),
    credential: trimmedCredential(account.credential),
  };
  const fault = newClaudeAccountFault(asked, await claudeAccounts(archive));

  if (fault !== null) {
    throw new Error(fault);
  }

  await archive.claudeAccounts().addClaudeAccount({
    id: asked.id,
    name: asked.name,
    enabled: asked.enabled ?? true,
    ...credentialColumns(asked.credential),
    basePrompt: asked.basePrompt ?? null,
  });

  return await claudeAccount(archive, asked.id) as ClaudeAccount;
}

/** An account named by the owner, minted its own id. */
export async function addNamedClaudeAccount(
  archive: Archive,
  account: { name: string; credential: ClaudeCredential; enabled?: boolean; basePrompt?: string | null },
): Promise<ClaudeAccount> {
  return addClaudeAccount(archive, {
    ...account,
    id: claudeAccountId(account.name, (await claudeAccounts(archive)).map((one) => one.id)),
  });
}

/** What the owner changed about an account, or the refusal. */
export async function updateClaudeAccount(archive: Archive, id: string, change: ClaudeAccountChange): Promise<ClaudeAccount> {
  const asked: ClaudeAccountChange = {
    ...change,
    ...(change.name === undefined ? {} : { name: change.name.trim() }),
    ...(change.credential === undefined ? {} : { credential: trimmedCredential(change.credential) }),
  };
  const fault = claudeAccountChangeFault(id, asked, await claudeAccounts(archive));

  if (fault !== null) {
    throw new Error(fault);
  }

  await archive.claudeAccounts().updateClaudeAccount(id, {
    name: asked.name,
    enabled: asked.enabled,
    // A credential given as a whole writes every column it keeps, so an account moved off an
    // endpoint loses the URL, the key file and the map rather than carrying them under its new kind.
    ...(asked.credential === undefined ? {} : credentialColumns(asked.credential)),
    basePrompt: asked.basePrompt,
  });
  await letChoiceGoIfOff(archive);

  return await claudeAccount(archive, id) as ClaudeAccount;
}

// The order the Claude Code entry walks its accounts in. It is the accounts' own rows read in
// order, and that is the whole of why the sequence and the switches cannot disagree: there is no
// second statement of either to fall out of step with the list of accounts itself. The providers
// are a list the code states and the owner's order is a change to it, which is why that one is a stored
// value the bot removes once it matches the default; the accounts are the owner's own list from the start,
// so there is nothing to remove — putting the order back to the default is a reset.

/** One account turned on or off, its place and every other account's left where they are. */
export async function setClaudeAccountEnabled(archive: Archive, id: string, enabled: boolean): Promise<void> {
  await updateClaudeAccount(archive, id, { enabled });
}

// The account the owner chose to run on now. Choosing it makes it the login in use, and wherever the
// login in use cannot answer the walk starts on it and goes on through the order from the top, so
// choosing one moves where every automatic run goes without touching the order the owner dragged: it is
// one setting naming one account by id beside the rows, and nothing else says it.
// Only an account that is on can be chosen, and the choice goes with the account the moment that
// account is turned off or removed, so turning it back on later does not quietly bring it back.

const chosenAccountKey = "claude_account_chosen";

export const chosenAccountOffReason: string = controlAnswers.accounts.chosen_off;

// The login in use. A login in use is used to the end: every automatic run starts on it for as long
// as it answers, rather than moving back the moment an account ahead of it or the chosen one answers
// again, since the sessions and warm caches belong to the login that opened them and the owner spends one
// account's window before the next. One setting naming one account, so a restart starts every run
// where the road stood; the owner's choice moves it.
const inUseAccountKey = "claude_account_in_use";

/** The login in use, or nothing where none is held or the one held is no longer on. */
export async function claudeAccountInUse(archive: Archive): Promise<string | null> {
  const id = await archive.settings().getSetting(inUseAccountKey);

  return id !== null && (await enabledClaudeAccounts(archive)).some((one) => one.id === id) ? id : null;
}

/** That the road's runs go out on this login now, written only where it moved. */
export async function holdClaudeAccount(archive: Archive, id: string): Promise<void> {
  if (await archive.settings().getSetting(inUseAccountKey) !== id) {
    await archive.settings().setSetting(inUseAccountKey, id);
  }
}

/** Why an account cannot be chosen to run on now, or nothing. */
export function chooseClaudeAccountFault(id: string, held: readonly ClaudeAccount[]): string | null {
  const account = held.find((one) => one.id === id);

  if (account === undefined) {
    return fillMarks(controlAnswers.accounts.unknown, { id });
  }

  return account.enabled ? null : chosenAccountOffReason;
}

/** The account the walk starts on, or nothing where it walks the order from the top. */
export async function chosenClaudeAccount(archive: Archive): Promise<ClaudeAccount | null> {
  const id = await archive.settings().getSetting(chosenAccountKey);

  return id === null ? null : ((await enabledClaudeAccounts(archive)).find((one) => one.id === id) ?? null);
}

/** One account chosen to run on now, or the choice let go with nothing. */
export async function chooseClaudeAccount(archive: Archive, id: string | null): Promise<void> {
  // Letting the choice go hands the road back to the order, so the login in use goes with it and
  // the next run starts on the first open account of the order.
  if (id === null) {
    await archive.settings().deleteSetting(inUseAccountKey);
    await archive.settings().deleteSetting(chosenAccountKey);

    return;
  }

  const fault = chooseClaudeAccountFault(id, await claudeAccounts(archive));

  if (fault !== null) {
    throw new Error(fault);
  }

  // A choice is the owner saying where runs go now: it is a switch, so the login in use moves with it.
  await archive.settings().setSetting(inUseAccountKey, id);
  await archive.settings().setSetting(chosenAccountKey, id);
}

// A choice naming an account that is no longer on is let go rather than left to wake again.
async function letChoiceGoIfOff(archive: Archive): Promise<void> {
  if (await archive.settings().getSetting(chosenAccountKey) !== null && await chosenClaudeAccount(archive) === null) {
    await archive.settings().deleteSetting(chosenAccountKey);
  }
}

function accountOf(row: ClaudeAccountRow): ClaudeAccount | null {
  const credential = credentialOf(row);

  return credential === null
    ? null
    : {
        id: row.id,
        name: row.name,
        place: row.place,
        enabled: row.enabled,
        credential,
        basePrompt: row.basePrompt,
        addedAt: row.addedAt,
        userId: row.userId,
      };
}
