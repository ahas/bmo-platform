import { readFileSync, statSync } from "node:fs";

import { providerSettingsArchive } from "./provider-order";
import { dataPath } from "@agency/support/roots";

import type { Archive } from "@agency/archive/archive";
import type { AgentProviderName, Sampling } from "./harness";
import type { TierMapping } from "@agency/archive/model-tiers";

// The client a request with no tools is asked through (docs/own-llm.md, 「A request with no tools」):
// one registered model behind agency's gateway's OpenAI-compatible API, a bearer key in a file, no
// session. Which service a request goes to is the registration its run names, made into a service by
// the registry's provider; nothing here names one.

export interface OwnLlmService {
  // The provider name it is chosen by and recorded under.
  name: AgentProviderName;
  // Its name as a screen shows it.
  label: string;
  // Its name in a failure's message and in the words a screen says about it.
  shortLabel: string;
  // Where it answers: `/v1/models` and `/v1/chat/completions` stand under this.
  baseUrl: string;
  // The file its key is read from, under the runtime data root's `env/`.
  keyFile: string;
  // The registration it is: the model every request names, the one it takes from the gateway's
  // list, and whose context window every request is sized to (docs/own-llm.md, 「The context
  // window」).
  registration: string;
}

// What one work key asks one own service for: the model and thinking of the key's tier mapping on
// that service, and the answer ceiling that mapping names, or else the work's own default. A mapping that cannot be read leaves
// the model empty, and the run is then refused by the service's own catalogue check.
export interface OwnLlmSettings {
  provider: AgentProviderName;
  model: string;
  // The effort the mapping named, as the provider's vocabulary has it.
  effort: string | null;
  thinking: boolean;
  answerTokens: number;
  // The model's window, in tokens, where the road knows it.
  contextTokens?: number;
}

// The window a request is sized to: its registration's, read afresh out of the archive this process
// installed, so a change reaches the next request with no deployment. A process holding no archive,
// and a registration the registry no longer holds, have no window to read, and the request is refused
// naming the registration rather than sized at a number nobody chose.
export async function ownLlmWindow(service: OwnLlmService, archive: Archive | null = providerSettingsArchive()): Promise<number> {
  if (archive === null) throw new Error(`${service.shortLabel}: there is no store to read the context window from.`);

  const registration = await archive.modelRegistry().get(service.registration);

  if (registration === null) throw new Error(`${service.shortLabel}: there is no registered model ${service.registration} to read the context window of.`);

  return registration.contextTokens;
}

// The answer ceiling of a key that is no gate, where its tier names none.
export const defaultOwnLlmAnswerTokens = 4_096;

export function ownLlmSettings(service: OwnLlmService, mapping: TierMapping | null): OwnLlmSettings {
  return {
    provider: service.name,
    model: mapping?.model ?? "",
    effort: mapping?.effort ?? null,
    thinking: mapping?.effort === "thinking",
    answerTokens: mapping?.answerTokens ?? defaultOwnLlmAnswerTokens,
  };
}

export function ownLlmChatUrl(service: OwnLlmService): string {
  return `${service.baseUrl}/v1/chat/completions`;
}

export function ownLlmModelsUrl(service: OwnLlmService): string {
  return `${service.baseUrl}/v1/models`;
}

export function ownLlmKeyPath(service: OwnLlmService): string {
  return dataPath("env", service.keyFile);
}

// A 4,096-token answer and the gateway's model list fit well inside this ceiling. Read through a
// byte-counted stream instead of Response.text()/arrayBuffer(): headers arrive before the body,
// and an upstream that answers forever must not turn into unbounded memory.
const maxSuccessResponseBytes = 512 * 1_024;
const maxFailureResponseBytes = 64 * 1_024;
const defaultAttempts = 2;

export type OwnLlmFailure =
  | "key-unavailable"
  | "request-too-large"
  | "timeout"
  | "network"
  | "unauthorized"
  | "rate-limited"
  | "service"
  | "malformed"
  | "empty"

  | "catalogue-changed"
  | "response-too-large"
  | "credential-reflected";

// The message contains only the service's name, a class of failure and (where useful) a status. It
// never carries a response body, request body or credential, so callers can log it without leaking
// gate input. A failure is raised where the service is not in hand and named at the door the call
// came in by, so the message says which service failed.
export class OwnLlmError extends Error {
  readonly kind: OwnLlmFailure;
  readonly status: number | null;
  readonly transient: boolean;

  constructor(kind: OwnLlmFailure, options: { status?: number | null; transient?: boolean; service?: string } = {}) {
    const status = options.status ?? null;
    super(failureMessage(options.service ?? "own LLM", kind, status));
    this.name = "OwnLlmError";
    this.kind = kind;
    this.status = status;
    this.transient = options.transient ?? false;
  }

