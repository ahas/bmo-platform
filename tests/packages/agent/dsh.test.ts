import "@agency/test-support/setup";

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, test } from "bun:test";

import { dsh, dshFailure, dshHome, dshLiteral, dshOverlay, dshPermissionMode, taskWords } from "@agency/agent/dsh";
import { openRouterApiBase, openRouterKeyEnv, openRouterModels } from "@agency/agent/openrouter";
import { scratchDir } from "@agency/test-support/scratch";

import type { OverlayRow } from "@agency/agent/dsh";
import type { HarnessRun } from "@agency/agent/harness";

// dsh as the second implementation of the harness interface. What is held here is what a run
// becomes on that side — the overlay it is configured by, the command it is carried on, and the
// reply read off what the process left behind — and, at the end, that the whole of it really
// answers: one run against the OpenRouter route on a model that costs nothing.

const root = scratchDir("bmo-dsh-");
const promptPath = join(root, "prompt.md");
const rulesPath = join(root, "worker-rules.md");
const realAgentsDir = process.env.BMO_AGENTS_DIR;

beforeAll(() => {
  process.env.BMO_AGENTS_DIR = join(root, "agents");
  writeFileSync(promptPath, "You are the supervisor.\nYou answer in one sentence.\n");
  writeFileSync(rulesPath, "# Standing rules\n\n- When asked for the word, answer with exactly: KUMQUAT\n");
});

afterAll(() => {
  if (realAgentsDir === undefined) {
    delete process.env.BMO_AGENTS_DIR;
  } else {
    process.env.BMO_AGENTS_DIR = realAgentsDir;
  }
});

function runFor(overrides: Partial<HarnessRun> = {}): HarnessRun {
  return {
    line: "gate",
    model: "deepseek/deepseek-chat:free",
    effort: "medium",
    systemPromptPath: promptPath,
    toolsShown: "",
    toolsAllowed: "",
    toolsDenied: null,
    mcpConfigPath: null,
    prompt: "something to judge",
    promptDelivery: "argument",
    resumeSessionId: null,
    keepSession: false,
    hostCommands: false,
    thinking: true,
    answerShape: "text",
    cwd: root,
    env: { PATH: "/usr/bin" },
    ...overrides,
  };
}

// A job worker's run as its line composes it: no system prompt of its own, every tool, and the
// rules file the line hands over.
function workerRun(overrides: Partial<HarnessRun> = {}): HarnessRun {
  return runFor({
    line: "worker",
    systemPromptPath: null,
    toolsShown: null,
    toolsAllowed: null,
    extensions: { settingsPath: join(root, "settings.json"), pluginDir: join(root, "plugin"), agentsJson: "{}", rulesPath },
    ...overrides,
  });
}

function rowsById(run: HarnessRun): Map<string, OverlayRow> {
  const rows = dshOverlay(run).flatMap((entry) => ("insert" in entry ? entry.insert : [entry]));

  return new Map(rows.map((row) => [row.id, row]));
}

