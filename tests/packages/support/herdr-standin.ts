import { appendFileSync, mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

import { transcriptPath } from "@agency/agent/cli-transcripts";
import { herdrModel } from "@agency/agent/herdr-model";
import { agentPlace, recordCall } from "@agency/agent/agent-place";
import { programLogin } from "@agency/agent/program-login";

import type { ProgramLogin } from "@agency/agent/program-login";
import { processBirth } from "@agency/agent/agent-host";
import { headAgentOf } from "@agency/agent/herdr-run";
import { readProgram, readRunning, recordStarted } from "@agency/agent/resident-pane";

// A herdr of a test's own, answering the calls one turn in a pane is made of.
//
// A run in a pane prints nothing: it is told its work over the socket and read afterwards out of
// the record the CLI writes for itself. So a stand-in for one is not a fake program printing a
// canned stream — it is a server that takes the pane, is told the words, and writes into the record
// at the place the run's own command says the record is. Nothing is spawned; the command it is
// handed is read for its session and its directory, which is what decides where the record goes.
//
// The machine's own herdr is never touched: a test points the socket path at this one, the way the
// launcher is pointed at a directory of its own.

// Which CLI herdr says it recognised in the pane. It is herdr's own word for the program, not the
// name of any agent of agency's.
const detectedCli = "claude";
// The line the CLI writes into a session's record once a turn is over.
const turnEnded = { type: "system", subtype: "turn_duration" };
const interruptedRecord = { type: "user", message: { role: "user", content: [{ type: "text", text: "[Request interrupted by user for tool use]" }] } };

// The session a program's command opens or resumes: Claude Code's named one, or the thread a Codex
// program is resumed on. None where the program opens one of its own choosing.
function sessionIn(args: readonly string[]): string {
  const named = args.indexOf("--session-id");
  const resumed = args.indexOf("--resume");
  const at = named >= 0 ? named : resumed;

  if (at >= 0) return args[at + 1] ?? "";

  return args[1] === "resume" ? (args[2] ?? "") : "";
}

export interface StandInHerdr {
  // The configuration directory to point `XDG_CONFIG_HOME` at; the socket is under it where herdr
  // keeps its own.
  configHome: string;
  socket: string;
  asked: { method: string; params: Record<string, unknown> }[];
  // Every record file this stand-in wrote, for the test to take away again.
  written: string[];
  stop(): void;
}

export function standInHerdr(
  name: string,
  options: {
    // What the run writes to its record when it is told something. Null writes nothing, which is a
    // run that came back to rest with no answer.
    answer: (told: { text: string; sessionId: string; cwd: string }) => unknown[] | null | Promise<unknown[] | null>;
    // What the agent reports once it has been told, look by look; the last repeats. By default it works
    // and then is done, as herdr reports a finished turn. A function is asked with the words told, for a
    // test whose turn goes on working until more words are pushed into it.
    after?: string[] | ((text: string) => string[]);
    // What it reports before it has been told anything. A function is asked on every look, for a
    // test that stages a program whose state changes under it — one wedged until it is reset.
    before?: string | (() => string);
    // What each turn of a resident program adds to the running total its status line hands over.
    cost?: number;
    // Where the program writes what it was told, for a program whose record is not Claude Code's
    // transcript; null writes nothing. The lines a turn ends with, where they are not Claude Code's.
    record?: (session: { id: string; cwd: string }) => string | null;
    ending?: unknown[];
    // Where Claude Code's transcripts go, for a program whose configuration home is not the machine's.
    projects?: string;
    // When words put in are sent: at once, as a program takes a prompt; only once `enter` is pressed,
    // as words pasted into the input box and left there; or never, as a box nothing will send.
    submits?: "at-once" | "on-enter" | "never";
    // How long a resident program takes to come up once its loop has started it. Until then words put
    // into its pane reach no input box and are lost, and herdr goes on reading the pane as the program
    // before it left it; once up, the program draws itself, which runs its status line.
    comesUpMs?: number;
    // What the pane shows when it is read.
    screen?: string;
    // The account windows a resident program's status line hands over after each turn, as the CLI
    // shapes them; none by default.
    limits?: unknown;
    // The login the resident program's status line says it runs under. By default it is the one the
    // program's own environment names, as the real recorder reads it.
    login?: ProgramLogin;
    // The context window a resident program's status line hands over after each turn, as the CLI shapes
    // it; none by default.
    context?: unknown;
    // Whether the turn a prompt opens ends as it is told, its Stop hook run at once; false for a turn
    // that goes on working, which the words pushed into it later end. Every turn ends at once by default.
    stops?: (text: string) => boolean;
    // The error a call is refused with, or null where it is answered; nothing of a refused call reaches
    // the pane, as with herdr's own refusals.
    refuses?: (method: string, params: Record<string, unknown>) => { code: string; message: string } | null;
  },
): StandInHerdr {
  const transcriptOf = (session: { id: string; cwd: string }): string => transcriptPath(session.cwd, session.id, options.projects);
  const recordOf = (session: { id: string; cwd: string }): string | null =>
    options.record !== undefined ? options.record(session) : session.id === "" ? null : transcriptOf(session);
  const configHome = join(tmpdir(), `bmo-herdr-standin-${name}-${process.pid}`);
  const socket = join(configHome, "herdr", "herdr.sock");

  rmSync(configHome, { recursive: true, force: true });
  mkdirSync(dirname(socket), { recursive: true });

  const kept: StandInHerdr = {
    configHome,
    socket,
    asked: [],
    written: [],
    stop() {
      clearInterval(tending);
      for (const drawing of draws) clearTimeout(drawing);
      for (const program of programs.values()) program.kill();
      server.stop(true);
      rmSync(configHome, { recursive: true, force: true });

      for (const file of kept.written) {
        rmSync(file, { force: true });
      }
    },
  };
  // Where things stand — workspaces, tabs, panes and what was reported on them — is herdr's own
  // arrangement, so a run of a standing agent opens beside a head exactly as it would on the machine.
  // The pane that holds the CLI is the one laid out with a session on its command line.
  const model = herdrModel();
  let runPane: string | null = null;
  let session = { id: "", cwd: "" };
  // A standing agent's head, which the machine's loop would turn into the agent's resident program:
  // here the place is watched and the program "started" — a sleeping process standing for it — each
  // time the place names one that is not up, on its restart command once it has been started, so a
  // launcher waiting for its program finds it and its session is where the prompt's record goes.
  // Each head is its pane's, on the data root its own environment names: one agent stands in a
  // workspace of its own on each data root (docs/agent-control.md, 「What goes where」), and each
  // pane holds the program of its own place.
  const heads = new Map<string, { agent: string; root: string | undefined }>();
  const held = new Map<string, string>();
  const sessions = new Map<string, { id: string; cwd: string }>();
  const programs = new Map<string, ReturnType<typeof Bun.spawn>>();
  let spent = 0;
  let residentPlace: string | null = null;
  let login: ProgramLogin | null = null;
  // One status reading as the recorder writes it: the running total, the windows where the turn
  // came with some, and the login of the program that made it.
  const statusOf = (at?: number): void => {
    if (residentPlace === null) return;

    const payload = {
      session_id: session.id,
      cost: { total_cost_usd: spent },
      ...(options.limits === undefined ? {} : { rate_limits: options.limits }),
      ...(options.context === undefined ? {} : { context_window: options.context }),
    };

    recordCall(residentPlace, "status", payload, at, login === null ? {} : { login });
  };
  // When the resident program standing now is up, and the draws still to come.
  let upAt = 0;
  const draws = new Set<ReturnType<typeof setTimeout>>();
  // Words pasted into the input box and not yet sent.
  let boxed: Record<string, unknown> | null = null;
  // Words typed into each pane's input box ahead of a paste, which go out with it as one message.
  const typed = new Map<string, string>();
  const tend = (): void => {
    for (const [pane, head] of heads) {
      if (!model.panes().some((one) => one.pane_id === pane)) {
        heads.delete(pane);
        held.delete(pane);
        continue;
      }

      // Asked where the place is now rather than where it was, as the loop does each time it starts.
      const place = agentPlace(head.agent, head.root);

      const program = readProgram(place);
      const running = readRunning(place);
      const up = programs.get(place);

      if (program !== null) held.set(pane, place);

      if (program === null || (up !== undefined && up.exitCode === null && up.signalCode === null && running?.identity === program.identity)) continue;

      up?.kill();
      const starts = running !== null && running.identity === program.identity ? running.starts : 0;
      const command = starts > 0 ? (program.restart ?? program.command) : program.command;
      const child = Bun.spawn(["sleep", "3600"], { stdout: "ignore", stderr: "ignore" });

      const startedAt = Date.now();

      programs.set(place, child);
      recordStarted(place, { identity: program.identity, pid: child.pid, birth: processBirth(child.pid), loop: { pid: child.pid, birth: processBirth(child.pid) ?? "" }, starts: starts + 1, startedAt });
      session = { id: sessionIn(command.args), cwd: command.cwd };
      runPane = pane;
      residentPlace = place;
      sessions.set(place, session);
      upAt = startedAt + (options.comesUpMs ?? 0);
      login = options.login ?? programLogin(command.env);

      const drawing = setTimeout(() => {
        draws.delete(drawing);
        recordCall(place, "status", { session_id: session.id, cost: { total_cost_usd: spent } }, Date.now(), login === null ? {} : { login });
      }, options.comesUpMs ?? 0);

      draws.add(drawing);
    }
  };
  const tending = setInterval(tend, 20);
  let statuses: string[] = [];
  const statusNow = (): string => {
    if (statuses.length === 0) {
      return typeof options.before === "function" ? options.before() : (options.before ?? "idle");
    }

    return statuses.length > 1 ? (statuses.shift() as string) : (statuses[0] as string);
  };
  const open = (): boolean => runPane !== null && model.panes().some((pane) => pane.pane_id === runPane);
  const answer = async (method: string, given: Record<string, unknown>): Promise<unknown> => {
    let params = given;

    if ((method === "pane.send_text" || method === "agent.prompt") && residentPlace !== null && Date.now() < upAt && (params.pane_id ?? params.target) === runPane) {
      // Nothing is there yet to take the words.
      return method === "agent.prompt" ? { type: "agent_prompted" } : { type: "ok" };
    }

    if (method === "pane.send_text") {
      typed.set(String(params.pane_id), `${typed.get(String(params.pane_id)) ?? ""}${String(params.text ?? "")}`);
    } else if (method === "pane.send_keys" && Array.isArray(params.keys) && params.keys.includes("enter") && boxed === null) {
      typed.delete(String(params.pane_id));
    } else if (method === "agent.prompt" && params.submits !== true && typed.has(String(params.target))) {
      params = { ...params, text: `${typed.get(String(params.target))}${String(params.text ?? "")}` };
      typed.delete(String(params.target));
    }

    if (method === "agent.prompt" && held.has(String(params.target))) {
      // Words go to the program the pane they are put into holds.
      const place = held.get(String(params.target))!;

      session = sessions.get(place) ?? session;
      residentPlace = place;
      runPane = String(params.target);
    }

    if (method === "layout.apply") {
      const root = (params.root ?? {}) as { command?: string[]; cwd?: string; env?: Record<string, string> };
      const command = root.command ?? [];
      const named = command.indexOf("--session-id");
      const resumed = command.indexOf("--resume");
      const at = named >= 0 ? named : resumed;
      const laid = model.answer(method, params) as { layout?: { root?: { pane_id?: string } } } | null;

      if (at >= 0) {
        session = { id: command[at + 1] ?? "", cwd: root.cwd ?? "" };
        runPane = laid?.layout?.root?.pane_id ?? null;
      }

      const agent = headAgentOf(command);

      if (agent !== null && laid?.layout?.root?.pane_id !== undefined) {
        heads.set(laid.layout.root.pane_id, { agent, root: root.env?.BMO_DATA_DIR });
        tend();
      }

      return laid;
    }

    if (method === "agent.prompt" && (options.submits ?? "at-once") !== "at-once" && params.submits !== true) {
      // Pasted, and left standing in the input box: the pane does not change until it is sent.
      boxed = params;

      return { type: "agent_prompted" };
    }

    if (method === "pane.send_keys" && Array.isArray(params.keys) && params.keys.includes("enter") && boxed !== null && options.submits === "on-enter") {
      const sent = boxed;

      boxed = null;
      await answer("agent.prompt", { ...sent, submits: true });

      return { type: "ok" };
    }

    if (method === "agent.prompt") {
      const records = await options.answer({ text: String(params.text ?? ""), sessionId: session.id, cwd: session.cwd });

      const file = records === null ? null : recordOf(session);

      if (records !== null && file !== null) {
        mkdirSync(dirname(file), { recursive: true });
        // A turn's lines end the way the program ends one, with the line that says the turn is over.
        // The prompt it was given opens the turn, as the CLI writes it.
        appendFileSync(file, `${[{ type: "user", message: { role: "user", content: String(params.text ?? "") } }, ...records, ...(options.ending ?? [turnEnded])].map((one) => JSON.stringify(one)).join("\n")}\n`);

        if (!kept.written.includes(file)) {
          kept.written.push(file);
        }
      }

      // A program that has finished a turn reads as done, not idle, which is how herdr reports one
      // between its turns (measured).
      const after = typeof options.after === "function" ? options.after(String(params.text ?? "")) : options.after;

      statuses = [...(after ?? ["working", "done"])];

      // A resident program hands its status line a running total after each turn, which is where a
      // turn's money is read from.
      if (residentPlace !== null && runPane === params.target && (options.stops?.(String(params.text ?? "")) ?? true)) {
        const at = Date.now();

        spent += options.cost ?? 0;
        recordCall(residentPlace, "stop", { session_id: session.id }, at);
        statusOf(at + 1);
      }

      return { type: "agent_prompted" };
    }

    // Escape into a working program stops its turn: it writes the interrupted marker and comes back
    // to rest, with no end line and no Stop hook (measured, 「What a resident turn reads」).
    if (method === "pane.send_keys" && params.pane_id === runPane && Array.isArray(params.keys) && params.keys.includes("escape")) {
      if (session.id !== "") {
        const file = transcriptOf(session);

        mkdirSync(dirname(file), { recursive: true });
        appendFileSync(file, `${JSON.stringify(interruptedRecord)}\n`);

        if (!kept.written.includes(file)) {
          kept.written.push(file);
        }
      }

      statuses = ["idle"];

      return { type: "ok" };
    }

    if (method === "agent.list") {
      return { agents: open() ? [{ pane_id: runPane, agent: detectedCli, agent_status: statusNow() }] : [] };
    }

    if (method === "pane.list") {
      return { panes: model.panes() };
    }

    if (method === "pane.read") {
      return { read: { text: options.screen ?? "a screen" } };
    }

    if ((method === "pane.close" && params.pane_id === runPane) || method === "workspace.close") {
      // The CLI writes its account as its process ends. This one names no model, so what a turn
      // spent is read from its own lines, which is what a test here writes.
      if (open() && session.id !== "") {
        const file = transcriptOf(session);

        mkdirSync(dirname(file), { recursive: true });
        appendFileSync(file, `${JSON.stringify({ type: "cost-state", modelUsage: {} })}\n`);

        if (!kept.written.includes(file)) {
          kept.written.push(file);
        }
      }
    }

    return model.answer(method, params) ?? { type: "ok" };
  };
  // A request is read to the end of its line, which may arrive over several reads, as herdr reads one.
  const server = Bun.listen<{ held: string; reading: TextDecoder }>({
    unix: socket,
    socket: {
      open(connection) {
        connection.data = { held: "", reading: new TextDecoder() };
      },
      async data(connection, chunk) {
        connection.data.held += connection.data.reading.decode(chunk, { stream: true });

        const end = connection.data.held.indexOf("\n");

        if (end < 0) {
          return;
        }

        const request = JSON.parse(connection.data.held.slice(0, end)) as { id: string; method: string; params?: Record<string, unknown> };
        const params = request.params ?? {};
        const refused = options.refuses?.(request.method, params) ?? null;

        kept.asked.push({ method: request.method, params });
        connection.write(`${JSON.stringify(refused === null ? { id: request.id, result: await answer(request.method, params) } : { id: request.id, error: refused })}\n`);
        connection.end();
      },
    },
  });

  return kept;
}

// One line of a run's record, in the shape the CLI writes it: an answer, what it cost and on which
// model. A test composes a turn out of these.
export function answerRecord(options: {
  text?: string;
  model?: string;
  read?: number;
  write?: number;
  input?: number;
  output?: number;
  sidechain?: boolean;
}): unknown {
  return {
    type: "assistant",
    isSidechain: options.sidechain ?? false,
    effort: "high",
    message: {
      role: "assistant",
      model: options.model ?? "claude-opus-5",
      content: options.text === undefined ? [] : [{ type: "text", text: options.text }],
      usage: {
        input_tokens: options.input ?? 0,
        output_tokens: options.output ?? 0,
        cache_read_input_tokens: options.read ?? 0,
        cache_creation_input_tokens: options.write ?? 0,
      },
    },
  };
}
