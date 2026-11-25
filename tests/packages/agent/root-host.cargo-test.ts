import "@agency/test-support/setup";

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { Archive } from "@agency/archive/archive";
import { installPlacer, spawnAgentProcess } from "@agency/agent/agent-process";
import { installProviderSettings } from "@agency/agent/provider-order";
import { MCP_TOKEN_VARIABLE, ToolHostDown, composeToolRecord, issueToolToken } from "@agency/agent/tool-host";
import declaredRoles from "@agency/policy/roles.toml";
import { workspaceBinary } from "@agency/test-support/binaries";
import { scratchDir } from "@agency/test-support/scratch";

import type { Placer } from "@agency/agent/pane-process";
import { scratchArchive, scratchDatabase } from "@agency/test-support/store";

// A line standing on the hosts (docs/agent-control.md, 「The host」), as a launcher
// meets them: the data root's agent host, the real binary on a scratch root, and a stand-in for the
// tool host's issuing socket that hands out tokens and notes what it was told. The worker the runs are
// issued for holds the worker role, whose bundles are all the worker line's runs are given.

const data = join(scratchDir("bmo-root-host-"), "data");
const sockets = scratchDir("bmo-root-host-sockets-");
const told: { issued: Record<string, unknown>[]; revoked: string[] } = { issued: [], revoked: [] };
const previous = { data: process.env.BMO_DATA_DIR, socket: process.env.BMO_MCP_HOST_SOCKET, issuer: process.env.BMO_MCP_HOST_ISSUER_TOKEN };
const archive = await scratchArchive();

// The bundles a role gives, as the roles' policy names them.
function bundlesOf(role: string | null): readonly string[] {
  return (declaredRoles.roles as { name: string; bundles: string[] }[]).find((one) => one.name === role)?.bundles ?? [];
}
let toolHost: ReturnType<typeof Bun.serve>;
let agentHost: ReturnType<typeof Bun.spawn>;

beforeAll(async () => {
  mkdirSync(join(data, "executions"), { recursive: true });
  process.env.BMO_DATA_DIR = data;
  process.env.BMO_MCP_HOST_SOCKET = join(sockets, "mcp-host.sock");
  process.env.BMO_MCP_HOST_ISSUER_TOKEN = "stand-in-issuer";
  toolHost = Bun.serve({
    unix: process.env.BMO_MCP_HOST_SOCKET,
    async fetch(request) {
      const path = new URL(request.url).pathname;

      if (request.method === "POST" && path === "/tool-sessions") {
        const record = (await request.json()) as Record<string, unknown>;
        const token = `token-${told.issued.length + 1}`;

        told.issued.push(record);
        return Response.json(
          {
            token,
            holderName: record.holderName ?? "",
            namespaces: record.namespaces ?? [],
            capabilities: record.capabilities ?? [],
            grants: record.grants ?? [],
            shown: record.shown ?? [],
            variables: record.variables ?? {},
            variablesFile: record.variablesFile ?? null,
            labels: record.labels ?? {},
            expiresAt: null,
            createdAt: Date.now(),
          },
          { status: 201 },
        );
      }

      if (request.method === "DELETE" && path.startsWith("/tool-sessions/")) {
        told.revoked.push(decodeURIComponent(path.slice("/tool-sessions/".length)));
        return new Response(null, { status: 204 });
      }

      return new Response(null, { status: 404 });
    },
  });
  agentHost = Bun.spawn([workspaceBinary("agent-host"), "serve", "--data-root", data, "--code-root", data, "--poll-ms", "10"], { stdout: "ignore", stderr: "ignore" });
  installProviderSettings(archive);
  composeToolRecord((run) => ({ granted: bundlesOf(run.agent === "worker" ? "worker" : null) }));

  for (let i = 0; i < 200 && !existsSync(join(data, "executions", "agent-host", "claim")); i++) await Bun.sleep(25);
  // The guard is against a hang, never a wait: the agent host is built first, from nothing where the build
  // directory holds none of it yet.
}, 900_000);

afterAll(async () => {
  agentHost.kill();
  await agentHost.exited;
  toolHost.stop(true);
  installProviderSettings(null);
  installPlacer(null);
  composeToolRecord(() => ({}));
  process.env.BMO_DATA_DIR = previous.data;
  if (previous.socket === undefined) delete process.env.BMO_MCP_HOST_SOCKET; else process.env.BMO_MCP_HOST_SOCKET = previous.socket;
  if (previous.issuer === undefined) delete process.env.BMO_MCP_HOST_ISSUER_TOKEN; else process.env.BMO_MCP_HOST_ISSUER_TOKEN = previous.issuer;
});

