import "@agency/test-support/setup";

import { afterEach, describe, expect, test } from "bun:test";
import { rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  closeLeftovers,
  closeRun,
  DEV_VERB,
  DEV_TAB_LABEL,
  openPane,
  shortTitle,
  STANDING_MARK,
  standingCommand,
  standingDevPane,
  standingTabs,
  standingWorkspace,
  stopInstruction,
  stopRun,
  waitForRun,
  type AgentHome,
  type StandingTab,
} from "@agency/agent/herdr-run";
import { herdrModel, type HerdrModel } from "@agency/agent/herdr-model";
import { UNIT } from "@agency/test-support/ids";

// What is held here is where a run's pane ends up and what it is found by, because each has a way
// of going wrong that answers ok and leaves the wrong thing standing: a run with no name on its pane
// is a run nothing can stop, a run laid into a standing agent's tab kills whatever already ran
// there, and a standing workspace found twice leaves a second one in the sidebar for ever.

interface Asked {
  method: string;
  params: Record<string, unknown>;
}

interface HerdrLike {
  path: string;
  asked: Asked[];
  model: HerdrModel;
  // A pane whose program ends right after the next picture is read, its pane going with it.
  endsAfterNextSnapshot(paneId: string): void;
  stop(): void;
}

const servers: HerdrLike[] = [];

afterEach(() => {
  for (const server of servers.splice(0)) {
    server.stop();
  }
});

// A stand-in herdr: the arrangement answered from the model, and the few ways a call can be refused
// or a run can answer that a test wants to choose.
function herdrLike(
  name: string,
  options: { refuse?: string[]; promptBlocked?: boolean } = {},
): HerdrLike {
  const path = join(tmpdir(), `bmo-herdr-run-${name}-${process.pid}.sock`);
  const model = herdrModel();

  rmSync(path, { force: true });

  let ending: string | null = null;
  const kept: HerdrLike = {
    path,
    asked: [],
    model,
    endsAfterNextSnapshot(paneId) {
      ending = paneId;
    },
    stop() {
      server.stop(true);
      rmSync(path, { force: true });
    },
  };
  const answer = (method: string, params: Record<string, unknown>): unknown => {
    if (options.refuse?.includes(method) === true) {
      return null;
    }

    if (method === "agent.prompt") {
      return options.promptBlocked === true ? "blocked" : { type: "agent_prompted" };
    }

    const answered = model.answer(method, params) ?? null;

    if (method === "session.snapshot" && ending !== null) {
      model.answer("pane.close", { pane_id: ending });
      ending = null;
    }

    return answered;
  };
  const server = Bun.listen<undefined>({
    unix: path,
    socket: {
      data(connection, chunk) {
        for (const line of chunk.toString().split("\n")) {
          if (line.trim() === "") {
            continue;
          }

          const request = JSON.parse(line) as { id: string; method: string; params?: Record<string, unknown> };

          kept.asked.push({ method: request.method, params: request.params ?? {} });

          const result = answer(request.method, request.params ?? {});

          connection.write(
            `${JSON.stringify(
              result === null
                ? { id: request.id, error: { code: "refused", message: `no ${request.method} here` } }
                : result === "blocked"
                  ? { id: request.id, error: { code: "agent_blocked", message: "agent is blocked and requires interactive input" } }
                  : { id: request.id, result },
            )}\n`,
          );
          connection.end();
        }
      },
    },
  });

  servers.push(kept);

  return kept;
}

const absent = (name: string): string => join(tmpdir(), `bmo-herdr-run-absent-${name}-${process.pid}.sock`);

const run = (name: string, extra: { title?: string; home?: AgentHome | null } = {}) => ({
  name,
  title: extra.title ?? `title of ${name}`,
  home: extra.home ?? null,
  cwd: "/repo",
  command: ["bun", "x", name],
  env: { PATH: "/bin" },
});

// The head program every head pane runs, at the tree a home names.
const paneHead = "/code/bin/pane-head";

const home = (label = "mother worker"): AgentHome => ({
  agent: "worker",
  label,
  // Laid as every head is, under its loop's mark, which is what tells a head pane from the rest.
  head: { command: ["/bin/sh", "-c", 'exec "$@"', "bmo-head-worker", paneHead, "worker"], cwd: "/code", env: { PATH: "/bin" } },
});

