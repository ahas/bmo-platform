import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  AGENT_BLOCKED,
  attachWithoutLoss,
  herdrCall,
  herdrLineCapBytes,
  herdrSocketPath,
  isBlocked,
  liveRuns,
  ownRunName,
  readFrame,
  RUN_NAME_VARIABLE,
  TOO_LARGE,
} from "@agency/agent/herdr";

// The one place that speaks to herdr. What is held here is the part that is easy to get wrong and
// invisible when it is: the order attaching happens in, and the difference between a refusal that
// means "a person has to answer this" and one that means the call failed.

describe("where the socket is", () => {
  test("XDG_CONFIG_HOME moves it, because that is the only thing that does", () => {
    expect(herdrSocketPath({ XDG_CONFIG_HOME: "/tmp/somewhere" })).toBe("/tmp/somewhere/herdr/herdr.sock");
  });

  test("and with nothing said it sits under the home configuration directory", () => {
    expect(herdrSocketPath({})).toMatch(/\/\.config\/herdr\/herdr\.sock$/);
    // An empty value is not a path; it reads as nothing said.
    expect(herdrSocketPath({ XDG_CONFIG_HOME: "" })).toMatch(/\/\.config\/herdr\/herdr\.sock$/);
  });

  // HERDR_CONFIG_PATH looks like it should move the socket and does not — it moves the
  // configuration file and leaves the socket where it was.
  test("the configuration path is not the socket path", () => {
    expect(herdrSocketPath({ HERDR_CONFIG_PATH: "/tmp/elsewhere/config.toml" })).toMatch(
      /\/\.config\/herdr\/herdr\.sock$/,
    );
  });
});

describe("one line off the socket", () => {
  test("a result is a result", () => {
    expect(readFrame<{ type: string }>('{"id":"q1","result":{"type":"ok"}}')).toEqual({
      ok: true,
      result: { type: "ok" },
    });
  });

  test("an error is carried rather than thrown, so the caller can tell which one it is", () => {
    const answer = readFrame('{"id":"q1","error":{"code":"agent_blocked","message":"agent w3:p1 is blocked"}}');

    expect(answer.ok).toBe(false);
    expect(isBlocked(answer)).toBe(true);
  });

  test("a blocked agent is the one refusal that is not a failure", () => {
    const blocked = readFrame(`{"error":{"code":"${AGENT_BLOCKED}","message":"needs a person"}}`);
    const timedOut = readFrame('{"error":{"code":"timeout","message":"timed out waiting for agent status"}}');

    expect(isBlocked(blocked)).toBe(true);
    // Every other refusal is not it, including the one that arrives from the same method.
    expect(isBlocked(timedOut)).toBe(false);
    expect(isBlocked({ ok: true, result: 1 })).toBe(false);
  });

  test("something that is not a frame at all is its own failure", () => {
    const answer = readFrame("herdr server running; you can use any herdr CLI command");

    expect(answer.ok).toBe(false);
    expect(answer.ok ? "" : answer.error.code).toBe("unreadable");
  });
});

// The measured order: subscribe, read the ack, buffer, snapshot, install, then apply what was
// buffered. An event raised between the ack and the snapshot is absent from the snapshot and
// present on the stream exactly once, so the buffer is the only thing that keeps it.
describe("attaching without losing an event", () => {
  test("an event that lands while the snapshot is in flight is applied once, and after the install", async () => {
    const order: string[] = [];
    let raise: ((event: string) => void) | null = null;

    await attachWithoutLoss<string, string>({
      subscribe: async (buffer) => {
        raise = buffer;
        order.push("ack");
      },
      snapshot: async () => {
        // Exactly the race the measurement constructed: this arrives after the subscription is
        // acknowledged and before the snapshot is installed.
        raise?.("during");
        order.push("snapshot");

        return "picture";
      },
      install: (snapshot) => order.push(`install:${snapshot}`),
      apply: (event) => order.push(`apply:${event}`),
    });

    expect(order).toEqual(["ack", "snapshot", "install:picture", "apply:during"]);
    // Once. A second delivery would be a duplicate applied to a snapshot that already has it.
    expect(order.filter((step) => step === "apply:during")).toHaveLength(1);
  });

  test("several of them keep the order they arrived in", async () => {
    const applied: string[] = [];
    let raise: ((event: string) => void) | null = null;

    await attachWithoutLoss<string, string>({
      subscribe: async (buffer) => {
        raise = buffer;
      },
      snapshot: async () => {
        raise?.("first");
        raise?.("second");
        raise?.("third");

        return "picture";
      },
      install: () => undefined,
      apply: (event) => applied.push(event),
    });

    expect(applied).toEqual(["first", "second", "third"]);
  });

  test("and once it is installed an event goes straight through rather than into the buffer", async () => {
    const applied: string[] = [];
    const stream: { raise: ((event: string) => void) | null } = { raise: null };

    await attachWithoutLoss<string, string>({
      subscribe: async (buffer) => {
        stream.raise = buffer;
      },
      snapshot: async () => "picture",
      install: () => undefined,
      apply: (event) => applied.push(event),
    });

    stream.raise?.("after");

    expect(applied).toEqual(["after"]);
  });

  test("nothing in the window means nothing applied, which is the ordinary case", async () => {
    const applied: string[] = [];

    await attachWithoutLoss<string, string>({
      subscribe: async () => undefined,
      snapshot: async () => "picture",
      install: () => undefined,
      apply: (event) => applied.push(event),
    });

    expect(applied).toEqual([]);
  });
});

