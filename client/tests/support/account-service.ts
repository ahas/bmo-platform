// A stand-in for the account service, for the tests of its callers (docs/accounts.md): the routes
// `docs/boundary.md` states, on a loopback port of a test's own and behind a token of its own, answering
// out of what the test seeds rather than out of sumida, so a console test needs neither the server's binary
// nor Valkey. What it decides follows the rules the server's own tests hold sumida to: the administrator
// passes every check; a sender is checked as their linked user and the room's members; a standing rule
// opens its room; an expired grant is none.

export interface StandInUser {
  id: number;
  name: string;
  password?: string;
  administrator?: boolean;
  senders?: number[];
}

export interface StandInGrant {
  kind?: string;
  object: string;
  relation: string;
  // `user:<id>` or `bmo_member:<room>`.
  subject: string;
  expiresAt?: number;
  words?: string;
}

export interface StandInSeed {
  inForce?: boolean;
  users?: StandInUser[];
  grants?: StandInGrant[];
  // The messages a check on a message reads: room, message, sender.
  messages?: [number, number, number | null][];
}

export interface StandIn {
  address: string;
  token: string;
  users: StandInUser[];
  grants: StandInGrant[];
  keys: Map<string, { user: number; key: string }>;
  moves: unknown[];
  state: { inForce: boolean };
  stop(): void;
}

const ROOM = "bmo_room";
const TOKEN = "a stand-in token";

function hashOf(key: string): string {
  return new Bun.CryptoHasher("sha256").update(`stand-in:${key}`).digest("hex");
}

