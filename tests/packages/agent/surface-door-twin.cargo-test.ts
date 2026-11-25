import "@agency/test-support/setup";

import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, describe, expect, setDefaultTimeout, test } from "bun:test";

import { identityAlive } from "@agency/agent/agent-host";
import { agentPlace, readRun } from "@agency/agent/agent-place";
import { endpointPlaceSettings } from "@agency/agent/claude-code";
import { endpointTurn } from "@agency/agent/claude-endpoint-turn";
import { cliProjectsRoot } from "@agency/agent/cli-transcripts";
import { codexResident } from "@agency/agent/codex";
import { residentCodexTurn } from "@agency/agent/codex-resident";
import { executionCancelKey } from "@agency/agent/execution-identity";
import { ExecutionStopped } from "@agency/agent/pane-process";
import { installProviderSettings } from "@agency/agent/provider-order";
import { readProgram, readRunning, residentCommand } from "@agency/agent/resident-pane";
import { headCommand } from "@agency/agent/units";
import { Archive } from "@agency/archive/archive";
import { workspaceBinary } from "@agency/test-support/binaries";
import { codeRoot } from "@agency/support/roots";
import { scratchDir } from "@agency/test-support/scratch";
import { toolHostStandIn } from "@agency/test-support/tool-host-stand-in";
import { answerRecord, standInHerdr } from "@agency/test-support/herdr-standin";

import type { ClaudeEndpoint } from "@agency/agent/claude-code";
import type { HarnessCommand, HarnessRun, ResidentSurface, ResidentTurnRequest } from "@agency/agent/harness";
import type { AgentHome } from "@agency/agent/herdr-run";
import { scratchArchive, scratchDatabase } from "@agency/test-support/store";

// One turn of a surface that carries its own turns — Codex's program read out of its rollout and the
// registry's Claude Code on the gateway — by the TypeScript surface turn kept as the parity reference and
// by agency's Rust door (`bmo-agency resident-turn`) for the same provider (docs/agent-control.md, 「The
// interactive surface a provider declares」). Each scenario stages the
// same program on a stand-in herdr of its own for each side, in a place and a checkout of that side's
// own, and what each side came to — the ending, the run the place names, the program it stood, the
// execution row, what it put into the pane or the inbox and how often the program was started — must be
// the same.

// A program written into a place is handed its session token first, by the tool host stood in here.
const toolHost = toolHostStandIn(scratchDir("bmo-tool-host-"));

afterAll(() => toolHost.stop());

const binary = workspaceBinary("bmo-agency");
const root = scratchDir("bmo-surface-twin-");
const realDataDir = process.env.BMO_DATA_DIR;
// The user's own Codex home, whose login and threads the program's home shares: the bun side is handed
// it, and the Rust side finds it under the home its environment names.
const userHome = join(root, "home");
const sessions = join(userHome, ".codex", "sessions", "2026", "09", "21");
// The registration's own configuration home, where its program's records go.
const registered = join(root, "registered-home");

process.env.BMO_DATA_DIR = root;
mkdirSync(sessions, { recursive: true });

afterAll(() => {
  if (realDataDir === undefined) delete process.env.BMO_DATA_DIR;
  else process.env.BMO_DATA_DIR = realDataDir;

  rmSync(root, { recursive: true, force: true });
});

type Side = "bun" | "rust";
type HerdrOptions = Parameters<typeof standInHerdr>[1];

// What a side came to, in one shape on both: an answer or the reason there is none, a turn stopped from
// outside, or what threw the turn out.
type Came = { reply: unknown } | { error: string } | { stopped: string } | { thrown: string };

interface Ask {
  name: string;
  run: HarnessRun;
}

interface Surface {
  provider: string;
  // The TypeScript side's turn of the surface.
  bun: (request: ResidentTurnRequest) => Promise<Came>;
  // The command its program stands on for this run, as the provider composes it.
  command: (run: HarnessRun, place: string) => Promise<HarnessCommand>;
  clocks: Record<string, number>;
}

const home = (agent: string): AgentHome => ({ agent, label: agent, head: { command: headCommand(agent, codeRoot), cwd: root, env: {} } });

async function bunTurn(surface: Surface, agent: string, socket: string, ask: Ask): Promise<Came> {
  try {
    return await surface.bun({ run: ask.run, home: home(agent), name: ask.name, title: "a talk", socket });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);

    return error instanceof ExecutionStopped ? { stopped: message } : { thrown: message };
  }
}

