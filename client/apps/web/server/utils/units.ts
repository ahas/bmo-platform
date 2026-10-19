import { CONTROL_SLOW, CONTROL_UNREACHABLE } from "#shared/config";
import { wording } from "#shared/wording";

// Where another unit is reached (docs/boundary.md, 「Settings」): each by the base address and the token its
// settings name, `<NAME>_ADDRESS` and `<NAME>_TOKEN` in the console's environment. A caller names the unit it
// calls; a unit whose address is not set is none, and the screen that needs it stands as for a unit not yet
// split, making no call. Every call is made here on the console's server: the browser only ever talks to this
// app's own routes, and no address or token reaches it.

/** A unit's base address, without a trailing slash, and the token it is called with where it takes one. */
export interface UnitAddress {
  address: string;
  token: string | null;
}

/** The unit the environment names under `name`, such as `AGENCY` or `CACHE_SIREN`, or null where none is set. */
export function unitAt(name: string, environment: Record<string, string | undefined> = process.env): UnitAddress | null {
  const address = environment[`${name}_ADDRESS`]?.trim() ?? "";

  if (address === "") return null;

  const token = environment[`${name}_TOKEN`]?.trim() ?? "";

  return { address: address.replace(/\/+$/, ""), token: token === "" ? null : token };
}

// A call that only reads a unit's state answers at once; one that runs a model takes as long as the model
// takes.
const CALL_TIMEOUT_MS = 5_000;

/** The headers a unit is called with: its token as a bearer, where it takes one. */
export function unitHeaders(unit: UnitAddress): Record<string, string> {
  return unit.token === null ? {} : { Authorization: `Bearer ${unit.token}` };
}

// How a call is made beyond the request itself: how long to wait, and whether a refusal the unit wrote for a
// person travels on in its own words. Most refusals are notes to a programmer and read as this app's own
// sentence instead; the ones written to be read on a screen say so here.
export interface UnitCall {
  timeoutMs?: number;
  // Whether the unit's own words for a refusal are passed on: for every refusal, or only for the statuses
  // named.
  carryReason?: boolean | number[];
  // Statuses beyond the usual ones that travel on as they are, where the caller's screen tells them apart.
  passStatus?: number[];
  // The unit's refusal codes, each with the pack's key for the sentence it is said in. The unit's own words
  // are English and for an agent to read, so a refusal whose code stands here is said in the pack's sentence;
  // the table is the calling unit's, written where its calls are.
  codes?: Readonly<Record<string, string>>;
  // Where a unit's refusal holds its code and that code's parameters: the refusal body itself where none is
  // given, or the part of it the unit's document names.
  refusal?: (body: Record<string, unknown>) => Record<string, unknown>;
}

// A unit that does not answer is this app's own failure, not the fetch's error: the view reads that one line
// and goes read-only. A call the unit took and the wait ran out on is the unit held up and is a 504; a call
// nothing took, a call on a unit whose address is not set, or one broken on the way is the unit gone and is
// a 503 (docs/web.md, 「How the units are talked to」).
export async function unitFetch<T>(name: string, path: string, init?: RequestInit, call: number | UnitCall = CALL_TIMEOUT_MS): Promise<T> {
  const asked: UnitCall = typeof call === "number" ? { timeoutMs: call } : call;
  const unit = unitAt(name);

  if (unit === null) throw fail(503, wording(CONTROL_UNREACHABLE));

  let response: Response;

  try {
    response = await fetch(`${unit.address}${path}`, {
      ...init,
      headers: { ...unitHeaders(unit), ...((init?.headers as Record<string, string> | undefined) ?? {}) },
      signal: AbortSignal.timeout(asked.timeoutMs ?? CALL_TIMEOUT_MS)
    });
  } catch (error) {
    throw timedOut(error) ? fail(504, wording(CONTROL_SLOW)) : fail(503, wording(CONTROL_UNREACHABLE));
  }

  if (!response.ok) {
    const status = asked.passStatus?.includes(response.status) === true ? response.status : statusOf(response.status);
    const body = await refusalOf(response);
    const held: Refusal | null = body === null || asked.refusal === undefined ? body : asked.refusal(body as Record<string, unknown>);
    const coded = typeof held?.code === "string" ? asked.codes?.[held.code] : undefined;
    const reason = coded !== undefined ? wording(coded, fieldsOf(held)) : reasonIn(body, response.status, carries(asked, response.status));

    // A refusal about one field names it, so the screen can say it under that field; its code travels too, for
    // the caller that tells refusals apart.
    throw createError({ statusCode: status, message: reason, data: { field: typeof body?.field === "string" ? body.field : null, code: typeof held?.code === "string" ? held.code : null } });
  }

  // An answer with no body — a 204, or a turn taken to be answered later — answers nothing.
  const text = response.status === 204 ? "" : await response.text();

  return (text === "" ? null : JSON.parse(text)) as T;
}