/** The stand-in, listening on a loopback port of its own until it is stopped. */
export function standInAccountService(seed: StandInSeed = {}): StandIn {
  const users = [...(seed.users ?? [])];
  const grants = [...(seed.grants ?? [])];
  const messages = new Map((seed.messages ?? []).map(([room, message, sender]) => [`${room}:${message}`, sender]));
  const sessions = new Map<string, number>();
  const keys = new Map<string, { user: number; key: string }>();
  const moves: unknown[] = [];
  const state = { inForce: seed.inForce ?? true };
  const userOfSender = (sender: number | null): StandInUser | undefined =>
    sender === null ? undefined : users.find((user) => (user.senders ?? []).includes(sender));
  const holds = (kind: string, object: string, relation: string, subjects: string[]): boolean =>
    grants.some(
      (one) =>
        (one.kind ?? ROOM) === kind &&
        one.object === object &&
        (one.relation === relation || (kind === ROOM && one.relation === "rule")) &&
        subjects.includes(one.subject) &&
        (one.expiresAt === undefined || one.expiresAt > Date.now())
    );
  const checkRoom = (room: number, sender: number | null, relation: string) => {
    const user = userOfSender(sender);
    const subjects = [`bmo_member:${room}`, ...(user === undefined ? [] : [`user:${user.id}`])];

    return {
      allowed: user?.administrator === true || holds(ROOM, String(room), relation, subjects),
      grant: `${ROOM}:${room}#${relation}`,
      user: user === undefined ? null : String(user.id)
    };
  };
  const json = (body: unknown, status = 200, headers: Record<string, string> = {}): Response =>
    new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", ...headers } });
  const refused = (status: number, code: string, error: string): Response => json({ error, code }, status);
  const done = (): Response => new Response(null, { status: 204 });
  const cookieOf = (request: Request): string | undefined => request.headers.get("cookie")?.split(";")[0]?.trim();
  const userNamed = (id: unknown): StandInUser | undefined => users.find((one) => String(one.id) === String(id));

  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    async fetch(request) {
      const path = new URL(request.url).pathname;
      const body =
        request.method === "GET" || request.method === "HEAD"
          ? null
          : ((await request.json().catch(() => null)) as Record<string, any> | null);

      // sumida's own session routes take the session cookie, not the token.
      if (path === "/sumida/auth/session" && request.method === "POST") {
        const user = users.find((one) => one.name === body?.username && one.password !== undefined && one.password === body?.password);

        if (user === undefined) return json({ title: "Unauthorized" }, 401);

        const id = crypto.randomUUID();

        sessions.set(`bmo.sid=${id}`, user.id);

        return json({}, 200, { "set-cookie": `bmo.sid=${id}; HttpOnly; Path=/` });
      }
      if (path === "/sumida/auth/session" && request.method === "DELETE") {
        sessions.delete(cookieOf(request) ?? "");

        return json({});
      }
      if (path === "/sumida/me") {
        const user = users.find((one) => one.id === sessions.get(cookieOf(request) ?? ""));

        return user === undefined ? json({ title: "Unauthorized" }, 401) : json({ id: user.id, username: user.name });
      }

      if (path === "/health") return json({ ok: true, version: "0.0.0", in_force: state.inForce });
      if (request.headers.get("authorization") !== `Bearer ${TOKEN}`) return refused(401, "unauthorized", "the token is missing or wrong");
      if (path === "/accounts/model") {
        return json(state.inForce ? { in_force: true } : { in_force: false, why: "the data move has not been written" });
      }
      if (path === "/accounts/move") {
        moves.push(body);

        const administrator = users.find((one) => one.administrator);

        return json({ done: true, written: moves.length === 1, administrator: administrator === undefined ? null : String(administrator.id) });
      }
      if (!state.inForce) return refused(503, "not_in_force", "the account model is not in force");

      const userRoute = path.match(/^\/users\/([^/]+)$/);

      if (userRoute !== null) {
        const user = userNamed(decodeURIComponent(userRoute[1]!));

        return user === undefined
          ? refused(404, "not_found", "no such user")
          : json({ user: String(user.id), name: user.name, administrator: user.administrator === true, senders: user.senders ?? [] });
      }

      const passwordRoute = path.match(/^\/users\/([^/]+)\/password$/);

      if (passwordRoute !== null) {
        const user = userNamed(decodeURIComponent(passwordRoute[1]!));

        if (user === undefined || user.password !== body!.current) return refused(401, "wrong_password", "the current password does not match");

        user.password = body!.next;

        return done();
      }

      const nameRoute = path.match(/^\/users\/([^/]+)\/name$/);

      if (nameRoute !== null) {
        const user = userNamed(decodeURIComponent(nameRoute[1]!));

        if (users.some((one) => one.name === body!.name && one !== user)) return refused(409, "name_taken", "another user holds that name");
        if (user !== undefined) user.name = body!.name;

        return done();
      }
      if (path === "/checks/room") return json(checkRoom(body!.room, body!.sender ?? null, body!.relation));
      if (path === "/checks/message") {
        const key = `${body!.room}:${body!.message}`;

        if (!messages.has(key)) return refused(404, "no_such_message", `no message ${body!.message} stands in room ${body!.room}`);

        const sender = messages.get(key) ?? null;

        return json({ ...checkRoom(body!.room, sender, body!.relation), sender });
      }
      if (path === "/checks/object") {
        const user = userNamed(body!.user);

        return json({
          allowed: user?.administrator === true || holds(body!.kind, body!.object, body!.relation, [`user:${body!.user}`]),
          grant: `${body!.kind}:${body!.object}#${body!.relation}`
        });
      }
      if (path === "/grants" && request.method !== "GET") {
        const granter = userOfSender(messages.get(`${body!.message.room}:${body!.message.id}`) ?? null);
        const kind = body!.room === undefined ? body!.kind : ROOM;
        const object = body!.room === undefined ? body!.object : String(body!.room);

        if (granter?.administrator !== true) return refused(403, "forbidden", "the sender may not grant");

        let subject: string;
        let user: string | null = null;

        if (body!.to.members === true) {
          subject = `bmo_member:${object}`;
        } else {
          let holder = body!.to.user !== undefined ? userNamed(body!.to.user) : userOfSender(body!.to.sender);

          if (holder === undefined && body!.to.sender !== undefined && request.method === "POST") {
            holder = { id: Math.max(0, ...users.map((one) => one.id)) + 1, name: `member-${body!.to.sender}`, senders: [body!.to.sender] };
            users.push(holder);
          }
          if (holder === undefined) return json({ grant: `${kind}:${object}#${body!.relation}`, revoked: 0 });

          subject = `user:${holder.id}`;
          user = String(holder.id);
        }

        const same = (one: StandInGrant): boolean =>
          (one.kind ?? ROOM) === kind && one.object === object && one.relation === body!.relation && one.subject === subject;

        if (request.method === "DELETE") {
          const before = grants.length;

          grants.splice(0, grants.length, ...grants.filter((one) => !same(one)));

          return json({ grant: `${kind}:${object}#${body!.relation}`, revoked: before - grants.length });
        }

        grants.splice(0, grants.length, ...grants.filter((one) => !same(one)));
        grants.push({ kind, object, relation: body!.relation, subject, expiresAt: body!.expires_at ?? undefined, words: body!.words ?? undefined });

        return json({ grant: `${kind}:${object}#${body!.relation}`, subject, user });
      }
      if (path === "/phones" && request.method === "POST") {
        const key = crypto.randomUUID().replaceAll("-", "") + crypto.randomUUID().replaceAll("-", "");
        const hash = hashOf(key);

        keys.set(hash, { user: Number(body!.user), key });

        return json({ key, hash });
      }
      if (path === "/phones/adopt") {
        const hash = hashOf(body!.token);

        keys.set(hash, { user: Number(body!.user), key: body!.token });

        return json({ hash });
      }
      if (path === "/phones/who") {
        const hash = hashOf(body!.key);
        const held = keys.get(hash);
        const user = users.find((one) => one.id === held?.user);

        return held === undefined || user === undefined
          ? refused(401, "unauthorized", "no live key")
          : json({ user: String(user.id), name: user.name, hash });
      }

      const phone = path.match(/^\/phones\/(.+)$/);

      if (phone !== null && request.method === "DELETE") {
        keys.delete(decodeURIComponent(phone[1]!));

        return done();
      }

      return refused(404, "not_found", `no route ${request.method} ${path}`);
    }
  });

  return {
    address: `http://127.0.0.1:${server.port}`,
    token: TOKEN,
    users,
    grants,
    keys,
    moves,
    state,
    stop() {
      server.stop(true);
    }
  };
}
