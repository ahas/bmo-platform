import { randomUUID } from "node:crypto";

import { agentPlace } from "./agent-place";
import { endpointPlaceSettings, endpointReply, recordsDirIn, settleEndpointHome } from "./claude-code";
import { cliProjectsRoot, transcriptPath } from "./cli-transcripts";
import { ExecutionStopped } from "./pane-process";
import { lineEnvironment } from "./units";
import { lines, roundLead, runTurnInPane } from "./worker-pane";

import type { ClaudeEndpoint } from "./claude-code";
import type { HarnessRun, ResidentSurface, ResidentTurnRequest, ResidentTurnResult } from "./harness";

// One turn of Claude Code on another's endpoint, standing in the agent's pane (docs/own-llm.md,
// 「Claude Code in the pane」). A turn handed a session goes on in it, in the program already standing
// on it; any other opens a session of its own, and the program is stood up for it on that session, so
// nothing of an earlier job rides along into a window that may hold tens of thousands of tokens. The
// words go in the way a person's would, and the answer is read out of the transcript the program
// writes under the road's home.

export async function endpointTurn(options: ResidentTurnRequest & { surface: ResidentSurface; endpoint: ClaudeEndpoint }): Promise<ResidentTurnResult> {
  const place = agentPlace(options.home.agent);
  const continued = options.run.resumeSessionId;
  const sessionId = continued ?? randomUUID();
  const run: HarnessRun = {
    ...options.run,
    resumeSessionId: continued,
    openSessionId: continued === null ? sessionId : null,
    keepSession: true,
    resident: { place, settingsPath: endpointPlaceSettings(options.run, place, await options.endpoint.model(options.run)) },
  };

  settleEndpointHome((await options.endpoint.model(run)).home, run.cwd);

  const command = await options.surface.command(run);
  const variables = Object.fromEntries(Object.entries(command.env).filter((entry): entry is [string, string] => entry[1] !== undefined));
  const turn = await runTurnInPane({
    provider: options.endpoint.provider,
    line: run.line,
    identity: { root: options.root, parent: options.parent, model: run.model, effort: run.effort || null },
    name: options.name,
    title: options.title,
    home: options.home,
    cwd: command.cwd,
    command: command.args,
    env: lineEnvironment(command.env, variables, options.name),
    prompt: run.prompt,
    sessionId,
    // A session gone on in is read from where its record stands now.
    from: continued === null ? 0 : lines(transcriptPath(command.cwd, sessionId, cliProjectsRoot(recordsDirIn(command.env)))).length,
    trustsFolder: true,
    interrupt: options.surface.interrupt,
    aside: options.aside,
    ...(run.line === "chat" ? { lead: roundLead } : {}),
    ...(options.socket === undefined ? {} : { socket: options.socket }),
  });

  if (turn.outcome === "answered") return { reply: endpointReply(options.endpoint, { ...turn.reply, model: run.model }) };
  if (turn.outcome === "stopped") throw new ExecutionStopped(`${options.name}: the turn was stopped`);

  // The program's own words where it wrote them in the answer's place; a screen is never words to hand
  // on, so every other ending is named by what it was (docs/agent-control.md, 「What a failed turn may
  // say」).
  return { error: turn.outcome === "errored" ? turn.error : `Claude Code's turn ended ${turn.outcome} with no answer` };
}