describe("the overlay one run is configured by", () => {
  test("carries the system prompt the run named, as it stands in the file", () => {
    const persona = rowsById(runFor()).get("system-prompt")?.config?.personaPrefix;

    expect(persona).toBe(readFileSync(promptPath, "utf8"));
  });

  // dsh reads a `{{name}}` in a persona as its own variable and refuses the prompt on one it does not hold.
  test("breaks every pair of braces in the text it hands over, and keeps the opening line's own variable", () => {
    expect(dshLiteral("Keep {{name}} and a {lone} brace")).toBe("Keep {\u2060{name}} and a {lone} brace");

    const rulesWithMarks = join(root, "rules-with-marks.md");

    writeFileSync(rulesWithMarks, "Fill {{this}} later.\n");

    const persona = rowsById(workerRun({ extensions: { settingsPath: join(root, "settings.json"), pluginDir: join(root, "plugin"), agentsJson: "{}", rulesPath: rulesWithMarks } }))
      .get("system-prompt")?.config?.personaPrefix as string;

    expect(persona).toContain("{{model}}");
    expect(persona.endsWith("Fill {\u2060{this}} later.\n")).toBe(true);
  });

  test("hands a job the worker's rules after the headless opening line, and keeps its instruction files", () => {
    const rows = rowsById(workerRun());
    const persona = rows.get("system-prompt")?.config?.personaPrefix as string;

    expect(persona.startsWith("You are a coding agent powered by the {{model}} model.")).toBe(true);
    expect(persona.endsWith(`\n\n${readFileSync(rulesPath, "utf8")}`)).toBe(true);
    expect(rows.has("agent-instructions")).toBe(false);
    expect(() => dshOverlay(workerRun({ extensions: undefined }))).toThrow("no rules");
  });

  test("names the model and the route it runs on, and the key by variable rather than by value", () => {
    const rows = rowsById(runFor());

    expect(rows.get("agent-default-model")?.config).toEqual({
      provider: "openrouter",
      model: "deepseek/deepseek-chat:free",
    });
    expect(rows.get("llm-pi-ai")?.config).toEqual({
      providers: {
        openrouter: {
          apiKeyEnv: openRouterKeyEnv,
          baseURL: openRouterApiBase(),
          api: "openai-completions",
          models: [{ id: "deepseek/deepseek-chat:free" }],
        },
      },
    });

    // Whatever else is in the file, the key is not: the route says where to read it from, and a
    // file anyone who can read the directory can read says nothing on its own.
    expect(JSON.stringify(dshOverlay(runFor()))).not.toContain("sk-or");
    expect(JSON.stringify(dshOverlay(runFor()))).toContain(openRouterKeyEnv);
  });

  test("an Anthropic model asks for the one-hour cache, and no other model or route is told anything of it", () => {
    expect(rowsById(runFor({ model: "anthropic/claude-sonnet-5" })).get("llm-pi-ai")?.config).toEqual({
      providers: {
        openrouter: {
          apiKeyEnv: openRouterKeyEnv,
          baseURL: openRouterApiBase(),
          api: "openai-completions",
          cacheRetention: "long",
          models: [{ id: "anthropic/claude-sonnet-5", compat: { supportsLongCacheRetention: true } }],
        },
      },
    });
    expect(JSON.stringify(rowsById(runFor()).get("llm-pi-ai"))).not.toContain("cacheRetention");
    expect(JSON.stringify(rowsById(runFor({ model: "qwen3.8-27b", env: { BMO_DSH_LOCAL_URL: "http://[::1]:1/v1" } })).get("llm-pi-ai"))).not.toContain("LongCache");
  });

  // dsh refuses a model asked for a level it does not offer, and which levels a stand-in offers
  // is not something agency can know, so a run that thinks names none of its own and takes the model's.
  test("a run that must not think names off, and one that thinks names no level of its own", () => {
    expect(rowsById(runFor({ thinking: false })).get("llm-pi-ai")?.config).toMatchObject({
      providers: { openrouter: { reasoning: "off" } },
    });
    expect(JSON.stringify(rowsById(runFor({ effort: "max" })).get("llm-pi-ai"))).not.toContain("reasoning");
  });

  // An agent's own level is the one level the run declares its model offers, and asks for, which
  // goes out as OpenRouter's reasoning.effort (docs/model-tiers.md, 「An agent's own thinking and effort」).
  test("a run at an agent's own level declares that level on its model and asks for it", () => {
    const config = rowsById(runFor({ model: "anthropic/claude-sonnet-5", effort: "high", reasoning: { thinking: null, effort: "high" } })).get("llm-pi-ai")?.config;

    expect(config).toMatchObject({ providers: { openrouter: { reasoning: "high" } } });
    expect((config as { providers: { openrouter: { models: unknown[] } } }).providers.openrouter.models).toEqual([
      { id: "anthropic/claude-sonnet-5", reasoningEfforts: { off: null, high: "high" }, compat: { thinkingFormat: "openrouter", supportsLongCacheRetention: true } },
    ]);
    expect(rowsById(runFor({ thinking: false, reasoning: { thinking: false, effort: "high" } })).get("llm-pi-ai")?.config).toMatchObject({
      providers: { openrouter: { reasoning: "off", models: [{ id: "deepseek/deepseek-chat:free" }] } },
    });
  });

  test("a run shown nothing has every tool off; one shown something keeps the working set and fences the rest", () => {
    const none = rowsById(runFor());

    expect(none.get("tool-bash")?.disabled).toBe(true);
    expect(none.get("tool-fs")?.disabled).toBe(true);
    // The plan mode puts a tool in front of the model on its own account, so it goes with them.
    expect(none.get("plan-mode")?.disabled).toBe(true);
    // Nothing shown, nothing to fence: the sandbox is left as the profile mounts it.
    expect(none.has("sandbox-policy")).toBe(false);

    const own = rowsById(runFor({ toolsShown: "Bash,Read,Write,Edit", toolsAllowed: "Bash,Read,Write,Edit", cwd: root }));

    // The shell and the filesystem stay on — no disable row for them — and the agentic rows go off.
    expect(own.has("tool-bash")).toBe(false);
    expect(own.has("tool-fs")).toBe(false);
    expect(own.has("tool-str-replace-editor")).toBe(false);
    expect(own.get("tool-workflow")?.disabled).toBe(true);
    expect(own.get("tool-web")?.disabled).toBe(true);
    expect(own.get("plan-mode")?.disabled).toBe(true);
    // Allowing exactly what it shows writes within the run's own directory.
    expect(own.get("sandbox-policy")?.config).toEqual({ mode: "workspace-write", workspaceRoot: root });
  });

  // A run shown file tools and no shell gets the filesystem rows and the shell stays off: whatever it
  // writes is a file tool's write, fenced to the directory the run works in.
  test("a run shown file tools alone keeps the filesystem, has no shell, and writes only within its own directory", async () => {
    const tools = "Read,Write,Edit,Glob,Grep";
    const rows = rowsById(runFor({ line: "chat", systemPromptPath: null, toolsShown: tools, toolsAllowed: tools, cwd: root }));

    expect(rows.get("tool-bash")?.disabled).toBe(true);
    expect(rows.get("tool-pwsh")?.disabled).toBe(true);
    expect(rows.get("tool-jobs")?.disabled).toBe(true);
    expect(rows.has("tool-fs")).toBe(false);
    expect(rows.has("tool-fs-search")).toBe(false);
    expect(rows.has("tool-str-replace-editor")).toBe(false);
    expect(rows.get("tool-subagent")?.disabled).toBe(true);
    expect(rows.get("tool-web")?.disabled).toBe(true);
    expect(rows.get("agent-instructions")?.disabled).toBe(true);
    expect(rows.get("sandbox-policy")?.config).toEqual({ mode: "workspace-write", workspaceRoot: root });
    expect((await dsh.command(runFor({ line: "chat", systemPromptPath: null, toolsShown: tools, toolsAllowed: tools, cwd: root }))).env.DSH_PERMISSION_MODE).toBe("workspace-write");
  });

  test("a run that allows less than it shows reads but does not write", () => {
    const rows = rowsById(runFor({ toolsShown: "Bash,Read,Write,Edit", toolsAllowed: "Read", cwd: root }));

    // The shell is still shown, but the fence is read-only, so its file writes are denied.
    expect(rows.get("tool-bash")).toBeUndefined();
    expect(rows.get("sandbox-policy")?.config).toEqual({ mode: "read-only", workspaceRoot: root });
  });

  test("nothing of the machine the run stands on is read", () => {
    const rows = rowsById(runFor());

    // The instruction files lying around the working directory, and the second model call that
    // would title a session nobody opens.
    expect(rows.get("agent-instructions")?.disabled).toBe(true);
    expect(rows.get("session-title-llm")?.disabled).toBe(true);
  });

  test("an MCP server becomes a client of its own, named as the file named it", () => {
    const configPath = join(root, "worker.mcp.json");

    writeFileSync(
      configPath,
      JSON.stringify({
        mcpServers: {
          web: { type: "stdio", command: "/usr/bin/bun", args: ["scripts/web-mcp.ts"], env: { BMO_WEB_STRATEGY: "direct-then-browser" } },
        },
      }),
    );

    expect(rowsById(runFor({ mcpConfigPath: configPath })).get("mcp-web")).toEqual({
      id: "mcp-web",
      name: "@deepseek-ai/dsh-mcp-client",
      config: {
        transport: "stdio",
        serverName: "web",
        command: "/usr/bin/bun",
        args: ["scripts/web-mcp.ts"],
        env: { BMO_WEB_STRATEGY: "direct-then-browser" },
        cwd: root,
        failOnStartupError: false,
      },
    });

    // The profile holds no row of a client's id, so a client is inserted rather than named as a change.
    const overlay = dshOverlay(runFor({ mcpConfigPath: configPath }));

    expect(overlay.some((entry) => "id" in entry && entry.id.startsWith("mcp-"))).toBe(false);
    expect(overlay.filter((entry) => "insert" in entry).map((entry) => ("insert" in entry ? entry.insert.map((row) => row.id) : []))).toEqual([["mcp-web"]]);
  });

  test("a server on the tool host becomes a Streamable HTTP client, its bearer read from dsh's own environment and its wait past the host's longest call", async () => {
    const configPath = join(root, "worker-hosts.mcp.json");

    writeFileSync(configPath, JSON.stringify({ mcpServers: { worker: { type: "http", url: "http://127.0.0.1:3290/mcp/worker", headers: { Authorization: "Bearer ${BMO_MCP_TOKEN}" } } } }));

    const run = runFor({ mcpConfigPath: configPath });

    expect(rowsById(run).get("mcp-worker")?.config).toMatchObject({ transport: "streamable-http", serverName: "worker", url: "http://127.0.0.1:3290/mcp/worker" });
    expect(Number(rowsById(run).get("mcp-worker")?.config?.toolCallTimeoutMs)).toBeGreaterThan(180_000);

    // The token is never written into the overlay: the header is an expression over dsh's environment.
    const { dshArgs } = launched(await dsh.command(run));
    const overlay = readFileSync(dshArgs[dshArgs.indexOf("--patch") + 1] ?? "", "utf8");

    expect(overlay).toContain(`"Authorization": !!js "\`Bearer \${process.env.BMO_MCP_TOKEN}\`"`);
  });

  test("a run that reaches no server has no client row at all", () => {
    expect([...rowsById(runFor()).keys()].filter((id) => id.startsWith("mcp-"))).toEqual([]);
  });
});