const runOf = (herdr: HerdrLike, name: string) => herdr.model.panes().find((pane) => pane.tokens?.run === name);

describe("opening a pane for a run of its own", () => {
  test("the workspace is labelled with what the run is, and the run's name goes on the pane", async () => {
    const herdr = herdrLike("own");
    const opened = await openPane(run("bmo-search-9", { title: "the weather today" }), { socket: herdr.path });

    expect(opened.ok).toBe(true);
    expect(herdr.asked[0]).toEqual({ method: "workspace.create", params: { label: "the weather today", cwd: "/repo", env: { PATH: "/bin" } } });
    expect(herdr.model.workspaces().map((one) => one.label)).toEqual(["the weather today"]);
    expect(runOf(herdr, "bmo-search-9")?.pane_id).toBe(opened.ok ? opened.result.paneId : "");
  });

  // Naming both is refused outright, and naming the workspace alone puts the command in a second
  // tab beside the shell — which leaves the shell holding the workspace open after the run ends.
  test("the command goes into the tab that already exists, named by the tab alone", async () => {
    const herdr = herdrLike("tab");

    await openPane(run("bmo-voice-1"), { socket: herdr.path });

    const applied = herdr.asked.find((one) => one.method === "layout.apply");

    expect(applied?.params.tab_id).toBe("w1:t1");
    expect(applied?.params).not.toHaveProperty("workspace_id");
    expect(applied?.params.root).toEqual({ type: "pane", command: ["bun", "x", "bmo-voice-1"], cwd: "/repo", env: { PATH: "/bin" } });
    expect(herdr.model.panes()).toHaveLength(1);
  });

  test("a workspace whose command could not be put in it is closed rather than left standing", async () => {
    const herdr = herdrLike("half", { refuse: ["layout.apply"] });
    const opened = await openPane(run("bmo-browse-3"), { socket: herdr.path });

    expect(opened.ok).toBe(false);
    expect(herdr.asked.map((one) => one.method)).toEqual(["workspace.create", "layout.apply", "workspace.close"]);
    expect(herdr.model.workspaces()).toHaveLength(0);
  });

  // A run with no name on its pane is one no liveness question and no stop can find.
  test("a pane that could not be given the run's name is closed and the launch fails", async () => {
    const herdr = herdrLike("unnamed", { refuse: ["pane.report_metadata"] });
    const opened = await openPane(run("bmo-browse-3"), { socket: herdr.path });

    expect(opened.ok).toBe(false);
    expect(herdr.model.panes()).toHaveLength(0);
  });

  test("a herdr that is not there is a carried failure, which is what the fallback reads", async () => {
    const opened = await openPane(run("bmo-search-9"), { socket: absent("open") });

    expect(opened.ok).toBe(false);
    expect(opened.ok ? "" : opened.error.code).not.toBe("");
  });

  test("a title is one line and short enough for a sidebar", () => {
    expect(shortTitle("  first line\nsecond line")).toBe("first line");
    expect(shortTitle("€".repeat(80))).toHaveLength(48);
    expect(shortTitle("€".repeat(80)).endsWith("…")).toBe(true);
  });
});

