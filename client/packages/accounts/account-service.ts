// The account service as the console reaches it (docs/accounts.md; docs/boundary.md, 「What the server
// serves」): every route the console asks the server on, over HTTP with the server's token, and sumida's
// session routes under `/sumida`, which take the session cookie instead. The service holds the decisions;
// this module only asks.

export interface ServiceOptions {
  // The server's base address; `WEB_SERVER_ADDRESS` where none is given.
  address?: string;
  // The token the server is called with; `WEB_SERVER_TOKEN` where none is given.
  token?: string;
  // How long one request may take before the service is read as not answering.
  timeoutMs?: number;
}

/** A user as the server names them: the id it gives, a string handed on as it stands. */
export type UserId = string;

/** The service did not answer at all: nothing listens at its address, or it did not answer in time. */
export class AccountServiceDown extends Error {
  constructor(readonly address: string, why: string) {
    super(`bmo-web-server does not answer at ${address}: ${why}`);
  }
}

/** The service answered and refused, with its status, its code and its sentence. */
export class AccountRefusal extends Error {
  constructor(readonly status: number, readonly code: string, readonly detail: string) {
    super(detail);
  }
}

interface Reply {
  status: number;
  body: unknown;
  headers: Headers;
}

const defaultTimeoutMs = 5_000;

function addressOf(options: ServiceOptions): string {
  return (options.address ?? process.env.WEB_SERVER_ADDRESS ?? "").replace(/\/+$/, "");
}

async function ask(
  method: string,
  path: string,
  body: unknown,
  headers: Record<string, string>,
  options: ServiceOptions
): Promise<Reply> {
  const address = addressOf(options);

  if (address === "") throw new AccountServiceDown("(no address)", "no address is set for the server");

  let response: Response;

  try {
    response = await fetch(`${address}${path}`, {
      method,
      headers: body === undefined ? headers : { "content-type": "application/json", ...headers },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(options.timeoutMs ?? defaultTimeoutMs)
    });
  } catch (error) {
    throw new AccountServiceDown(address, error instanceof Error ? error.message : String(error));
  }

  const text = await response.text();
  let parsed: unknown = null;

  try {
    parsed = text === "" ? null : JSON.parse(text);
  } catch {
    parsed = text;
  }

  return { status: response.status, body: parsed, headers: response.headers };
}

// The server's refusal is `{ error, code }`; sumida's own routes answer a problem with a `title` and a
// `detail`.
function refusalOf(reply: Reply): AccountRefusal {
  const said = reply.body !== null && typeof reply.body === "object" ? (reply.body as Record<string, unknown>) : {};
  const sentence = [said.error, said.detail, said.title].find((one): one is string => typeof one === "string");
  const code = typeof said.code === "string" ? said.code : "failed";

  return new AccountRefusal(reply.status, code, sentence ?? (typeof reply.body === "string" ? reply.body : "no reason given"));
}

// A route of the server's own, asked with its token: the JSON answer, or the refusal thrown.
async function answered<T>(method: string, path: string, body: unknown, options: ServiceOptions): Promise<T> {
  const token = options.token ?? process.env.WEB_SERVER_TOKEN ?? "";
  const reply = await ask(method, path, body, { authorization: `Bearer ${token}` }, options);

  if (reply.status < 200 || reply.status >= 300) throw refusalOf(reply);

  return reply.body as T;
}

/** Whether the model is in force, and why not where it is not. */
export interface ModelState {
  inForce: boolean;
  reason?: string;
}

/** What the service says of the model, or null where it does not answer. */
export async function modelState(options: ServiceOptions = {}): Promise<ModelState | null> {
  try {
    const said = await answered<{ in_force: boolean; why?: string | null }>("GET", "/accounts/model", undefined, options);

    return said.in_force ? { inForce: true } : { inForce: false, reason: said.why ?? undefined };
  } catch (error) {
    if (error instanceof AccountServiceDown) return null;

    throw error;
  }
}

// The headers sumida's own routes are asked with: the browser's address, which a ban reads, and the
// session's cookie where there is one.
function signedHeaders(address: string, cookie?: string): Record<string, string> {
  return cookie === undefined ? { "x-real-ip": address } : { "x-real-ip": address, cookie };
}

/**
 * A sign-in with a name and a password: the session's cookie, as `name=value`, or null where the
 * service refuses the credentials. `address` is the browser's.
 */
export async function signIn(name: string, password: string, address: string, options: ServiceOptions = {}): Promise<string | null> {
  const reply = await ask(
    "POST",
    "/sumida/auth/session",
    { username: name, password, rememberMe: false },
    signedHeaders(address),
    options
  );

  if (reply.status === 401 || reply.status === 403) return null;
  if (reply.status !== 200) throw refusalOf(reply);

  const cookie = reply.headers.get("set-cookie")?.split(";")[0]?.trim();

  if (cookie === undefined || cookie === "") throw new AccountRefusal(reply.status, "failed", "the sign-in answered with no session");

  return cookie;
}