// What dsh is started with once npx has the pinned package on its path: the shell string npx is
// handed, run the way npx runs it, with a stand-in dsh that writes down its own arguments.
function launched(command: { args: string[] }): { dshArgs: string[]; left: boolean; taskPath: string } {
  const call = command.args[command.args.indexOf("--call") + 1] ?? "";
  const bin = join(root, "bin");
  const out = join(root, `dsh-args-${Math.random()}`);
  const taskPath = /'([^']*\.words)'/.exec(call)?.[1] ?? "";

  mkdirSync(bin, { recursive: true });
  writeFileSync(join(bin, "dsh"), `#!/usr/bin/env bash\nprintf '%s\\0' "$@" > "$DSH_ARGS_OUT"\n`, { mode: 0o755 });

  const ran = Bun.spawnSync(["sh", "-c", call], {
    env: { ...process.env, PATH: `${bin}:${process.env.PATH ?? ""}`, DSH_ARGS_OUT: out },
  });

  expect(ran.exitCode).toBe(0);

  return { dshArgs: readFileSync(out, "utf8").split("\0").slice(0, -1), left: existsSync(taskPath), taskPath };
}

describe("the command a run is carried on", () => {
  test("is the pinned version on the headless profile, with the task standing in dsh's line", async () => {
    const command = await dsh.command(runFor());

    expect(command.args.slice(0, 5)).toEqual([
      "npx",
      "--yes",
      "--package",
      // Pinned rather than floating: a release that renamed a row the overlay addresses would be
      // a different harness answering the same command.
      "@deepseek-ai/dsh@0.2.1-rc.2",
      "--call",
    ]);

    const { dshArgs } = launched(command);

    expect(dshArgs.slice(0, 3)).toEqual(["--profile", "headless", "--patch"]);
    expect(dshArgs.at(-1)).toBe("something to judge");
    // headless reads its task from the command line and nothing from its input.
    expect(command.stdin).toBeNull();
  });

  test("continues the session a turn resumes, named on dsh's line", async () => {
    const command = await dsh.command({ ...runFor(), resumeSessionId: "session-a" });
    const { dshArgs } = launched(command);

    expect(dshArgs[dshArgs.indexOf("--session-id") + 1]).toBe("session-a");
  });

  test("writes the overlay where the line points, under agency's own root and not the user's home", async () => {
    const command = await dsh.command(runFor());
    const { dshArgs } = launched(command);
    const overlayPath = dshArgs[dshArgs.indexOf("--patch") + 1] ?? "";

    expect(overlayPath.startsWith(join(dshHome(), "overlays"))).toBe(true);
    expect(dshHome().startsWith(join(root, "agents"))).toBe(true);
    expect(JSON.parse(readFileSync(overlayPath, "utf8"))).toEqual(dshOverlay(runFor()));
    // Everything dsh writes for itself goes under that same root.
    expect(command.env.DSH_HOME).toBe(dshHome());
  });

  // A room's person-book batch runs to a few hundred kilobytes. The kernel starts no process with
  // one argument of 128 KiB or more, npx hands its command to a shell as one string, and headless
  // joins its task words with one space.
  test("carries a task too long for one argument to dsh as words that join back into it", async () => {
    const line = "- Owner: one line for the person book -- and a hyphen after a space\n";
    const prompt = `people ${line.repeat(8_000)}end`;
    const words = taskWords(prompt);
    const command = await dsh.command(runFor({ prompt }));
    const { dshArgs, left, taskPath } = launched(command);

    expect(Buffer.byteLength(prompt)).toBeGreaterThan(300_000);
    expect(command.args.every((arg) => Buffer.byteLength(arg) < 4_096)).toBe(true);
    expect(words.length).toBeGreaterThan(2);
    expect(words.join(" ")).toBe(prompt);
    expect(words.every((word) => Buffer.byteLength(word) < 128 * 1024)).toBe(true);
    expect(words.every((word) => !word.startsWith("-"))).toBe(true);
    expect(dshArgs.slice(4)).toEqual(words);
    // The task file is gone once dsh has its words.
    expect(taskPath.startsWith(join(dshHome(), "tasks"))).toBe(true);
    expect(left).toBe(false);
  });

  test("keeps a short task as the one word it is, and cuts a stretch with no space where it runs out", () => {
    expect(taskWords("something to judge")).toEqual(["something to judge"]);

    const words = taskWords("€".repeat(50_000));

    expect(words.length).toBe(2);
    expect(words.join("")).toBe("€".repeat(50_000));
    expect(words.every((word) => Buffer.byteLength(word) < 128 * 1024)).toBe(true);
  });

  test("quotes the files it names, so a root with a quote or a space in it reaches dsh whole", async () => {
    const home = process.env.BMO_AGENTS_DIR;
    process.env.BMO_AGENTS_DIR = join(root, "owner's agents");

    try {
      const { dshArgs } = launched(await dsh.command(runFor()));

      expect(existsSync(dshArgs[dshArgs.indexOf("--patch") + 1] ?? "")).toBe(true);
    } finally {
      process.env.BMO_AGENTS_DIR = home;
    }
  });

  test("two runs in flight never write over each other's overlay", async () => {
    const first = await dsh.command(runFor());
    const second = await dsh.command(runFor());

    expect(first.args).not.toEqual(second.args);
  });
});