describe("a run of a standing agent", () => {
  test("opens in a tab of its own in the agent's workspace, which is labelled with the agent's name", async () => {
    const herdr = herdrLike("home");
    const opened = await openPane(run(UNIT, { title: "#322 herdr screen", home: home() }), { socket: herdr.path });

    expect(opened.ok).toBe(true);
    expect(herdr.model.workspaces().map((one) => one.label)).toEqual(["mother worker"]);

    const panes = herdr.model.panes();
    const head = panes.find((pane) => pane.command?.includes(paneHead));
    const task = runOf(herdr, UNIT);

    expect(head?.label).toBe("mother worker");
    expect(task?.label).toBe("#322 herdr screen");
    // Never split into the head's tab: the head is the agent's one pane, and nothing is moved into it.
    expect(task?.tab_id).not.toBe(head?.tab_id ?? "");
    expect(herdr.asked.some((one) => one.method === "pane.move")).toBe(false);
    // Never a layout laid over the head's tab, which would start every pane in it afresh.
    expect(herdr.asked.filter((one) => one.method === "layout.apply" && one.params.tab_id === head?.tab_id)).toHaveLength(0);
  });

  test("a second run stands in a tab of its own rather than replacing the first, and the head outlives both", async () => {
    const herdr = herdrLike("two");

    await openPane(run("bmo-search-1", { home: home("voyager") }), { socket: herdr.path });
    await openPane(run("bmo-search-2", { home: home("voyager") }), { socket: herdr.path });

    const first = runOf(herdr, "bmo-search-1");
    const second = runOf(herdr, "bmo-search-2");

    expect(first).toBeDefined();
    expect(second?.tab_id).not.toBe(first?.tab_id ?? "");
    expect(herdr.asked.some((one) => one.method === "pane.move")).toBe(false);
    expect(herdr.model.workspaces()).toHaveLength(1);
    // The head is found where it stands beside a run — herdr describes a pane by its whole tab — and
    // is never started a second time.
    expect(herdr.asked.filter((one) => ((one.params.root ?? {}) as { command?: string[] }).command?.includes(paneHead) === true)).toHaveLength(1);

    herdr.model.exit(first?.pane_id ?? "");
    await closeRun("bmo-search-2", { socket: herdr.path });

    expect(herdr.model.workspaces().map((one) => one.label)).toEqual(["voyager"]);
    expect(herdr.model.panes().map((pane) => pane.command?.includes(paneHead))).toEqual([true]);
  });

  test("the next run finds the same workspace, and a name the owner changed is given to it", async () => {
    const herdr = herdrLike("rename");

    await openPane(run(UNIT, { home: home("mother worker") }), { socket: herdr.path });
    await closeRun(UNIT, { socket: herdr.path });
    await openPane(run("bmo-task-2", { home: home("chat worker") }), { socket: herdr.path });

    expect(herdr.model.workspaces().map((one) => one.label)).toEqual(["chat worker"]);
    expect(herdr.asked.filter((one) => one.method === "workspace.create")).toHaveLength(1);
  });

  // What a herdr that came back hands over: the workspace and its label, a fresh shell where the head
  // was, and no tokens at all.
  test("a workspace herdr rebuilt gets its head started again and its dead shells closed", async () => {
    const herdr = herdrLike("rebuilt");

    herdr.model.answer("workspace.create", { label: "mother worker" });

    const opened = await openPane(run("bmo-task-3", { home: home() }), { socket: herdr.path });

    expect(opened.ok).toBe(true);
    expect(herdr.model.workspaces()).toHaveLength(1);
    expect(herdr.model.panes().filter((pane) => pane.command === null)).toHaveLength(0);
    expect(herdr.model.panes().filter((pane) => pane.command?.includes(paneHead))).toHaveLength(1);
    expect(runOf(herdr, "bmo-task-3")).toBeDefined();
  });

  // A place whose label follows the run in it is revived under whichever label it was left on, which
  // is the resting one once the turn that renamed it has ended.
  test("a rebuilt workspace left on the resting label is the agent's, and is given the label it now stands under", async () => {
    const herdr = herdrLike("resting");

    herdr.model.answer("workspace.create", { label: "co-worker bmo 01" });

    const opened = await openPane(run("bmo-task-4", { home: { ...home("#721 scout 01"), resting: "co-worker bmo 01" } }), { socket: herdr.path });

    expect(opened.ok).toBe(true);
    expect(herdr.model.workspaces().map((one) => one.label)).toEqual(["#721 scout 01"]);
  });

  // A head that let go of its pane leaves a shell standing in it, and the head that came up next
  // stands in a tab of its own. Beside a live head that shell is a tab of nothing, and nothing would
  // ever look at it again if only the making of a head swept it.
  test("a shell left beside a live head is closed, though no head had to be made", async () => {
    const herdr = herdrLike("leftover");
    const first = await standingWorkspace(home(), { socket: herdr.path });

    expect(first.ok).toBe(true);

    const workspaceId = first.ok ? first.result.workspaceId : "";

    herdr.model.answer("layout.apply", { workspace_id: workspaceId, tab_label: "mother worker", root: { type: "pane", label: "mother worker" } });

    expect(herdr.model.panes().filter((pane) => pane.command === null)).toHaveLength(1);

    const again = await standingWorkspace(home(), { socket: herdr.path });

    expect(again.ok ? again.result.workspaceId : "").toBe(workspaceId);
    expect(herdr.model.panes().filter((pane) => pane.command === null)).toHaveLength(0);
    expect(herdr.model.panes().filter((pane) => pane.command?.includes(paneHead))).toHaveLength(1);
    expect(herdr.asked.filter((one) => one.method === "workspace.create")).toHaveLength(1);
  });

  // A head pane's loop that ends between the two readings of a stand takes the workspace whose one pane it
  // is with it; the stand looks again and makes the workspace anew rather than failing the turn.
  test("a workspace gone between its finding and its head's is made again", async () => {
    const herdr = herdrLike("gone");
    const first = await standingWorkspace(home(), { socket: herdr.path });

    expect(first.ok).toBe(true);

    const before = first.ok ? first.result : null;

    herdr.endsAfterNextSnapshot(before?.headPaneId ?? "");

    const again = await standingWorkspace(home(), { socket: herdr.path });

    expect(again.ok).toBe(true);
    expect(again.ok ? again.result.workspaceId : before?.workspaceId).not.toBe(before?.workspaceId);
    expect(herdr.model.workspaces()).toHaveLength(1);
    expect(herdr.model.panes().filter((pane) => pane.command?.includes(paneHead))).toHaveLength(1);
  });

  test("a leftover is swept on its own, with nothing made where the agent has no workspace", async () => {
    const herdr = herdrLike("sweep");

    expect(await closeLeftovers(home(), { socket: herdr.path })).toEqual({ ok: true, result: 0 });
    expect(herdr.model.workspaces()).toHaveLength(0);

    herdr.model.answer("workspace.create", { label: "mother worker" });

    const swept = await closeLeftovers(home(), { socket: herdr.path });

    expect(swept).toEqual({ ok: true, result: 1 });
    expect(herdr.model.panes()).toHaveLength(0);
  });

  test("two workspaces for one agent settle on the lower-numbered, which both launches agree on", async () => {
    const herdr = herdrLike("race");

    herdr.model.answer("workspace.create", { label: "voyager" });
    herdr.model.answer("workspace.create", { label: "voyager" });

    const settled = await standingWorkspace(home("voyager"), { socket: herdr.path });

    expect(settled.ok ? settled.result.workspaceId : "").toBe("w1");
  });

  test("an agent whose workspace cannot be had still gets its run, in a workspace of its own", async () => {
    const herdr = herdrLike("no-home", { refuse: ["session.snapshot"] });
    const opened = await openPane(run("bmo-task-4", { title: "keepalive", home: home() }), { socket: herdr.path });

    expect(opened.ok).toBe(true);
    expect(herdr.model.workspaces().map((one) => one.label)).toEqual(["keepalive"]);
  });
});