  named(service: OwnLlmService): this {
    this.message = failureMessage(service.shortLabel, this.kind, this.status);
    return this;
  }
}

function failureMessage(service: string, kind: OwnLlmFailure, status: number | null): string {
  return `${service} ${kind}${status === null ? "" : ` (${status})`}`;
}

// Runs one call at a service's door, so whatever failure comes out of it names that service.
async function atDoor<T>(service: OwnLlmService, call: () => Promise<T>): Promise<T> {
  try {
    return await call();
  } catch (error) {
    throw error instanceof OwnLlmError ? error.named(service) : error;
  }
}

export interface OwnLlmModel {
  id: string;
  ownedBy: string | null;
}

export type OwnLlmFetch = (input: string | URL | Request, init?: RequestInit) => Promise<Response>;

// How a request asks its model: thinking on or off, the answer's ceiling, and the sampling settings
// its caller names, carried to the engine by the gateway's sampling header; a request naming none is
// asked with the engine's own defaults.
export interface OwnLlmOptions {
  thinking?: boolean;
  answerTokens?: number;
  sampling?: Sampling;
  // The registration's context window as the process holding the archive read it, handed to a
  // request run in a process that holds none.
  contextTokens?: number;
}
export interface OwnLlmDependencies {
  fetch?: OwnLlmFetch;
  readCredential?: () => string;
  chatUrl?: string;
  modelsUrl?: string;
  attempts?: number;
  // The catalogue the caller has already verified, so a request run apart does not ask for it again.
  catalogue?: OwnLlmModel[];
}

// Whether the gateway's key file is usable: a file of something at 0600. The browser and archive
// may know this one bit and nothing more. A file with loose permissions is deliberately counted as
// absent: accepting it would turn a documented 0600 boundary into a suggestion.
function usableSecretFile(path: string): boolean {
  try {
    const stat = statSync(path);

    return stat.isFile() && (stat.mode & 0o777) === 0o600 && stat.size > 0;
  } catch {
    return false;
  }
}

export function readOwnLlmCredential(service: OwnLlmService, path: string = ownLlmKeyPath(service)): string {
  if (!usableSecretFile(path)) {
    throw new OwnLlmError("key-unavailable", { service: service.shortLabel });
  }

  const credential = readFileSync(path, "utf8").trim();

  if (credential === "") {
    throw new OwnLlmError("key-unavailable", { service: service.shortLabel });
  }

  return credential;
}

function headers(credential: string, more: Record<string, string> = {}): Headers {
  return new Headers({
    ...more,
    authorization: `Bearer ${credential}`,
    accept: "application/json",
    "content-type": "application/json",
  });
}

function statusError(status: number): OwnLlmError {
  if (status === 401 || status === 403) {
    return new OwnLlmError("unauthorized", { status });
  }

  if (status === 408 || status === 429) {
    return new OwnLlmError(status === 429 ? "rate-limited" : "timeout", { status, transient: true });
  }

  return new OwnLlmError("service", { status, transient: status >= 500 });
}

function parseResponseBody(body: string): unknown {
  if (body.trim() === "") {
    throw new OwnLlmError("empty");
  }

  try {
    return JSON.parse(body);
  } catch {
    throw new OwnLlmError("malformed");
  }
}