async function rustTurn(surface: Surface, agent: string, configHome: string, ask: Ask, address: string): Promise<Came> {
  const request = join(root, `${agent}-${ask.name.replaceAll(":", "-")}.json`);
  const command = residentCommand(await surface.command(ask.run, agentPlace(agent)));

  writeFileSync(request, JSON.stringify({
    agent, provider: surface.provider, name: ask.name, title: "a talk",
    run: ask.run, command, ...surface.clocks,
  }));

  const run = Bun.spawn([binary, "resident-turn", request, "--data-root", root], {
    cwd: codeRoot,
    env: { ...process.env, XDG_CONFIG_HOME: configHome, HOME: userHome, BMO_ARCHIVE_URL: address },
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([new Response(run.stdout).text(), new Response(run.stderr).text(), run.exited]);

  expect({ code, err: code === 0 ? "" : err }).toEqual({ code: 0, err: "" });

  const said = JSON.parse(out) as { ending?: Came; thrown?: string };

  return said.ending ?? { thrown: said.thrown ?? "" };
}

// A program a turn ended on its way out, stood again by the stand-in's loop, so what the place holds is
// read once it stands.
async function standing(place: string): Promise<void> {
  for (let look = 0; look < 200; look += 1) {
    const running = readRunning(place);

    if (running === null || identityAlive({ pid: running.pid, birth: running.birth ?? "" })) return;

    await Bun.sleep(10);
  }
}

// What a side came to, with what is that side's own — its agent, its checkout, the clock, the ids a row,
// a thread or a session is given — named the same way on both.
function normalised(value: unknown, agent: string, cwd: string, own: Record<string, string>): unknown {
  let text = JSON.stringify(value)
    .replaceAll(cwd, "<cwd>")
    .replaceAll(agentPlace(agent), "<place>")
    .replaceAll(cwd.replace(/[^a-zA-Z0-9]/g, "-"), "<project>")
    .replace(/ · \d\d:\d\d\]/g, " · <clock>]")
    .replace(/bmo-root-[0-9a-f-]{36}/g, "<root>")
    // The token a program is handed is the tool host's to choose, issued afresh for each side.
    .replace(/"token-\d+"/g, "\"<token>\"")
    // A session a turn opens for itself is named afresh on each side.
    .replace(/[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}/g, "<session>");

  for (const [mine, named] of Object.entries(own)) text = text.replaceAll(mine, named);

  return JSON.parse(text) as unknown;
}

interface Staged {
  archive: Archive;
  agent: string;
  cwd: string;
  side: Side;
}

async function side(
  side: Side,
  base: string,
  surface: Surface,
  herdrOptions: (staged: Staged) => HerdrOptions,
  asks: (staged: Staged) => Ask[],
  extra: { own?: (staged: Staged) => Record<string, string>; beside?: (staged: Staged) => () => unknown } = {},
): Promise<unknown> {
  const archive = await scratchArchive();
  const agent = `${base}-${side}`;
  const cwd = join(root, side, base);
  const staged = { archive, agent, cwd, side };
  const herdr = standInHerdr(`${base}-${side}`, herdrOptions(staged));
  const place = agentPlace(agent);

  mkdirSync(cwd, { recursive: true });
  installProviderSettings(archive);

  // Whatever runs beside the program for the scenario — a loop taking its inbox — and what it saw.
  const beside = extra.beside?.(staged) ?? (() => null);

  try {
    const turns: unknown[] = [];

    for (const ask of asks(staged)) {
      const came = side === "bun" ? await bunTurn(surface, agent, herdr.socket, ask) : await rustTurn(surface, agent, herdr.configHome, ask, archive.path);
      await standing(place);

      const row = (await archive.executions().executions()).find((one) => one.parent === ask.name) ?? null;
      const program = readProgram(place);
      const placeRun = readRun(place);

      turns.push({
        came,
        run: placeRun === null ? null : { name: placeRun.name, title: placeRun.title, person: placeRun.person, env: placeRun.env, ended: placeRun.endedAt !== null },
        program: program === null ? null : { command: program.command, restart: program.restart, opened: program.opened },
        row: row === null ? null : {
          root: row.root, parent: row.parent, role: row.role, model: row.model, effort: row.effort, sessionId: row.sessionId,
          provider: row.provider, line: row.line, state: row.state, panePath: row.panePath, attempt: row.attempt, exitCode: row.exitCode,
          ended: row.endedAt !== null, resident: row.resident === undefined ? null : { place: row.resident.place, record: row.resident.record, from: row.resident.from },
        },
        starts: readRunning(place)?.starts ?? null,
      });
    }

    const put = herdr.asked
      .filter((one) => ["pane.send_text", "agent.prompt", "pane.send_keys", "pane.close"].includes(one.method))
      .map((one) => ({ method: one.method, params: one.params }));

    return normalised({ turns, put, beside: beside() }, agent, cwd, extra.own?.(staged) ?? {});
  } finally {
    installProviderSettings(null);
    herdr.stop();
  }
}

async function twin(base: string, surface: Surface, herdrOptions: (staged: Staged) => HerdrOptions, asks: (staged: Staged) => Ask[], extra: Parameters<typeof side>[5] = {}) {
  const bun = await side("bun", base, surface, herdrOptions, asks, extra);
  const rust = await side("rust", base, surface, herdrOptions, asks, extra);

  expect(rust).toEqual(bun);

  return bun as { turns: { came: Came; starts: number | null; row: { state: string } | null }[]; put: { method: string; params: Record<string, unknown> }[]; beside: unknown };
}

function harnessRun(overrides: Partial<HarnessRun>): HarnessRun {
  return {
    line: "chat", model: "", effort: "", systemPromptPath: null, toolsShown: "", toolsAllowed: null, toolsDenied: null,
    mcpConfigPath: null, prompt: "hello", promptDelivery: "input", resumeSessionId: null, keepSession: true, hostCommands: false,
    thinking: true, answerShape: "text", cwd: root, env: { PATH: "/usr/bin:/bin" }, ...overrides,
  };
}

// A twin starts `bmo-agency` and stands a store of its own on each side, several times in one
// scenario: a second or two alone, many times that on a machine carrying other runs or on a slower one. The bounds
// are guards against a hang, never a wait, so they are wide.
setDefaultTimeout(300_000);

describe("a turn of the registry's Claude Code on the gateway, by either side", () => {
  const model = { home: registered, name: "qwen-test" };
  const endpoint = { provider: "registered", model: async () => model } as unknown as ClaudeEndpoint;
  // The road's program as its provider composes it, on the session the run goes on in or opens.
  const command = async (run: HarnessRun): Promise<HarnessCommand> => ({
    args: ["claude", "--model", "qwen-test", run.resumeSessionId === null ? "--session-id" : "--resume", run.resumeSessionId ?? run.openSessionId ?? "", "--settings", run.resident?.settingsPath ?? ""],
    cwd: run.cwd,
    env: { PATH: "/usr/bin:/bin", CLAUDE_CONFIG_DIR: registered, ANTHROPIC_BASE_URL: "http://[::1]:9/gateway" },
    stdin: null,
  });
  const surface = { command } as unknown as ResidentSurface;
  const road: Surface = {
    provider: "registered",
    bun: (request) => endpointTurn({ ...request, surface, endpoint }),
    command: async (run, place) => {
      const resident = { place, settingsPath: endpointPlaceSettings(run, place, model as never) };

      return await command({ ...run, openSessionId: run.resumeSessionId === null ? crypto.randomUUID() : null, resident });
    },
    clocks: {},
  };
  const run = (staged: Staged, overrides: Partial<HarnessRun> = {}) => harnessRun({ model: "qwen-test", effort: "high", cwd: staged.cwd, ...overrides });
  const herdr = (): HerdrOptions => ({ projects: cliProjectsRoot(registered), answer: async () => [answerRecord({ text: "done", input: 5 })] });

  test("a round opens a session of its own behind the run's head line alone, and a word goes on in the session the program stands on", async () => {
    const continued = "5b1d5a1c-0000-4000-8000-0000000000e1";
    const seen = await twin(
      "road",
      road,
      herdr,
      (staged) => [
        { name: "chat:20", run: run(staged) },
        { name: "worker-word:1", run: run(staged, { line: "worker", resumeSessionId: continued }) },
      ],
    );

    expect(seen.turns.map((one) => ("reply" in one.came ? (one.came.reply as { mode: string; text: string }) : one.came))).toMatchObject([
      { mode: "registered", text: "done" },
      { mode: "registered", text: "done" },
    ]);
    const roundTyped = seen.put
      .filter((one) => one.method === "pane.send_text" && String(one.params.text).startsWith("[chat:20 ·"))
      .map((one) => String(one.params.text));

    expect(roundTyped).toHaveLength(1);
    expect(roundTyped[0]).toBe("[chat:20 · a talk · <clock>] ");
    expect(roundTyped[0]).not.toContain("The text pasted after this line");
  }, 300_000);

  test("a turn that wrote the program's own error is that error", async () => {
    const failed = { type: "assistant", isApiErrorMessage: true, message: { id: "e", model: "<synthetic>", content: [{ type: "text", text: "API Error: 529 Overloaded" }] } };
    const seen = await twin(
      "road-errored",
      road,
      () => ({ projects: cliProjectsRoot(registered), answer: async () => [failed] }),
      (staged) => [{ name: "worker-word:2", run: run(staged, { line: "worker", resumeSessionId: "5b1d5a1c-0000-4000-8000-0000000000e2" }) }],
    );

    expect(seen.turns[0]?.came).toEqual({ error: "API Error: 529 Overloaded" });
  }, 300_000);
});