// A worker's dev server stands in its workspace beside the head and is no run: nothing names it, so
// what is held here is that it is found again by what it runs, that it is replaced when that
// changes, and that neither a head redrawn nor a run beside it takes it away.
describe("a worker's dev-server pane", () => {
  const dev = (version: string | null = "v1") => ({
    command: version === null ? null : ["bun", DEV_VERB, "worker", `--version=${version}`],
    cwd: "/code",
    env: { PATH: "/bin" },
  });
  const devPanes = (herdr: HerdrLike) => herdr.model.panes().filter((pane) => pane.command?.includes(DEV_VERB) === true);

  test("stands in a tab of its own in the worker's workspace, with no run on it and nothing focused", async () => {
    const herdr = herdrLike("dev-place");
    const placed = await standingDevPane(home(), dev(), { socket: herdr.path });

    expect(placed.ok && placed.result.started).toBe(true);
    expect(herdr.model.workspaces().map((one) => one.label)).toEqual(["mother worker"]);

    const head = herdr.model.panes().find((pane) => pane.command?.includes(paneHead));
    const [server] = devPanes(herdr);

    expect(server?.workspace_id).toBe(head?.workspace_id ?? "");
    expect(server?.tab_id).not.toBe(head?.tab_id);
    expect(server?.tokens).toBeNull();
    expect(herdr.asked.find((one) => ((one.params.root ?? {}) as { command?: string[] }).command?.includes(DEV_VERB) === true)?.params).toMatchObject({ workspace_id: "w1", tab_label: DEV_TAB_LABEL });
    expect(herdr.asked.some((one) => one.method === "pane.move" || one.method === "tab.focus")).toBe(false);
  });

  test("is found again rather than started twice, and replaced once its command line differs", async () => {
    const herdr = herdrLike("dev-again");

    await standingDevPane(home(), dev("v1"), { socket: herdr.path });

    const again = await standingDevPane(home(), dev("v1"), { socket: herdr.path });

    expect(again.ok && again.result.started).toBe(false);
    expect(devPanes(herdr)).toHaveLength(1);

    const first = devPanes(herdr)[0]?.pane_id;
    const newer = await standingDevPane(home(), dev("v2"), { socket: herdr.path });

    expect(newer.ok && newer.result.started).toBe(true);
    expect(devPanes(herdr).map((pane) => pane.command?.at(-1))).toEqual(["--version=v2"]);
    expect(devPanes(herdr)[0]?.pane_id).not.toBe(first);
  });

  test("of two standing for one worker, one is kept", async () => {
    const herdr = herdrLike("dev-two");

    await standingDevPane(home(), dev(), { socket: herdr.path });
    herdr.model.answer("layout.apply", { workspace_id: "w1", tab_label: DEV_TAB_LABEL, root: { type: "pane", command: dev().command } });

    expect(devPanes(herdr)).toHaveLength(2);

    await standingDevPane(home(), dev(), { socket: herdr.path });

    expect(devPanes(herdr)).toHaveLength(1);
  });

  test("goes when the command is taken away, and the head and its runs stay", async () => {
    const herdr = herdrLike("dev-gone");

    await standingDevPane(home(), dev(), { socket: herdr.path });
    await openPane(run("bmo-task-9", { home: home() }), { socket: herdr.path });

    const gone = await standingDevPane(home(), dev(null), { socket: herdr.path });

    expect(gone.ok).toBe(true);
    expect(devPanes(herdr)).toHaveLength(0);
    expect(runOf(herdr, "bmo-task-9")).toBeDefined();
    expect(herdr.model.panes().filter((pane) => pane.command?.includes(paneHead))).toHaveLength(1);
  });

  test("taking away a command no pane runs makes no workspace", async () => {
    const herdr = herdrLike("dev-nothing");

    await standingDevPane(home(), dev(null), { socket: herdr.path });

    expect(herdr.model.workspaces()).toHaveLength(0);
  });

  test("outlives a head that had to be started again", async () => {
    const herdr = herdrLike("dev-head");

    await standingDevPane(home(), dev(), { socket: herdr.path });

    const head = herdr.model.panes().find((pane) => pane.command?.includes(paneHead));

    herdr.model.exit(head?.pane_id ?? "");
    await openPane(run("bmo-task-10", { home: home() }), { socket: herdr.path });

    expect(herdr.model.panes().filter((pane) => pane.command?.includes(paneHead))).toHaveLength(1);
    expect(devPanes(herdr)).toHaveLength(1);
    expect(runOf(herdr, "bmo-task-10")).toBeDefined();
  });

  // What a herdr that came back hands over: the workspace, a fresh shell where the head was and
  // another where the dev server was, no commands and no tokens. The shells go and the server is
  // started again.
  test("is started again in a workspace herdr rebuilt, and the rebuilt shells are closed", async () => {
    const herdr = herdrLike("dev-rebuilt");

    herdr.model.answer("workspace.create", { label: "mother worker" });
    herdr.model.answer("layout.apply", { workspace_id: "w1", tab_label: DEV_TAB_LABEL, root: { type: "pane" } });

    const placed = await standingDevPane(home(), dev(), { socket: herdr.path });

    expect(placed.ok && placed.result.started).toBe(true);
    expect(herdr.model.panes().filter((pane) => pane.command === null)).toHaveLength(0);
    expect(herdr.model.panes().filter((pane) => pane.command?.includes(paneHead))).toHaveLength(1);
    expect(devPanes(herdr)).toHaveLength(1);
  });

  test("a dev pane of another worker is none of this worker's business", async () => {
    const herdr = herdrLike("dev-other");

    await standingDevPane(home(), dev(), { socket: herdr.path });
    herdr.model.answer("layout.apply", { workspace_id: "w1", tab_label: DEV_TAB_LABEL, root: { type: "pane", command: ["bun", DEV_VERB, "trainer", "--version=v1"] } });
    await standingDevPane(home(), dev(null), { socket: herdr.path });

    expect(devPanes(herdr).map((pane) => pane.command?.[2])).toEqual(["trainer"]);
  });
});

