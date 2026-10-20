// A console of its own for the browser tests that drive one (docs/checks.md, 「How the suite runs the tests」):
// the account service on a scratch database of the test server, a stand-in for every unit the console reads, the
// fixture account each of those tests signs in with, and the console's development server on top of them, the
// one console that serves the screen-measuring module. It prints the console's address once the console answers,
// and stands until it is told to stop, then stops everything it started and drops the database.
//
// Usage, from `client/`: `BMO_WEB_SERVER_BIN=<the bmo-web-server binary> bun tests/support/browser-console.ts`.

import { spawn, type ChildProcess } from "node:child_process";
import { appendFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { SQL } from "bun";

import { connectionOf } from "../../apps/web/server/utils/db";
import { addressOf } from "./web-database";

const CLIENT = new URL("../..", import.meta.url).pathname;
const CONSOLE_DIR = join(CLIENT, "apps/web");
const SERVER_DIR = join(CLIENT, "../server");
const TOKEN = "browser-console-token-0123456789";

// The accounts the console-driven tests sign in with, each by the name and password its file names.
const ACCOUNTS: readonly [string, string][] = [
  ["agent-topbar", "fixture-password-1558"],
  ["bubble-wrap", "fixture-password-1477"],
  ["console-ids", "fixture-password-789"],
  ["prompt-wrap", "fixture-password-1575"],
  ["room-feed", "fixture-password-1457"],
  ["skeleton259", "fixture-password-259"],
  ["topbar-square", "fixture-password-1444"]
];

// The owner's room, which the receiver's stand-in names as theirs and lists.
const OWNER_CHAT = 42;
// The base path the console is served under; the tests read theirs off the address they are given.
const BASE = "/console/";

const binary = process.env.BMO_WEB_SERVER_BIN?.trim() ?? "";

if (binary === "") {
  throw new Error("BMO_WEB_SERVER_BIN names no bmo-web-server binary");
}

const scratch = mkdtempSync(join(tmpdir(), "bmo-web-browser-console-"));
const children: ChildProcess[] = [];
const servers: { stop: (force?: boolean) => void }[] = [];
const database = `bmo_web_browser_console_${process.pid}`;
const databaseUrl = addressOf(database);

// A port nothing listens on now, taken by listening once and letting go.
function freePort(): number {
  const probe = Bun.serve({ port: 0, hostname: "127.0.0.1", fetch: () => new Response(null) });
  const port = probe.port;

  probe.stop(true);

  if (port === undefined) {
    throw new Error("no port was given to listen on");
  }

  return port;
}

// Each process heads a group of its own, so stopping it stops what it started; what it says goes to a log of
// its own in the scratch folder.
function start(command: string, args: string[], env: Record<string, string>, cwd: string, name: string): ChildProcess {
  const log = join(scratch, `${name}.log`);
  const child = spawn(command, args, { cwd, env: { ...process.env, ...env }, detached: true, stdio: ["ignore", "pipe", "pipe"] });

  child.stdout?.on("data", (chunk) => appendFileSync(log, chunk));
  child.stderr?.on("data", (chunk) => appendFileSync(log, chunk));
  children.push(child);

  return child;
}

async function run(command: string, args: string[], env: Record<string, string>, cwd: string): Promise<void> {
  const ran = Bun.spawn([command, ...args], { cwd, env: { ...process.env, ...env }, stdout: "pipe", stderr: "pipe" });

  if ((await ran.exited) !== 0) {
    throw new Error(`${command} ${args.join(" ")} ended ${ran.exitCode}: ${await new Response(ran.stderr).text()}`);
  }
}

async function answering(what: string, address: string, seconds: number): Promise<void> {
  const end = Date.now() + seconds * 1000;

  while (Date.now() < end) {
    try {
      if ((await fetch(address)).ok) {
        return;
      }
    } catch {
      // Not listening yet.
    }

    await Bun.sleep(500);
  }

  throw new Error(`${what} did not answer at ${address} within ${seconds}s; its log is in ${scratch}`);
}

// An event stream that stays open and says nothing.
function silentStream(): Response {
  return new Response(new ReadableStream({ start: (controller) => controller.enqueue(new TextEncoder().encode(": open\n\n")) }), {
    headers: { "content-type": "text/event-stream", "cache-control": "no-cache" }
  });
}

function agent(name: string, kind: string, label: string, extra: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    name, kind, label, role: null, sessionScope: kind === "chat" ? "agent" : null, turnShape: kind === "chat" ? "window" : null,
    defaultAnswerer: kind === "chat", model: null, configuredMode: null, configuredLabel: null, modelLabel: null,
    lastMode: null, lastModel: null, lastEffort: null, aliases: null, icon: null, cwd: null, builtin: true,
    chatId: kind === "chat" ? OWNER_CHAT : null, runs: 0, lastRunAt: null, lastKeepaliveAt: null, contextTokens: null,
    keepaliveStopped: false, keepalivePausedAt: null, ...extra
  };
}

