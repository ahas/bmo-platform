import "@agency/test-support/setup";

import { readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, describe, expect, test } from "bun:test";

import { agentPlace } from "@agency/agent/agent-place";
import { herdrModel } from "@agency/agent/herdr-model";
import { readRunning, readStartFailure, recordStarted, standResident, startLooping, type ResidentProgram } from "@agency/agent/resident-pane";
import { codeRoot } from "@agency/support/roots";
import { scratchDir } from "@agency/test-support/scratch";
import { headCommand } from "@agency/agent/units";

// The resident program an agent's pane holds, as the launcher's side writes, replaces and reads it:
// offered, replaced when the launcher asks for a different program, waited on until it is up, and
// taken away. What the head does with the place is `pane-head`'s, tested in its own crate; here the
// head is stood in for by writing what it would write.

const root = scratchDir("bmo-resident-pane-");
const realDataDir = process.env.BMO_DATA_DIR;

process.env.BMO_DATA_DIR = root;

afterAll(() => {
  if (realDataDir === undefined) delete process.env.BMO_DATA_DIR;
  else process.env.BMO_DATA_DIR = realDataDir;

  rmSync(root, { recursive: true, force: true });
});

const sh = (script: string) => ({ args: ["/bin/sh", "-c", script], cwd: root, env: { PATH: "/usr/bin:/bin" } });

const program = (identity: string, first: string, again: string | null): ResidentProgram => ({
  identity,
  command: sh(first),
  restart: again === null ? null : sh(again),
});

const home = (agent: string) => ({ agent, label: agent, head: { command: headCommand(agent, codeRoot), cwd: codeRoot, env: { PATH: process.env.PATH ?? "", BMO_DATA_DIR: root } } });

// A stand-in herdr answering from the model, for the launcher's side that stands the workspace up.
function herdr(name: string) {
  const path = join(root, `${name}.sock`);
  const model = herdrModel();
  const server = Bun.listen<undefined>({
    unix: path,
    socket: {
      data(connection, chunk) {
        for (const line of chunk.toString().split("\n").filter(Boolean)) {
          const request = JSON.parse(line);

          connection.write(`${JSON.stringify({ id: request.id, result: model.answer(request.method, request.params ?? {}) ?? null })}\n`);
          connection.end();
        }
      },
    },
  });

  return { path, model, stop: () => server.stop(true) };
}

// A check that lets the wait go on for a while and then throws, as a turn cancelled meanwhile does.
function stoppedAfter(ms: number): () => void {
  const by = Date.now() + ms;

  return () => {
    if (Date.now() >= by) throw new Error("execution cancelled");
  };
}

describe("the launcher before a turn", () => {
  test("a program that has not come up is waited on until the turn is stopped, with no clock ending the wait", async () => {
    const socket = herdr("never");

    try {
      const answer = standResident({ place: agentPlace("never"), home: home("never"), program: program("X", "true", null), socket: socket.path, pollMs: 10, check: stoppedAfter(200) });

      await expect(answer).rejects.toThrow("execution cancelled");
    } finally {
      socket.stop();
    }
  });

  // A head stood up on a PATH without the CLI on it can never start its program; it says why through
  // the place, and the turn ends on it at once.
  test("a program whose executable is not on its PATH ends the wait at once, naming both", async () => {
    const agent = "missing";
    const place = agentPlace(agent);
    const socket = herdr(agent);
    const missing: ResidentProgram = { identity: "M", command: { args: ["no-such-program-bmo"], cwd: root, env: { PATH: "/usr/bin:/bin" } }, restart: null };
    const reason = "no-such-program-bmo: no such executable on PATH=/usr/bin:/bin";

    try {
      const answer = standResident({ place, home: home(agent), program: missing, socket: socket.path, pollMs: 20 });

      // The head, finding the program it was handed unstartable, writes why beside it.
      writeFileSync(join(place, "start-failed.json"), JSON.stringify({ identity: "M", at: Date.now(), reason }));

      const stood = await answer;

      expect(stood.ok ? "" : stood.error.code).toBe("program_not_started");
      expect(stood.ok ? "" : stood.error.message).toContain(reason);
      expect(readStartFailure(place)?.identity).toBe("M");
    } finally {
      socket.stop();
    }
  });
});