// What a run may do to the files around it, and the one thing that has to travel with it. dsh
// keeps two knobs — how far a write may reach, and whether the run stops to ask before it acts —
// and its permission table names only the pairs they make together. Setting the reach alone
// leaves the asking where the profile left it, and the pair a job composes that way is one the
// table cannot name: dsh refuses to compose at all, in a second, before a word reaches a gateway.
// So the mode is handed over as the one name dsh resolves both ends of.

describe("what a run may do to the files around it", () => {
  test("a job may do anything, a chat writes within its own directory, a restricted room only reads", () => {
    expect(dshPermissionMode(runFor({ line: "worker", toolsShown: null, toolsAllowed: null }))).toBe("danger-full-access");
    expect(dshPermissionMode(runFor({ toolsShown: "Bash,Read,Write,Edit", toolsAllowed: "Bash,Read,Write,Edit" }))).toBe("workspace-write");
    expect(dshPermissionMode(runFor({ toolsShown: "Bash,Read,Write,Edit", toolsAllowed: "Read" }))).toBe("read-only");
  });

  // An MCP server a run is allowed stands beside the file tools it shows, and does not make the run
  // one that only reads.
  test("an MCP server allowed beside the tools shown leaves the run writing", () => {
    expect(dshPermissionMode(runFor({ line: "chat", toolsShown: "Read,Write,Edit,Glob,Grep", toolsAllowed: "Read,Write,Edit,Glob,Grep,mcp__fragments" }))).toBe("workspace-write");
    expect(dshPermissionMode(runFor({ line: "chat", toolsShown: "Read,Write,Edit,Glob,Grep", toolsAllowed: "Read,Glob,mcp__fragments" }))).toBe("read-only");
  });

  test("and a run shown nothing composes no file effects at all, so it names none", async () => {
    expect(dshPermissionMode(runFor())).toBeNull();
    expect((await dsh.command(runFor())).env.DSH_PERMISSION_MODE).toBeUndefined();
  });

  test("the mode travels in the environment as well as on the row, so both of dsh's knobs move with it", async () => {
    for (const run of [
      runFor({ line: "worker", toolsShown: null, toolsAllowed: null }),
      runFor({ toolsShown: "Bash,Read,Write,Edit", toolsAllowed: "Bash,Read,Write,Edit" }),
      runFor({ toolsShown: "Bash,Read,Write,Edit", toolsAllowed: "Read" }),
    ]) {
      const mode = dshPermissionMode(run);

      // The row is what the overlay file says the run may do, and the environment is what dsh
      // reads to set the asking beside it. One value stands in both, so they cannot fall apart.
      expect(rowsById(run).get("sandbox-policy")?.config).toEqual({ mode, workspaceRoot: run.cwd });
      expect((await dsh.command(run)).env.DSH_PERMISSION_MODE).toBe(mode as string);
    }
  });
});