// One model file as the agency's model resource serves it, and the catalogue as a whole.
function catalogueEntry(name: string, engine: string, registration: string): Record<string, unknown> {
  return {
    name, label: `Entry ${name}`, file: `${name}.gguf`, bytes: 1_000_000_000, sha256: "a".repeat(64),
    source: "https://example.invalid/model.gguf", engine, registration, nodes: ["node-a"], note: null,
    thinking: false, temperature: 0.7, topP: 0.9, presencePenalty: 0, maxTokens: 4_096, serving: false
  };
}

// The registered models and model files the agency's stand-in holds, so the two model sections of the
// agents screen render their lists and their changes as they would against the service.
const models = new Map<string, Record<string, unknown>>([
  ["model-a", { key: "registered::model-a", provider: "registered", model: "model-a-7b", tier: "large", healthy: true }],
  ["model-b", { key: "registered::model-b", provider: "registered", model: "model-b-7b", tier: null, healthy: false }]
]);
const entries = new Map<string, Record<string, unknown>>([
  ["entry-a", catalogueEntry("entry-a", "engine-a", "model-a")],
  ["entry-b", catalogueEntry("entry-b", "engine-b", "model-b")]
]);
const uses: Record<string, unknown>[] = [];

const catalogueView = () => ({ entries: [...entries.values()], uses });
const notFound = () => Response.json({ code: "not-found", error: "not served by this stand-in" }, { status: 404 });

// What each unit's stand-in answers, read from that unit's documented routes; anything else is the unit's own
// not-found. The tests serve the rest of what they measure through the page's routes themselves.
const UNITS: Record<string, (path: string, request: Request) => Response | null | Promise<Response | null>> = {
  AGENCY: async (path, request) => {
    if (path === "/agents") {
      return Response.json({ agents: [agent("agent-a", "chat", "Agent A"), agent("worker", "worker", "Worker", { cwd: "/srv/checkouts/worker", checkout: true })], roles: [] });
    }

    if (path === "/events") return silentStream();
    if (path === "/pool") return Response.json({ readAt: Date.now(), workers: [], buckets: [] });
    if (path === "/fallback") {
      return Response.json({ active: { provider: "claude-code", account: null, accountName: "", accounts: 0, session: null, week: null }, codexPresent: false, keyPresent: false, services: [], since: null, order: [], orderIsDefault: true, accounts: [], profiles: [] });
    }

    if (path === "/models" && request.method === "GET") return Response.json({ items: [...models.values()], more: false });

    if (path === "/models" && request.method === "POST") {
      const body = (await request.json()) as { key?: string; model?: string };
      const key = body.key ?? "";
      const name = key.slice(key.indexOf("::") + 2);

      if (models.has(name)) return Response.json({ code: "conflict", error: "a model is already registered" }, { status: 409 });

      const made = { key, provider: "registered", model: body.model ?? "", tier: null, healthy: false };

      models.set(name, made);

      return Response.json(made, { status: 201 });
    }

    const one = /^\/models\/([^/]+)$/.exec(path);

    if (one !== null) {
      const name = decodeURIComponent(one[1]!);
      const held = models.get(name);

      if (held === undefined) return notFound();

      if (request.method === "PATCH") {
        const body = (await request.json()) as { model?: string };

        models.set(name, { ...held, model: body.model ?? held.model });

        return Response.json(models.get(name));
      }

      if (request.method === "DELETE") {
        models.delete(name);

        return new Response(null, { status: 204 });
      }
    }

    if (path === "/model-catalogue" && request.method === "GET") return Response.json(catalogueView());

    if (path === "/model-catalogue" && request.method === "POST") {
      const body = (await request.json()) as Record<string, unknown>;
      const name = String(body.name ?? "");

      if (body.action === "save") entries.set(name, { ...catalogueEntry(name, String(body.engine ?? "engine-a"), String(body.registration ?? "model-a")), ...body, name });
      if (body.action === "remove") entries.delete(name);
      if (body.action === "use") {
        uses.push({ id: uses.length + 1, entry: name, engine: entries.get(name)?.engine ?? "engine-a", chosenBy: "tester", startedAt: Math.floor(Date.now() / 1000), endedAt: Math.floor(Date.now() / 1000), state: "done", step: null, reason: null, log: null, logTail: [] });
      }

      return Response.json(catalogueView());
    }

    return null;
  },
  JOB: (path) => {
    if (path === "/events") return silentStream();
    if (path === "/todos" || path === "/jobs/recent") return Response.json([]);
    if (path === "/queue/holds") return Response.json({ paused: false });

    return null;
  },
  RECEIVER: (path) => {
    const now = Math.floor(Date.now() / 1000);

    if (path === "/owner") return Response.json({ chat: OWNER_CHAT });
    if (path === "/events") return silentStream();
    if (path === "/live") return Response.json({ at: now, compacting: [] });
    if (path === "/console/rooms") return Response.json({ rooms: [{ chatId: OWNER_CHAT, title: null, messageCount: 0, lastMessageAt: null, pinned: false }] });
    if (/^\/console\/rooms\/-?\d+\/title$/.test(path)) return Response.json({ title: null });
    if (/^\/console\/rooms\/-?\d+\/messages/.test(path)) return Response.json({ messages: [], hasMore: false });
    if (/^\/console\/rooms\/-?\d+\/day-start$/.test(path)) return Response.json({ messageId: null });

    const figures = /^\/console\/rooms\/(-?\d+)\/figures$/.exec(path);

    return figures === null
      ? null
      : Response.json({
          chatId: Number(figures[1]), messages: 12, fromPeople: 7, speakers: 1, firstAt: now - 86_400, lastAt: now - 60, lastAnswerAt: now - 120,
          top: [], byHour: Array.from({ length: 24 }, (_, hour) => (hour % 6 === 0 ? 2 : 0)), agents: []
        });
  },
  BRAIN: () => null,
  ARTIFACT: () => null,
  TELEGRAM: () => null,
  CACHE_SIREN: () => null,
  CACHE_WARMER: (path) => path === "/round" ? Response.json({ stopped: false }) : null
};

