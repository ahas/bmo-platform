import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { ownIdentity } from "@agency/agent/agent-host";
import { installPlacer } from "@agency/agent/agent-process";
import { herdrModel } from "@agency/agent/herdr-model";
import { supervisePane, type Placer } from "@agency/agent/pane-process";
import { toolHostStandIn } from "@agency/test-support/tool-host-stand-in";

// A herdr of a test's own that really starts what a pane is given: the arrangement answers from the
// model, and every pane made with a command gets that command as a process, with the pane's id in
// its environment the way herdr hands it over. A pane closed takes its process with it, and a
// process that ends takes its pane. Beside it stand the two hosts a launch asks of the machine until
// the fixture is closed: the tool host, stood in on a socket of its own, and the agent host, a host
// inside the test's own process.
export function processPaneFixture(options: {
  // What a pane is really started with, for a test that stands something else in for a command.
  rewrite?: (command: string[]) => string[];
  env?: Record<string, string>;
  // A directory each pane's own output is written into, one file named by its pane id, for a test
  // about what a pane shows.
  output?: string;
} = {}) {
  const root = mkdtempSync(join(tmpdir(), "process-pane-"));
  mkdirSync(join(root, "herdr"));
  const toolHost = toolHostStandIn(root);
  installPlacer(inProcessHost());
  const model = herdrModel();
  const children = new Map<string, ReturnType<typeof Bun.spawn>>();
  const launched: { paneId: string; command: string[] }[] = [];
  const start = (paneId: string, node: { command?: string[]; cwd?: string; env?: Record<string, string> }) => {
    if (!node.command?.length) return;
    const child = Bun.spawn(options.rewrite?.(node.command) ?? node.command, {
      cwd: node.cwd, env: { ...process.env, ...(node.env ?? {}), ...(options.env ?? {}), HERDR_PANE_ID: paneId }, stdin: "ignore", stdout: options.output === undefined ? "ignore" : Bun.file(join(options.output, paneId)), stderr: "ignore",
    });
    children.set(paneId, child);
    launched.push({ paneId, command: node.command });
    void child.exited.then(() => { if (children.get(paneId) === child) { children.delete(paneId); model.exit(paneId); } });
  };
  const server = Bun.listen({ unix: join(root, "herdr/herdr.sock"), socket: {
    data(socket, bytes) {
      const request = JSON.parse(bytes.toString());
      const params = (request.params ?? {}) as Record<string, unknown>;
      const before = new Set(model.panes().map(pane => pane.pane_id));
      let result = model.answer(request.method, params);
      if (request.method === "pane.read") {
        socket.end(JSON.stringify({ id: request.id, error: { code: "invalid_request", message: "read requires rendering options" } }) + "\n");
        return;
      }
      if (request.method === "layout.apply" && result) {
        const paneId = (result as { layout: { root: { pane_id: string } } }).layout.root.pane_id;
        start(paneId, params.root as { command?: string[]; cwd?: string; env?: Record<string, string> });
      }
      const after = new Set(model.panes().map(pane => pane.pane_id));
      for (const paneId of before) {
        if (!after.has(paneId)) { const child = children.get(paneId); children.delete(paneId); child?.kill("SIGHUP"); }
      }
      if (result === undefined) result = { type: "ok" };
      if (result === null) {
        socket.end(JSON.stringify({ id: request.id, error: { code: "not_found", message: `${request.method} refused` } }) + "\n");
        return;
      }
      socket.end(JSON.stringify({ id: request.id, result }) + "\n");
    },
  } });
  return {
    root,
    model,
    launched,
    // The process a pane holds, for a test that ends one abruptly.
    process: (paneId: string) => children.get(paneId),
    async close() {
      server.stop(true);
      toolHost.stop();
      installPlacer(null);
      const running = [...children.values()];
      children.clear();
      for (const child of running) child.kill("SIGTERM");
      await Promise.race([Promise.all(running.map(child => child.exited)), Bun.sleep(5_000)]);
      for (const child of running) child.kill("SIGKILL");
      await Promise.all(running.map(child => child.exited));
      rmSync(root, { recursive: true, force: true });
    },
  };
}

// A host inside the test's own process: what is put in is supervised right here, with no herdr and
// no pane. For a test about what a caller does with a run's answer rather than about where it ran.
export function inProcessHost(): Placer {
  return async request => {
    void supervisePane(request.directory).catch(() => {});
    return { ok: true, result: { paneId: "in-process", host: ownIdentity(), reensure: async () => ({ ok: true, result: ownIdentity() }) } };
  };
}