describe("ending a run in a pane", () => {
  test("the run is found by its name and only its pane is closed", async () => {
    const herdr = herdrLike("stop");

    await openPane(run("bmo-search-9"), { socket: herdr.path });

    expect(await closeRun("bmo-search-9", { socket: herdr.path })).toBe(true);
    expect(herdr.asked.at(-1)?.method).toBe("pane.close");
    expect(herdr.model.panes()).toHaveLength(0);
  });

  test("a name herdr is not holding is not something it can close", async () => {
    const herdr = herdrLike("absent-name");

    expect(await closeRun("bmo-search-9", { socket: herdr.path })).toBe(false);
    expect(herdr.asked.map((one) => one.method)).toEqual(["session.snapshot"]);
  });

  test("a close herdr refuses is reported rather than called done", async () => {
    const herdr = herdrLike("refused", { refuse: ["pane.close"] });

    await openPane(run("bmo-search-9"), { socket: herdr.path });

    expect(await closeRun("bmo-search-9", { socket: herdr.path })).toBe(false);
  });
});

describe("waiting for a run in a pane", () => {
  test("it falls the moment no pane carries the name", async () => {
    const herdr = herdrLike("wait");
    const opened = await openPane(run("bmo-search-7"), { socket: herdr.path });

    setTimeout(() => herdr.model.exit(opened.ok ? opened.result.paneId : ""), 150);

    await waitForRun("bmo-search-7", { everyMs: 50, socket: herdr.path });

    expect(herdr.model.panes().some((pane) => pane.pane_id === (opened.ok ? opened.result.paneId : ""))).toBe(false);
  });

  // No clock ends the wait: a run that takes its time over its ending is waited on until it has ended.
  test("and it waits for a run that takes its time, however long", async () => {
    const herdr = herdrLike("long");
    const opened = await openPane(run("bmo-search-7"), { socket: herdr.path });
    let ended = false;

    setTimeout(() => { ended = true; herdr.model.exit(opened.ok ? opened.result.paneId : ""); }, 600);
    await waitForRun("bmo-search-7", { everyMs: 50, socket: herdr.path });

    expect(ended).toBe(true);
  });

  test("a herdr that cannot be asked is not holding anything, so the wait is over", async () => {
    await waitForRun("bmo-search-7", { socket: absent("wait") });
  });
});