describe("the reply read off a finished run", () => {
  test("is the answer on stdout, and it names no session, no window and no cost", async () => {
    const reply = await dsh.readAnswer({ stdout: "  an answer  \n", stderr: "dsh: reasoning:\nthinking\n", exitCode: 0 });

    expect(reply.text).toBe("an answer");
    // dsh writes a session log of its own and headless takes one task, so there is nothing here
    // for a chat to resume and the reply claims none.
    expect(reply.sessionId).toBeNull();
    expect(reply.windows).toEqual([]);
    // headless prints the answer and nothing beside it, so what the run cost is not in it.
    expect(reply.spend).toEqual([]);
    expect(reply.contextRatio).toBeNull();
  });

  test("a run that ended any other way answers nothing, and says what dsh said about it", async () => {
    const failed = {
      stdout: "",
      stderr: 'dsh: reasoning:\nwhile thinking\ndsh: RATE_LIMIT: 429: {"message":"Provider returned error"}\n',
      exitCode: 1,
    };

    expect((await dsh.readAnswer(failed)).text).toBe("");
    expect(dshFailure(failed)).toBe('dsh: RATE_LIMIT: 429: {"message":"Provider returned error"}');
    // A run that said nothing at all about itself is named by how it ended.
    expect(dshFailure({ stdout: "", stderr: "", exitCode: 137 })).toBe("exit 137");
    // Where the failure was never dsh's to report, the exit code alone names none of what went
    // wrong, so the last word that is not the model thinking aloud is carried with it.
    expect(dshFailure({ stdout: "", stderr: "npm error 404 Not Found - GET https://registry.npmjs.org/@deepseek-ai%2fdsh\n", exitCode: 1 }))
      .toBe("exit 1: npm error 404 Not Found - GET https://registry.npmjs.org/@deepseek-ai%2fdsh");
    // And the reasoning is still not the failure, however much of it came last.
    expect(dshFailure({ stdout: "", stderr: "connect ECONNREFUSED 127.0.0.1:443\ndsh: reasoning ...thinking\n", exitCode: 1 }))
      .toBe("exit 1: connect ECONNREFUSED 127.0.0.1:443");
  });

  // A run that never started is the failure a record is worst at: dsh throws rather than writing
  // its own line, the runtime prints the stack and then its own version, and the last word of
  // that is a version banner. What has to be carried is the sentence dsh put in the error.
  test("a run that could not be composed at all is named by what dsh threw, not by what came last", () => {
    const failed = {
      stdout: "",
      stderr: [
        "npm notice run npx",
        "file:///home/someone/.npm/_npx/x/node_modules/@deepseek-ai/dsh-app-boot/lib/index.js:1511",
        "\t\tthrow new Error(`${binName}: ${stage}: ${detail}${stack}`, { cause });",
        "",
        "Error: dsh: plugin tree failed to load: failed to apply loader entry permission "
          + "(@deepseek-ai/dsh-permission-presets): permission: composed sandbox and approval "
          + "defaults match no preset; configure defaultPreset explicitly",
        "    at new PermissionPresetService (file:///home/someone/.npm/_npx/x/lib/index.js:112:41)",
        "",
        "Node.js v24.19.0",
      ].join("\n"),
      exitCode: 1,
    };

    expect(dshFailure(failed)).toContain("plugin tree failed to load");
    expect(dshFailure(failed)).toContain("configure defaultPreset explicitly");
    expect(dshFailure(failed)).not.toContain("Node.js v24.19.0");
  });

  // However dsh says it, the record gets a line and not a paragraph: a thrown error carries its
  // own stack behind the sentence, and a log line is what this stands in.
  test("and whatever it said is cut to a line", () => {
    const said = `dsh: ${"€".repeat(500)}`;

    expect(dshFailure({ stdout: "", stderr: `${said}\n`, exitCode: 1 }).length).toBe(300);
  });
});