describe("a structured run on a line standing on the hosts", () => {
  test("is issued a token, runs under the data root's agent host, shows in its agent's view and gives the token back", async () => {
    const cwd = scratchDir("bmo-root-host-checkout-");
    const proc = await spawnAgentProcess(
      { args: [process.execPath, "-e", `process.stdout.write(process.env.${MCP_TOKEN_VARIABLE} ?? "none")`], cwd, env: { BMO_TURN_CHAT: "-100" }, stdin: null },
      "worker",
      "claude-code",
      { agent: "worker" },
    );
    const [stdout, exit] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
    const token = stdout;

    expect(exit).toBe(0);
    expect(token).toStartWith("token-");
    expect(told.issued.at(-1)).toMatchObject({
      holderName: "worker",
      namespaces: ["web", "worker", "bmo"],
      capabilities: ["queue.read", "web.read", "brain.lessons"],
      variables: { BMO_DATA_DIR: data, BMO_TURN_CHAT: "-100" },
      labels: { line: "worker", checkout: cwd },
    });

    for (let i = 0; i < 100 && !told.revoked.includes(token); i++) await Bun.sleep(20);
    expect(told.revoked).toContain(token);

    // The host wrote its beginning and, once the collector had it, its rest; the collector its end.
    const view = join(data, "executions", "agent-host", "views", "worker");
    let lines: string[] = [];

    for (let i = 0; i < 100 && !lines.some((line) => line.startsWith("resting")); i++) {
      await Bun.sleep(20);
      lines = existsSync(view) ? readFileSync(view, "utf8").trim().split("\n") : [];
    }

    expect(lines[0]).toMatch(/^── \d\d:\d\d worker · claude-code · attempt 1 \(.+\) started$/);
    expect(lines.some((line) => /^── \d\d:\d\d worker · claude-code · attempt 1 ended$/.test(line))).toBe(true);
    expect(lines.at(-1)).toStartWith("resting");
  }, 60_000);

  test("waits for a tool host that does not answer yet, placing nothing until it answers", async () => {
    let placed = 0;
    const counting: Placer = async () => {
      placed += 1;
      return { ok: false, error: { code: "unused", message: "unused" } };
    };
    const socket = process.env.BMO_MCP_HOST_SOCKET;
    const late = join(sockets, "late-launch.sock");
    let host: ReturnType<typeof Bun.serve> | null = null;

    installPlacer(counting);
    process.env.BMO_MCP_HOST_SOCKET = late;

    try {
      const proc = await spawnAgentProcess({ args: [process.execPath, "-e", "0"], cwd: data, env: {}, stdin: null }, "worker", "claude-code", { agent: "worker" });

      await Bun.sleep(800);
      expect(placed).toBe(0);

      host = Bun.serve({ unix: late, fetch: () => Response.json({ token: "late-token" }) });
      await proc.exited.catch(() => {});
      expect(placed).toBe(1);
    } finally {
      installPlacer(null);
      process.env.BMO_MCP_HOST_SOCKET = socket;
      (host as ReturnType<typeof Bun.serve> | null)?.stop(true);
    }
  }, 30_000);
});

test("a host not answering yet is waited for until it answers", async () => {
  const late = join(sockets, "late.sock");
  const record = { holderName: "worker", namespaces: ["web", "worker"], capabilities: [], grants: [] };
  let host: ReturnType<typeof Bun.serve> | null = null;
  const coming = Bun.sleep(600).then(() => {
    host = Bun.serve({ unix: late, fetch: () => Response.json({ token: "late-token" }) });
  });

  try {
    expect(await issueToolToken(record, { socket: late })).toBe("late-token");
  } finally {
    await coming;
    (host as ReturnType<typeof Bun.serve> | null)?.stop(true);
  }
});

test("a host that answers and refuses the record is said at once, not waited for", async () => {
  const refusing = Bun.serve({ unix: join(sockets, "refusing.sock"), fetch: () => Response.json({ code: "bad-request", error: "unknown field" }, { status: 400 }) });
  const record = { holderName: "worker", namespaces: ["web", "worker"], capabilities: [], grants: [] };

  try {
    const began = Date.now();

    expect(issueToolToken(record, { socket: join(sockets, "refusing.sock") })).rejects.toBeInstanceOf(ToolHostDown);
    await issueToolToken(record, { socket: join(sockets, "refusing.sock") }).catch(() => {});
    expect(Date.now() - began).toBeLessThan(2_000);
  } finally {
    refusing.stop(true);
  }
});