// A stop that asks before it takes the pane away. The difference between this and a kill is not
// the socket — the same call carries both — it is that the run is given the chance to end itself.
describe("stopping a run in a pane", () => {
  test("the run is asked, and a run that ends itself is not closed", async () => {
    const herdr = herdrLike("asked");
    const opened = await openPane(run("bmo-run-9-build-0-1"), { socket: herdr.path });

    // It goes as soon as it is asked, which is what a run that commits and reports looks like.
    setTimeout(() => herdr.model.exit(opened.ok ? opened.result.paneId : ""), 100);

    expect(await stopRun("bmo-run-9-build-0-1", { everyMs: 50, socket: herdr.path })).toBe("ended");

    const asked = herdr.asked.filter((one) => one.method === "agent.prompt");

    expect(asked).toHaveLength(1);
    expect(asked[0]?.params.target).toBe(opened.ok ? opened.result.paneId : "");
    expect(String(asked[0]?.params.text)).toContain("commit and push what you have");
    expect(herdr.asked.some((one) => one.method === "pane.close")).toBe(false);
  });

  test("a run that takes its time over its ending is waited on, and its pane is never taken away", async () => {
    const herdr = herdrLike("slow");
    const opened = await openPane(run("bmo-run-9-build-0-1", { home: home() }), { socket: herdr.path });

    setTimeout(() => herdr.model.exit(opened.ok ? opened.result.paneId : ""), 600);

    expect(await stopRun("bmo-run-9-build-0-1", { everyMs: 50, socket: herdr.path })).toBe("ended");
    expect(herdr.asked.filter((one) => one.method === "agent.prompt")).toHaveLength(1);
    expect(herdr.asked.some((one) => one.method === "pane.close" || one.method === "workspace.close")).toBe(false);
  });

  test("a name herdr is not holding is nothing to stop", async () => {
    const herdr = herdrLike("nothing");

    expect(await stopRun("bmo-run-9-build-0-1", { socket: herdr.path })).toBe("absent");
    expect(herdr.asked.map((one) => one.method)).toEqual(["session.snapshot"]);
  });

  // The words are the whole of the difference, so they say what to do rather than only to stop.
  test("what it says names the ending it wants, not just the stop", () => {
    expect(stopInstruction).toContain("Stop here");
    expect(stopInstruction).toContain("answer with your handoff");
    expect(stopInstruction).toContain("exit normally");
  });
});