let stopping = false;

async function stop(): Promise<void> {
  if (stopping) {
    return;
  }

  stopping = true;

  for (const server of servers) server.stop(true);
  for (const child of children) {
    try {
      process.kill(-child.pid!, "SIGTERM");
    } catch {
      // Already gone.
    }
  }

  await Bun.sleep(1500);

  for (const child of children) {
    try {
      process.kill(-child.pid!, "SIGKILL");
    } catch {
      // Already gone.
    }
  }

  const admin = new SQL(connectionOf(addressOf("postgres")) as SQL.Options);

  await admin.unsafe(`DROP DATABASE IF EXISTS "${database}" WITH (FORCE)`);
  await admin.close();
  process.exit(0);
}

process.on("SIGINT", () => void stop());
process.on("SIGTERM", () => void stop());

try {
  const units: Record<string, string> = {};

  for (const [unit, answer] of Object.entries(UNITS)) {
    const server = Bun.serve({
      port: 0,
      hostname: "127.0.0.1",
      idleTimeout: 0,
      fetch: async (request) => (await answer(new URL(request.url).pathname, request)) ?? notFound()
    });

    servers.push(server);
    units[`${unit}_ADDRESS`] = `http://127.0.0.1:${server.port}`;
    units[`${unit}_TOKEN`] = TOKEN;
  }

  const admin = new SQL(connectionOf(addressOf("postgres")) as SQL.Options);

  await admin.unsafe(`CREATE DATABASE "${database}"`);
  await admin.close();

  const serverPort = freePort();
  const serverEnv = {
    BMO_WEB_SERVER_DATABASE__URL: databaseUrl,
    BMO_WEB_SERVER_SERVER__TOKEN: TOKEN,
    BMO_WEB_SERVER_SERVER__LISTEN: `127.0.0.1:${serverPort}`,
    BMO_WEB_SERVER_NAMES__CHAT_AGENT: "agent-a"
  };

  await run(binary, ["migrate"], serverEnv, SERVER_DIR);
  start(binary, ["serve"], serverEnv, SERVER_DIR, "server");
  await answering("the account service", `http://127.0.0.1:${serverPort}/health`, 30);

  const consolePort = freePort();
  const consoleEnv = {
    ...units,
    NUXT_SESSION_PASSWORD: "browser-console-session-password-0123456789",
    WEB_DATABASE_URL: databaseUrl,
    WEB_SERVER_ADDRESS: `http://127.0.0.1:${serverPort}`,
    WEB_SERVER_TOKEN: TOKEN,
    WEB_DATA_DIR: join(scratch, "data"),
    NUXT_PUBLIC_CHAT_AGENT_NAME: "agent-a",
    NUXT_PUBLIC_CHAT_AGENT_LABEL: "Agent A",
    KEEPALIVE_CONSUMER: "console",
    DEPLOY_PROGRESS_FILE: join(scratch, "deploy-progress.json"),
    // A build folder of its own inside the checkout, so a typecheck beside it leaves what it serves alone and its
    // generated modules still resolve the console's packages.
    BMO_WEB_BUILD_DIR: ".nuxt-browser-tests",
    BMO_WEB_BASE_URL: BASE
  };

  for (const [name, password] of ACCOUNTS) {
    await run("bun", ["scripts/user.ts", "set", name, password, "--administrator"], consoleEnv, CONSOLE_DIR);
  }

  start("bun", ["--bun", "nuxi", "dev", "--port", String(consolePort), "--host", "127.0.0.1"], consoleEnv, CONSOLE_DIR, "console");

  const origin = `http://127.0.0.1:${consolePort}${BASE.replace(/\/$/, "")}`;

  await answering("the console", `${origin}/login`, 240);
  console.log(origin);
} catch (error) {
  console.error(error);
  await stop();
}