async function pause(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function boundedBody(
  response: Response,
  limit: number,
  keep: boolean,
): Promise<string> {
  const body = response.body;

  if (body === null) {
    return "";
  }

  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;

  try {
    while (true) {
      const part = await reader.read();

      if (part.done) {
        break;
      }

      size += part.value.byteLength;
      if (size > limit) {
        await reader.cancel().catch(async () => undefined);
        throw new OwnLlmError("response-too-large");
      }

      if (keep) {
        chunks.push(part.value);
      }
    }
  } finally {
    reader.releaseLock();
  }

  if (!keep || size === 0) {
    return "";
  }

  const joined = new Uint8Array(size);
  let offset = 0;

  for (const chunk of chunks) {
    joined.set(chunk, offset);
    offset += chunk.byteLength;
  }

  return new TextDecoder().decode(joined);
}

function rejectCredentialReflection(value: string, credential: string): void {
  if (credential !== "" && value.includes(credential)) {
    throw new OwnLlmError("credential-reflected");
  }
}

// One request waited for until it answers or its connection is refused or broken, asked again where its
// failure may pass, up to the attempts given.
async function request(url: string, init: RequestInit, fetcher: OwnLlmFetch, attempts: number): Promise<string> {
  let last = new OwnLlmError("network", { transient: true });

  for (let attempt = 0; attempt < Math.max(1, attempts); attempt += 1) {
    try {
      // Bun's fetch otherwise gives up on a connection silent for five minutes, and an engine reading a
      // long prompt sends nothing for longer than that.
      const response = await fetcher(url, { ...init, timeout: false } as RequestInit);

      if (response.ok) {
        // Fetch resolves when the headers arrive, not when the response has been consumed; the body is
        // read under a byte ceiling.
        const body = await boundedBody(response, maxSuccessResponseBytes, true);

        return body;
      }

      last = statusError(response.status);
      if (response.status === 400) {
        const complaint = await boundedBody(response, maxFailureResponseBytes, true);
        if (/context|maximum.*tokens/i.test(complaint)) throw new OwnLlmError("request-too-large");
      }
      // Drain only a bounded amount without retaining or exposing it. An oversized complaint is
      // cancelled and any failure while reading the complaint is ignored: the HTTP status was
      // already received and must remain authoritative, especially for non-retryable 401/403.
      try {
        await boundedBody(response, maxFailureResponseBytes, false);
      } catch {
        // The provider's error body is diagnostics, never part of agency's failure contract.
      }
    } catch (error) {
      last = error instanceof OwnLlmError ? error : new OwnLlmError("network", { transient: true });
    }

    if (!last.transient || attempt + 1 >= Math.max(1, attempts)) {
      throw last;
    }

    await pause(100 * (attempt + 1));
  }

  throw last;
}

// The catalogue a call verifies against, reused for a minute. Asked on every call it doubled the
// requests a gate makes; the catalogue itself changes rarely, and a
// call made after the minute asks again. It is kept per fetch and per catalogue address, so two
// callers holding different transports never read each other's answer, and each service's
// catalogue is its own.
const catalogueTtlMs = 60_000;
const catalogues = new WeakMap<OwnLlmFetch, Map<string, { at: number; models: OwnLlmModel[] }>>();

async function verifiedCatalogue(
  service: OwnLlmService,
  deps: Required<Omit<OwnLlmDependencies, "catalogue">>,
  credential: string,
): Promise<OwnLlmModel[]> {
  const byUrl = catalogues.get(deps.fetch) ?? new Map<string, { at: number; models: OwnLlmModel[] }>();
  const held = byUrl.get(deps.modelsUrl);
  const now = performance.now();

  if (held !== undefined && now - held.at < catalogueTtlMs) {
    return held.models;
  }

  const models = await ownLlmModels(service, {
    ...deps,
    readCredential: () => credential,
  });

  byUrl.set(deps.modelsUrl, { at: performance.now(), models });
  catalogues.set(deps.fetch, byUrl);

  return models;
}

function dependencies(service: OwnLlmService, options: OwnLlmDependencies): Required<Omit<OwnLlmDependencies, "catalogue">> {
  return {
    fetch: options.fetch ?? fetch,
    readCredential: options.readCredential ?? (() => readOwnLlmCredential(service)),
    chatUrl: options.chatUrl ?? ownLlmChatUrl(service),
    modelsUrl: options.modelsUrl ?? ownLlmModelsUrl(service),
    attempts: options.attempts ?? defaultAttempts,
  };
}

// The catalogue a gate request is checked against, as the caller's side holds it: model discovery, asked of
// the service at most once a minute, and handed to the request that runs apart.
export async function ownLlmCatalogue(service: OwnLlmService, options: OwnLlmDependencies = {}): Promise<OwnLlmModel[]> {
  return await atDoor(service, async () => {
    const deps = dependencies(service, options);

    return await verifiedCatalogue(service, deps, deps.readCredential());
  });
}

export async function ownLlmModels(
  service: OwnLlmService,
  options: OwnLlmDependencies = {},
): Promise<OwnLlmModel[]> {
  return await atDoor(service, async () => {
    const deps = dependencies(service, options);
    const credential = deps.readCredential();
    const response = await request(
      deps.modelsUrl,
      { method: "GET", headers: headers(credential) },
      deps.fetch,
      deps.attempts,
    );
    const parsed = parseResponseBody(response) as {
      data?: unknown;
    };

    if (!Array.isArray(parsed?.data)) {
      throw new OwnLlmError("malformed");
    }

    const models = parsed.data.flatMap((entry): OwnLlmModel[] => {
      if (typeof entry !== "object" || entry === null || typeof (entry as { id?: unknown }).id !== "string") {
        return [];
      }

      const id = (entry as { id: string }).id.trim();
      const owned = (entry as { owned_by?: unknown }).owned_by;

      return id === "" ? [] : [{ id, ownedBy: typeof owned === "string" ? owned : null }];
    });

    if (models.length === 0) {
      throw new OwnLlmError("empty");
    }

    for (const entry of models) {
      rejectCredentialReflection(entry.id, credential);
      if (entry.ownedBy !== null) {
        rejectCredentialReflection(entry.ownedBy, credential);
      }
    }

    return models;
  });
}