// A run is the token on its pane. That is the whole mapping from a row in the archive to a pane,
// and a label — which is only what a person reads — names nothing.
describe("which runs herdr still holds", () => {
  const snapshot = {
    workspaces: [
      { workspace_id: "w1", label: "mother worker" },
      { workspace_id: "w2", label: "bmo-gone" },
    ],
    panes: [
      { pane_id: "w1:p1", workspace_id: "w1", label: "mother worker" },
      { pane_id: "w1:p2", workspace_id: "w1", label: "#322 herdr screen", tokens: { run: "bmo-task-1789" } },
      { pane_id: "w2:p1", workspace_id: "w2", tokens: { run: "bmo-run-308-stage-1" } },
    ],
  };

  test("a name herdr carries is alive", () => {
    expect([...liveRuns(snapshot, ["bmo-task-1789"])]).toEqual(["bmo-task-1789"]);
  });

  test("a name it does not carry is over", () => {
    expect([...liveRuns(snapshot, ["bmo-task-0000"])]).toEqual([]);
  });

  test("asking about several answers about several, and never invents one", () => {
    const alive = liveRuns(snapshot, ["bmo-task-1789", "bmo-run-308-stage-1", "bmo-gone"]);

    expect(alive.has("bmo-task-1789")).toBe(true);
    expect(alive.has("bmo-run-308-stage-1")).toBe(true);
    expect(alive.has("bmo-gone")).toBe(false);
    expect(alive.size).toBe(2);
  });

  // A freshly restarted herdr answers with an empty picture, and that is exactly the case where
  // being wrong matters: it must read as "these runs are gone", never as "no answer, assume alive".
  test("an empty snapshot says every run is gone, which is what a restarted herdr means", () => {
    expect(liveRuns({}, ["bmo-task-1789"]).size).toBe(0);
    expect(liveRuns({ workspaces: [] }, ["bmo-task-1789"]).size).toBe(0);
  });

  test("a label is never taken for a run's name", () => {
    expect(liveRuns(snapshot, ["bmo-gone", "mother worker"]).size).toBe(0);
  });
});

describe("the name a run was given", () => {
  test("is read from the environment, since a pane has no cgroup to read it from", () => {
    expect(ownRunName({ [RUN_NAME_VARIABLE]: "bmo-task-1789" })).toBe("bmo-task-1789");
  });

  test("and a run nobody named has none to give", () => {
    expect(ownRunName({})).toBeNull();
    expect(ownRunName({ [RUN_NAME_VARIABLE]: "" })).toBeNull();
  });
});

