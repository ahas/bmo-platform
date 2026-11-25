// What a span is called where a person reads it. The names a span carries are the CLI's own —
// a tool is called what the tool is called in the protocol, and a Claude Workflow branch is filed
// under the index it ran at — and neither is a word anyone says out loud. Every screen that
// shows a breakdown reads its names through here, so the same span is called the same thing on
// the console as in the account the script prints.
//
// The vocabulary is code and not a row in the archive: every key here has to match what the CLI
// actually sends, and a key a person could edit could only ever break that match. Nothing is
// hidden by a name the lists do not carry — an unknown tool or server stands under the name it
// came with, so a tool that first runs tomorrow reads plainly tomorrow. The model half is not
// written out again: a model has one face on this console and it is the one every other screen
// reads it by.
//
// The bot keeps a copy of this vocabulary for what it prints; the two are held to the same keys
// by this module's own test, because two copies that drift are the only way this reads one way
// on the screen and another in the account.

import { mcpCallOf } from "#shared/mcp-names";
import { BRANCH_WORD } from "#shared/text-marks";

import { modelLabel } from "./status";

import type { TodoSpan } from "#shared/types";
import type { Translate } from "#shared/wording";

// A span as this module reads it: what level it belongs to and what it is called. Nothing here
// asks how long it took, so a figure gathered anywhere can be named by the same call.
export type NamedSpan = Pick<TodoSpan, "kind" | "name">;

// The CLI's own tools in the words the work is talked about in. What a call did is the question
// the breakdown answers, so each is named by the act rather than by the class that runs it. Each
// tool holds the key of its word.
const TOOL_FACES: Record<string, string> = {
  Bash: "tool.label.bash",
  BashOutput: "tool.label.bash_output",
  KillShell: "tool.label.kill_shell",
  Read: "tool.label.read",
  Write: "tool.label.write",
  Edit: "tool.label.edit",
  NotebookEdit: "tool.label.notebook_edit",
  Glob: "tool.label.glob",
  Grep: "tool.label.grep",
  Task: "tool.label.coworker",
  Agent: "tool.label.coworker",
  TaskOutput: "tool.label.task_output",
  TaskStop: "tool.label.task_stop",
  Workflow: "tool.label.workflow",
  ToolSearch: "tool.label.tool_search",
  Skill: "tool.label.skill",
  Monitor: "tool.label.monitor",
  SendMessage: "tool.label.send_message",
  ScheduleWakeup: "tool.label.schedule_wakeup",
  TodoWrite: "tool.label.todo_write",
  WebSearch: "tool.label.web_search",
  WebFetch: "tool.label.web_fetch"
};

// The MCP servers this machine puts on an agent's line, by what a reader would call them. A
// server not named here keeps the name it registered under: the shape still says which server
// and which tool, and inventing a word for a server nobody has met would say less.
const SERVER_FACES: Record<string, string> = {
  "chrome-devtools": "tool.label.server_chrome_devtools",
  toolbook: "tool.label.server_toolbook",
  web: "tool.label.server_web"
};


// A co-worker opened as a tool call, as the tally files it: the tool that opened it, the kind of
// agent in brackets where the call named one, and enough of the instruction to tell one branch
// from the next.
const COWORKER = /^(?:Task|Agent)(?:\(([^)]*)\))?(?: · (.+))?$/;


// The tool whose seconds are the main thread waiting on a Claude Workflow's branches. It is the one
// name in the list that measures nothing of its own: the branches did the work and each is a
// span already, so a breakdown that shows both puts the same stretch of the run in twice.
const BACKGROUND_WAIT_TOOL = "TaskOutput";

// Between the halves of a name — a branch's role and its model, a server and its tool.
const JOIN = " · ";

export function toolFace(t: Translate, name: string): string {
  // An MCP tool, by the server and the tool its logged name says, whichever road wrote it.
  const mcp = mcpCallOf(name);

  if (mcp !== null) {
    const face = SERVER_FACES[mcp.server];

    return `${face === undefined ? mcp.server : t(face)}${JOIN}${mcp.tool}`;
  }

  const face = TOOL_FACES[name];

  return face === undefined ? name : t(face);
}

// The whole of a span's name as it is read out. A Claude Workflow branch is read role first and model
// after, because what a reader is asking of a breakdown is what that stretch of the run was
// doing and only then what did it; the index it ran at answers neither, so it stands only where
// there is no role to stand in its place. A phase is already the owner's own word and is left
// exactly as it came.
export function spanFace(t: Translate, span: NamedSpan): string {
  if (span.kind === "agent") {
    return agentFace(t, span.name);
  }

  return span.kind === "tool" ? toolFace(t, span.name) : span.name;
}

// What of the run's inside is worth showing. The wait on a Claude Workflow is dropped where the
// branches it waited on are standing in the list themselves — they are the same seconds under a
// name that says what happened in them. An item that has such a wait and no branch to show
// keeps it: something held the run for that long and the reader is owed the figure, whatever
// the reason it cannot be broken down.
export function shownInside<Span extends NamedSpan>(inside: readonly Span[]): Span[] {
  const branches = inside.some((span) => span.kind === "agent");

  return branches ? inside.filter((span) => !isBackgroundWait(span)) : [...inside];
}

function isBackgroundWait(span: NamedSpan): boolean {
  return span.kind === "tool" && span.name === BACKGROUND_WAIT_TOOL;
}

// An agent span by its two shapes: a branch of a Claude Workflow, which the CLI hands over with a role
// and a model, and a co-worker opened as a tool call, which carries neither — its own definition
// pins its model, and the call says only what it was asked.
function agentFace(t: Translate, name: string): string {
  const branch = branchFace(name);

  if (branch !== null) {
    return branch;
  }

  const coworker = COWORKER.exec(name);

  if (coworker === null) {
    return name;
  }

  const [, kind, said] = coworker;
  const head = kind === undefined || kind === "" ? t("tool.label.coworker") : t("tool.label.coworker_kind", { kind });

  return said === undefined || said === "" ? head : `${head}${JOIN}${said}`;
}

// A Claude Workflow branch read back out of the name it was filed under. The model is told from the
// role by what a model identifier looks like, which is what lets a branch that was handed only
// one of the two still be read: a role alone stands alone, and a model alone leaves the index
// standing in the role's place, since a branch named by nothing else is only its number.
function branchFace(name: string): string | null {
  const parts = name.split(JOIN);
  const index = parts[0] ?? "";

  if (!index.startsWith(`${BRANCH_WORD} `)) {
    return null;
  }

  const rest = parts.slice(1);
  const model = rest.length > 0 && isModelId(rest[rest.length - 1] ?? "") ? rest.pop() : undefined;
  const role = rest.join(JOIN);
  const head = role === "" ? index : role;

  return model === undefined ? head : `${head}${JOIN}${modelLabel(model)}`;
}

// Whether a part of a branch's name is the model it ran on. A model the console knows answers
// for itself; one it does not is still a model where it is written the way the vendor writes
// them, and a role is a word of the Claude Workflow's own and never is.
function isModelId(part: string): boolean {
  return modelLabel(part) !== part || part.startsWith("claude-");
}