// A run that cannot be asked. The refusal means nothing was delivered — the pane is left
// byte-identical — so there is nothing to wait for and the pane goes at once.
describe("stopping a run that is waiting on a person", () => {
  test("it is not waited for, because it was never told anything", async () => {
    const herdr = herdrLike("blocked-stop", { promptBlocked: true });

    await openPane(run("bmo-run-9-build-0-1"), { socket: herdr.path });

    const began = Bun.nanoseconds();
    const how = await stopRun("bmo-run-9-build-0-1", { everyMs: 50, socket: herdr.path });
    const held = (Bun.nanoseconds() - began) / 1e6;

    expect(how).toBe("closed");
    // The refusal is the answer, so there is nothing to wait out.
    expect(held).toBeLessThan(2_000);
    expect(herdr.asked.some((one) => one.method === "pane.close")).toBe(true);
  });
});

// An owner's standing tabs stand beside the head and carry no run: what is held here is that a
// wanted line is opened once and found again, that a line no longer wanted or an older one goes, that
// another owner's panes are left alone, and that no pass over leftovers takes them away.
describe("an owner's standing tabs", () => {
  const tab = (label: string, word: string): StandingTab => ({ label, command: standingCommand("owned", ["bun", "x.ts", word]), cwd: "/code", env: { PATH: "/bin" } });
  const owned = (command: readonly string[]) => command.includes(`${STANDING_MARK}owned`);
  const ownedPanes = (herdr: HerdrLike) => herdr.model.panes().filter((pane) => pane.command?.includes(`${STANDING_MARK}owned`) === true);

  test("are opened one tab each in the agent's workspace, nothing focused, and found again rather than opened twice", async () => {
    const herdr = herdrLike("tabs-place");
    const first = await standingTabs(home(), [tab("lounge", "a"), tab("small room", "b")], owned, { socket: herdr.path });

    expect(first).toEqual({ ok: true, result: { opened: 2, closed: 0 } });
    expect(herdr.model.workspaces().map((one) => one.label)).toEqual(["mother worker"]);
    expect(herdr.asked.filter((one) => one.method === "layout.apply" && owned(((one.params.root ?? {}) as { command?: string[] }).command ?? [])).map((one) => one.params.tab_label)).toEqual(["lounge", "small room"]);
    expect(herdr.asked.some((one) => one.method === "pane.move" || one.method === "tab.focus")).toBe(false);

    expect(await standingTabs(home(), [tab("lounge", "a"), tab("small room", "b")], owned, { socket: herdr.path })).toEqual({ ok: true, result: { opened: 0, closed: 0 } });
    expect(ownedPanes(herdr)).toHaveLength(2);
  });

  test("close a line no longer wanted and replace an older one, leaving another owner's panes", async () => {
    const herdr = herdrLike("tabs-change");

    await standingTabs(home(), [tab("lounge", "a"), tab("small room", "b")], owned, { socket: herdr.path });
    await standingDevPane(home(), { command: ["bun", DEV_VERB, "worker", "--version=v1"], cwd: "/code", env: {} }, { socket: herdr.path });

    const changed = await standingTabs(home(), [tab("lounge", "a2")], owned, { socket: herdr.path });

    expect(changed).toEqual({ ok: true, result: { opened: 1, closed: 2 } });
    expect(ownedPanes(herdr).map((pane) => pane.command?.at(-1))).toEqual(["a2"]);
    expect(herdr.model.panes().filter((pane) => pane.command?.includes(DEV_VERB) === true)).toHaveLength(1);
  });

  test("are no leftovers to the pass that sweeps the workspace or the one that heads it", async () => {
    const herdr = herdrLike("tabs-kept");

    await standingTabs(home(), [tab("lounge", "a")], owned, { socket: herdr.path });

    expect(await closeLeftovers(home(), { socket: herdr.path })).toEqual({ ok: true, result: 0 });
    await standingWorkspace(home(), { socket: herdr.path });
    expect(ownedPanes(herdr)).toHaveLength(1);
  });
});