describe("starting again is bounded", () => {
  test("six starts within five minutes is a program in a loop", () => {
    const now = 10_000_000;
    const running = (recent: number[]) => ({ identity: "x", pid: 0, birth: null, loop: { pid: 1, birth: "x" }, starts: recent.length, startedAt: now, recent });

    expect(startLooping(running([1, 2, 3, 4, 5].map((at) => now - at * 30_000)), now)).toBe(false);
    expect(startLooping(running([1, 2, 3, 4, 5, 6].map((at) => now - at * 30_000)), now)).toBe(true);
    expect(startLooping(running([1, 2, 3, 4, 5, 6].map((at) => now - at * 60_000)), now)).toBe(false);
    expect(startLooping(null, now)).toBe(false);
  });

  test("a program resting after a loop reads as one in a loop whatever its starts", () => {
    const now = 10_000_000;

    expect(startLooping({ identity: "x", pid: 0, birth: null, loop: { pid: 1, birth: "x" }, starts: 6, startedAt: now - 600_000, recent: [], restingSince: now - 300_000 }, now)).toBe(true);
  });
});

describe("a place that will not stand", () => {
  test("the turn's words go to the pane holding the program, not to the first head listed", async () => {
    const agent = "held-pane";
    const place = agentPlace(agent);
    const socket = herdr("held-pane");
    const running = Bun.spawn(["sleep", "30"]);
    const birth = readFileSync(`/proc/${running.pid}/stat`, "utf8").split(") ").at(-1)!.split(" ")[19]!;

    try {
      await Bun.write(join(place, "program.json"), JSON.stringify(program("H", "unused", null)));
      recordStarted(place, { identity: "H", pid: running.pid, birth, loop: { pid: process.pid, birth: "x" }, starts: 1, startedAt: Date.now(), paneId: "w9:p7", recent: [Date.now()] });

      const stood = await standResident({ place, home: home(agent), program: program("H", "unused", null), socket: socket.path, pollMs: 10 });

      expect(stood.ok && stood.result.paneId).toBe("w9:p7");
    } finally {
      running.kill();
      socket.stop();
    }
  });

  test("a program in a start loop has its place rebuilt before the turn, once, and a loop after that is not up", async () => {
    const agent = "crash-looping";
    const place = agentPlace(agent);
    const socket = herdr("crash-looping");
    const running = Bun.spawn(["sleep", "30"]);
    const birth = readFileSync(`/proc/${running.pid}/stat`, "utf8").split(") ").at(-1)!.split(" ")[19]!;
    const ended: number[] = [];
    const closed: string[] = [];
    const now = Date.now();

    try {
      await Bun.write(join(place, "program.json"), JSON.stringify(program("L", "unused", null)));
      recordStarted(place, { identity: "L", pid: running.pid, birth, loop: { pid: process.pid, birth: "x" }, starts: 40, startedAt: now, paneId: "w9:p3", recent: [1, 2, 3, 4, 5, 6].map((at) => now - at * 10_000) });

      // Nothing runs the stand-in's panes, so the program never comes back: the turn waits on it until
      // it is stopped.
      const stood = standResident({
        place, home: home(agent), program: program("L", "unused", null), socket: socket.path, pollMs: 10,
        end: (pid) => { ended.push(pid); running.kill(); },
        close: async (paneId) => { closed.push(paneId); },
        check: stoppedAfter(200),
      });

      await expect(stood).rejects.toThrow("execution cancelled");
      expect(ended).toEqual([running.pid]);
      expect(closed).toEqual(["w9:p3"]);
      // The start history is cleared and the count kept, so the next start resumes the session.
      expect(readRunning(place)?.recent).toEqual([]);
      expect(readRunning(place)?.starts).toBe(40);

      // Looping again within the minute, the place is not rebuilt a second time, and the turn ends on
      // a program that is not up.
      const looping = readRunning(place)!;
      recordStarted(place, { ...looping, recent: [1, 2, 3, 4, 5, 6].map((at) => Date.now() - at * 10_000) });

      const again = await standResident({ place, home: home(agent), program: program("L", "unused", null), socket: socket.path, pollMs: 10, close: async (paneId) => { closed.push(paneId); } });

      expect(again.ok).toBe(false);
      expect(again.ok ? "" : again.error.code).toBe("program_not_up");
      expect(again.ok ? "" : again.error.message).toContain("keeps dying");
      expect(closed).toEqual(["w9:p3"]);
    } finally {
      running.kill();
      socket.stop();
    }
  });
});