/** Who a session is, or null where it is no live session. */
export async function sessionUser(cookie: string, address: string, options: ServiceOptions = {}): Promise<{ id: UserId; name: string } | null> {
  const reply = await ask("GET", "/sumida/me", undefined, signedHeaders(address, cookie), options);

  if (reply.status === 401 || reply.status === 403) return null;
  if (reply.status !== 200) throw refusalOf(reply);

  const said = reply.body as { id?: unknown; username?: unknown };
  const id = typeof said.id === "number" || typeof said.id === "string" ? String(said.id) : null;

  return id !== null && typeof said.username === "string" ? { id, name: said.username } : null;
}

/** A session ended; one already gone is ended too. */
export async function signOut(cookie: string, address: string, options: ServiceOptions = {}): Promise<void> {
  await ask("DELETE", "/sumida/auth/session", undefined, signedHeaders(address, cookie), options);
}

/** A user as the console reads them. */
export interface ServiceUser {
  id: UserId;
  name: string;
  administrator: boolean;
  senders: number[];
}

const userPath = (user: UserId | number) => `/users/${encodeURIComponent(String(user))}`;

/** A user, or null where none has that id. */
export async function userOf(id: UserId | number, options: ServiceOptions = {}): Promise<ServiceUser | null> {
  try {
    const said = await answered<{ user: UserId; name: string; administrator: boolean; senders: number[] }>(
      "GET",
      userPath(id),
      undefined,
      options
    );

    return { id: said.user, name: said.name, administrator: said.administrator, senders: said.senders };
  } catch (error) {
    if (error instanceof AccountRefusal && error.status === 404) return null;

    throw error;
  }
}

/** A user's password changed on the strength of their current one; false where that does not match. */
export async function changeUserPassword(user: UserId | number, current: string, next: string, options: ServiceOptions = {}): Promise<boolean> {
  try {
    await answered("POST", `${userPath(user)}/password`, { current, next }, options);

    return true;
  } catch (error) {
    if (error instanceof AccountRefusal && error.code === "wrong_password") return false;

    throw error;
  }
}

/** A user given another name; false where another user holds it. */
export async function renameUser(user: UserId | number, name: string, options: ServiceOptions = {}): Promise<boolean> {
  try {
    await answered("POST", `${userPath(user)}/name`, { name }, options);

    return true;
  } catch (error) {
    if (error instanceof AccountRefusal && error.code === "name_taken") return false;

    throw error;
  }
}

/** A phone's new key, handed out once, and the hash it is kept by. */
export async function makePhoneKey(user: UserId | number, options: ServiceOptions = {}): Promise<{ key: string; hash: string }> {
  return await answered("POST", "/phones", { user: String(user) }, options);
}

/** A token issued before the model carried onto a key of the same text; its hash. */
export async function adoptPhone(token: string, user: UserId | number, options: ServiceOptions = {}): Promise<string> {
  return (await answered<{ hash: string }>("POST", "/phones/adopt", { token, user: String(user) }, options)).hash;
}

/** Whose a phone's key is, or null where it is no live key. */
export async function phoneHolder(key: string, options: ServiceOptions = {}): Promise<{ user: UserId; name: string; hash: string } | null> {
  try {
    return await answered("POST", "/phones/who", { key }, options);
  } catch (error) {
    if (error instanceof AccountRefusal && error.status === 401) return null;

    throw error;
  }
}

/** A phone's key taken away by its hash; one already gone is taken away too. */
export async function deletePhoneKey(hash: string, options: ServiceOptions = {}): Promise<void> {
  await answered("DELETE", `/phones/${encodeURIComponent(hash)}`, undefined, options);
}

/** What a check answered: whether it opens, the grant it asked for, and the user it was made for. */
export interface CheckAnswer {
  allowed: boolean;
  grant: string;
  user: UserId | null;
}

/** Whether a sender holds a room relation in a room; a null sender is the room's members alone. */
export async function checkRoom(room: number, sender: number | null, relation: string, options: ServiceOptions = {}): Promise<CheckAnswer> {
  const said = await answered<{ allowed: boolean; grant: string; user?: UserId | null }>(
    "POST",
    "/checks/room",
    { room, sender, relation },
    options
  );

  return { allowed: said.allowed, grant: said.grant, user: said.user ?? null };
}

/** Whether a user holds a relation on a worker, a repository or a tool capability. */
export async function checkObject(
  kind: string,
  object: string,
  relation: string,
  user: UserId | number,
  options: ServiceOptions = {}
): Promise<CheckAnswer> {
  const said = await answered<{ allowed: boolean; grant: string }>(
    "POST",
    "/checks/object",
    { kind, object, relation, user: String(user) },
    options
  );

  return { allowed: said.allowed, grant: said.grant, user: String(user) };
}

/** What the data move is handed: what only the other units hold (docs/accounts.md, 「The data move」). */
export interface MoveRequest {
  ownerSenders: number[];
  rooms: number[];
  roomRules: { room: number; id: string; subject: number; words: string }[];
}

/** The data move, run once; a second call answers that it is done. */
export async function runMove(
  request: MoveRequest,
  options: ServiceOptions = {}
): Promise<{ done: boolean; written: boolean; administrator: UserId | null }> {
  const said = await answered<{ done: boolean; written: boolean; administrator?: UserId | null }>(
    "POST",
    "/accounts/move",
    { owner_senders: request.ownerSenders, rooms: request.rooms, room_rules: request.roomRules },
    { ...options, timeoutMs: options.timeoutMs ?? 120_000 }
  );

  return { done: said.done, written: said.written, administrator: said.administrator ?? null };
}