// The name that asks for the live run below, and nothing else turns it on.
const liveRunEnv = "BMO_DSH_LIVE_RUN";

// The whole of it, really run — the one check in this repository that leaves the machine. It is
// skipped unless somebody asked for it by name, because an ordinary `bun test` must not reach the
// network: a suite that does is a suite whose result depends on a gateway being up, on a free pool
// not being rate-limited and on somebody's request budget, and it spends a real request of the
// owner's on every run whoever started it. The key check stands beside the switch rather than in
// place of it — a checkout without a key could not run this at all, and Bun loads this
// repository's .env, so on the machine agency runs on the key is always there and would gate nothing.
//
// A free model is asked for the answer, and the free ones are read off the listing rather than
// written down here: which models cost nothing changes month by month, and several are tried
// because a shared free pool rate-limits whichever one is busy.
describe.skipIf(process.env[liveRunEnv] === undefined || process.env[openRouterKeyEnv] === undefined)("a run that really goes", () => {
  test(
    "answers on the OpenRouter route with the persona it was given",
    async () => {
      const home = join(root, "real");

      mkdirSync(home, { recursive: true });
      writeFileSync(promptPath, "You answer with one Arabic numeral and nothing else.\n");

      const free = (await openRouterModels()).filter((model) => model.free).slice(0, 3);

      expect(free.length).toBeGreaterThan(0);

      const said: string[] = [];

      for (const model of free) {
        const command = await dsh.command(runFor({ model: model.id, prompt: "1 + 1 = ?", cwd: home, env: process.env }));
        const proc = Bun.spawn(command.args, {
          cwd: command.cwd,
          env: command.env,
          stdout: "pipe",
          stderr: "pipe",
        });
        const stdout = await new Response(proc.stdout).text();
        const stderr = await new Response(proc.stderr).text();
        const exitCode = await proc.exited;
        const reply = await dsh.readAnswer({ stdout, stderr, exitCode });

        console.log(`dsh · ${model.id} · exit ${exitCode} · ${reply.text === "" ? dshFailure({ stdout, stderr, exitCode }) : reply.text}`);

        if (reply.text !== "") {
          said.push(reply.text);
          expect(reply.sessionId).toBeNull();

          break;
        }
      }

      expect(said.at(0)).toContain("2");
    },
    600_000,
  );

  // The gate run above is the shape that always worked, and it is not the shape a job takes. A
  // job is shown tools and may do anything to the files around it, and that is the composition
  // dsh refuses when it is handed half of it — so what is driven here is a job's own shape,
  // against a real gateway, because the refusal happens as dsh composes and no check made of
  // agency's own objects can see it.
  test(
    "a job's own shape composes and answers too, and not only a judgement's",
    async () => {
      const home = join(root, "worker");

      mkdirSync(home, { recursive: true });

      const free = (await openRouterModels()).filter((model) => model.free).slice(0, 3);
      let answered: string | null = null;

      for (const model of free) {
        const run = workerRun({ model: model.id, prompt: "What is the word? Reply with the word alone.", cwd: home, env: process.env });
        const command = await dsh.command(run);
        const proc = Bun.spawn(command.args, {
          cwd: command.cwd,
          env: command.env,
          stdout: "pipe",
          stderr: "pipe",
        });
        const stdout = await new Response(proc.stdout).text();
        const stderr = await new Response(proc.stderr).text();
        const exitCode = await proc.exited;
        const reply = await dsh.readAnswer({ stdout, stderr, exitCode }, run);
        const why = reply.text === "" ? dshFailure({ stdout, stderr, exitCode }) : reply.text;

        console.log(`dsh worker · ${model.id} · exit ${exitCode} · ${why}`);

        // A composition dsh will not take is not a busy free pool, so it fails the test where a
        // rate limit only moves it on to the next model.
        expect(why).not.toContain("plugin tree failed to load");

        if (reply.text !== "") {
          answered = reply.text;

          break;
        }
      }

      // The word is only in the rules the line handed over, so an answer naming it is a turn that
      // read them.
      expect(answered).toContain("KUMQUAT");
    },
    600_000,
  );
});