// A call over a real socket, because the checks above are all of the pure parts and a whole class
// of fault lives outside them: the first version of this module ended the connection before it
// handed back what it had read, and ending it runs the close handler there and then, so every
// answer was thrown away and reported as "closed". Nothing that stops short of opening a socket
// could have caught that, so this opens one. The server here answers the way herdr does — one
// line back, then it hangs up.
describe("a call over a real socket", () => {
  const socketFor = (name: string): string => join(tmpdir(), `bmo-herdr-${name}-${process.pid}.sock`);

  async function answering(name: string, reply: (line: string) => string | null): Promise<{ path: string; stop: () => void }> {
    const path = socketFor(name);

    rmSync(path, { force: true });

    const server = Bun.listen({
      unix: path,
      socket: {
        data(connection, chunk) {
          const answer = reply(chunk.toString());

          if (answer !== null) {
            connection.write(answer);
          }

          // herdr hangs up after answering, which is the case that broke the first version.
          connection.end();
        },
      },
    });

    return { path, stop: () => { server.stop(true); rmSync(path, { force: true }); } };
  }

  test("hands back the line the server wrote, rather than reporting the hang-up", async () => {
    const { path, stop } = await answering("ok", () => '{"id":"x","result":{"type":"session_snapshot"}}\n');

    try {
      const answer = await herdrCall<{ type: string }>("session.snapshot", {}, { socket: path });

      expect(answer.ok).toBe(true);
      expect(answer.ok ? answer.result.type : "").toBe("session_snapshot");
    } finally {
      stop();
    }
  });

  test("carries a refusal through as a refusal, with its code intact", async () => {
    const { path, stop } = await answering(
      "blocked",
      () => `{"id":"x","error":{"code":"${AGENT_BLOCKED}","message":"needs a person"}}\n`,
    );

    try {
      const answer = await herdrCall("agent.prompt", { target: "w1:p1", text: "hi" }, { socket: path });

      expect(isBlocked(answer)).toBe(true);
    } finally {
      stop();
    }
  });

  test("a request past herdr's line is refused as too large and never sent", async () => {
    let reached = 0;
    const { path, stop } = await answering("large", () => {
      reached += 1;
      return null;
    });

    try {
      const answer = await herdrCall("agent.prompt", { target: "w1:p1", text: "x".repeat(herdrLineCapBytes) }, { socket: path });

      expect(answer.ok ? "" : answer.error.code).toBe(TOO_LARGE);
      expect(reached).toBe(0);
    } finally {
      stop();
    }
  });

  test("a server that hangs up saying nothing is the one real closed-before-answering", async () => {
    const { path, stop } = await answering("mute", () => null);

    try {
      const answer = await herdrCall("ping", {}, { socket: path });

      expect(answer.ok).toBe(false);
      expect(answer.ok ? "" : answer.error.code).toBe("closed");
    } finally {
      stop();
    }
  });

  // herdr drops a connection whose line never came, which is what a caller whose thread a busy disk
  // held meets with herdr well (docs/agent-control.md, 「herdr」). A socket that refuses the
  // connection stands in for it here: a plain file where the socket stands takes no connection.
  test("a request that never reached herdr is sent again, and answered once herdr takes it", async () => {
    const path = socketFor("late");
    const pauses: number[] = [];
    let server = null as { stop: () => void } | null;

    writeFileSync(path, "");

    try {
      const answer = await herdrCall<{ type: string }>("session.snapshot", {}, {
        socket: path,
        pause: async (ms) => {
          pauses.push(ms);
          server ??= await answering("late", () => '{"id":"x","result":{"type":"session_snapshot"}}\n');
        },
      });

      expect(answer.ok ? answer.result.type : answer.error.code).toBe("session_snapshot");
      expect(pauses).toHaveLength(1);
    } finally {
      server?.stop();
      rmSync(path, { force: true });
    }
  });

  test("a herdr that never takes it is given up on after three more sendings, and one with no socket at once", async () => {
    const standing = socketFor("refusing");
    const pauses: number[] = [];
    const pause = async (ms: number) => pauses.push(ms);

    writeFileSync(standing, "");

    try {
      expect(await herdrCall("ping", {}, { socket: standing, pause })).toMatchObject({ ok: false, error: { code: "unreachable" } });
      expect(pauses).toHaveLength(3);

      pauses.length = 0;

      expect(await herdrCall("ping", {}, { socket: socketFor("absent"), pause })).toMatchObject({ ok: false, error: { code: "unreachable" } });
      expect(pauses).toEqual([]);
    } finally {
      rmSync(standing, { force: true });
    }
  });

  test("a request written whole is never sent again, though herdr hung up without answering", async () => {
    const pauses: number[] = [];
    let asked = 0;
    const { path, stop } = await answering("mute-once", () => {
      asked += 1;

      return null;
    });

    try {
      const answer = await herdrCall("ping", {}, { socket: path, pause: async (ms) => pauses.push(ms) });

      expect(answer.ok ? "" : answer.error.code).toBe("closed");
      expect(asked).toBe(1);
      expect(pauses).toEqual([]);
    } finally {
      stop();
    }
  });

  // herdr reads a request to the end of its line and gives up on one left short. A socket takes about
  // 214KB in one write, and a work-order stage's prompt is longer than that.
  test("a request longer than the socket takes at once reaches the server whole", async () => {
    const path = socketFor("long");
    const text = "€".repeat(200_000);

    rmSync(path, { force: true });

    const server = Bun.listen<{ held: string; reading: TextDecoder }>({
      unix: path,
      socket: {
        open(connection) {
          connection.data = { held: "", reading: new TextDecoder() };
        },
        data(connection, chunk) {
          connection.data.held += connection.data.reading.decode(chunk, { stream: true });

          if (!connection.data.held.endsWith("\n")) return;

          const request = JSON.parse(connection.data.held) as { params: { text: string } };

          connection.write(`${JSON.stringify({ id: "x", result: { length: request.params.text.length } })}\n`);
          connection.end();
        },
      },
    });

    try {
      const answer = await herdrCall<{ length: number }>("agent.prompt", { target: "w1:p1", text }, { socket: path });

      expect(answer.ok ? answer.result.length : answer.error.code).toBe(text.length);
    } finally {
      server.stop(true);
      rmSync(path, { force: true });
    }
  });

  test("no herdr there at all is a fact the caller can act on, not a throw", async () => {
    const answer = await herdrCall("ping", {}, { socket: socketFor("absent") });

    expect(answer.ok).toBe(false);
    expect(["unreachable", "closed"]).toContain(answer.ok ? "" : answer.error.code);
  });
});