// The headers of a unit's answer a browser reads of bytes handed on whole: what they are, how many, how they are
// kept, and the stretch a range was answered with.
const BYTE_HEADERS = ["content-type", "content-length", "content-disposition", "cache-control", "x-content-type-options",
  "etag", "last-modified", "accept-ranges", "content-range"];

// How long a unit has to begin answering bytes; the bytes themselves, a file sent whole, may take longer.
const BYTES_BEGIN_MS = 30_000;

/** How bytes are asked of a unit: the request as it goes, and the unit's refusal codes said in the pack's words. */
export interface UnitBytes {
  method?: string;
  headers?: Record<string, string>;
  body?: string;
  codes?: Readonly<Record<string, string>>;
}

/**
 * A unit's answer handed on whole, its status, its bytes as they stream and the headers a browser reads of them;
 * a refusal whose code stands in the caller's table is said in the pack's sentence, and any other answer goes on
 * as the unit gave it.
 */
export async function unitBytes(name: string, path: string, asked: UnitBytes = {}): Promise<Response> {
  const unit = unitAt(name);

  if (unit === null) throw fail(503, wording(CONTROL_UNREACHABLE));

  const waiting = new AbortController();
  const timer = setTimeout(() => waiting.abort(new DOMException("the unit did not begin its answer", "TimeoutError")), BYTES_BEGIN_MS);
  let response: Response;

  try {
    response = await fetch(`${unit.address}${path}`, { method: asked.method ?? "GET", headers: { ...unitHeaders(unit), ...asked.headers }, body: asked.body, signal: waiting.signal });
  } catch (error) {
    throw timedOut(error) || waiting.signal.aborted ? fail(504, wording(CONTROL_SLOW)) : fail(503, wording(CONTROL_UNREACHABLE));
  } finally {
    clearTimeout(timer);
  }

  if (!response.ok && asked.codes !== undefined && (response.headers.get("content-type") ?? "").includes("json")) {
    const refused = (await response.clone().json().catch(() => null)) as Refusal | null;
    const key = typeof refused?.code === "string" ? asked.codes[refused.code] : undefined;

    if (key !== undefined) {
      throw createError({ statusCode: response.status, message: wording(key, fieldsOf(refused)), data: { code: refused?.code, field: null } });
    }
  }

  const answered = new Headers();

  for (const header of BYTE_HEADERS) {
    const value = response.headers.get(header);

    if (value !== null) answered.set(header, value);
  }

  return new Response(response.body, { status: response.status, statusText: response.statusText, headers: answered });
}

interface Refusal {
  error?: unknown;
  code?: unknown;
  field?: unknown;
}

// A refusal's own fields, which fill the marks of the sentence its code is said in: a word or a number as it
// stands, and a list of them joined into one.
function fieldsOf(body: Refusal | null): Record<string, string | number> {
  const fields: Record<string, string | number> = {};

  for (const [name, value] of Object.entries(body ?? {})) {
    if (name === "code" || name === "error") continue;
    if (typeof value === "string" || typeof value === "number") fields[name] = value;
    else if (Array.isArray(value) && value.every((one) => typeof one === "string" || typeof one === "number")) fields[name] = value.join(", ");
  }

  return fields;
}

async function refusalOf(response: Response): Promise<Refusal | null> {
  return (await response.json().catch(async () => null)) as Refusal | null;
}

function timedOut(error: unknown): boolean {
  return error instanceof Error && error.name === "TimeoutError";
}

/** Whether a failed unit call was the unit too busy to answer rather than a unit not there. */
export function unitSlow(error: unknown): boolean {
  return (error as { statusCode?: unknown } | null)?.statusCode === 504;
}

export function carries(call: UnitCall, status: number): boolean {
  return call.carryReason === true || (Array.isArray(call.carryReason) && call.carryReason.includes(status));
}

// A unit that refused the call refused it for a reason of its own, and the caller has something to say about
// each of those, so they travel on as they are. Anything else is this app's own gateway failure.
function statusOf(status: number): number {
  return status === 400 || status === 404 || status === 409 || status === 503 ? status : 502;
}

// A refusal a unit wrote for a person is the whole of what they need, and saying "the unit answered 400" in
// its place would throw the answer away. Every unit's refusal carries its sentence as `error`.
export async function reasonOf(response: Response, carry: boolean): Promise<string> {
  return reasonIn(await refusalOf(response), response.status, carry);
}

function reasonIn(body: Refusal | null, status: number, carry: boolean): string {
  if (carry && typeof body?.error === "string" && body.error !== "") {
    return body.error;
  }

  return wording("control.error.status", { status });
}

/** Whether a unit answers its health at all. */
export async function unitReachable(name: string): Promise<boolean> {
  const unit = unitAt(name);

  if (unit === null) return false;

  try {
    const response = await fetch(`${unit.address}/health`, { headers: unitHeaders(unit), signal: AbortSignal.timeout(CALL_TIMEOUT_MS) });

    return response.ok;
  } catch {
    return false;
  }
}
