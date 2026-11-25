import type { LiveTask, RunWorkflow } from "./finished";
import type { AgentProviderName } from "./harness";
import type { ModelSpend } from "./spend";

// The way a model turn left this bot, which is the provider that carried it: the product boundary
// a person chooses, not the executable that happened to answer — dsh is an implementation while
// OpenRouter is the road whose model, usage and answer the rest of agency has to name. The name is
// its own because `mode` is the word a stored reply, a usage row and a session record call the
// field by; what it holds is one of the providers, and that list is the providers' own.
export type AgentMode = AgentProviderName;

// The account window a harness was able to read. Most harnesses know none; the Claude Code
// reader currently knows both the short and weekly windows and carries them in this common form.
export interface SessionUsage {
  utilization: number;
  resetsAt: number;
  weeklyUtilization: number | null;
  weeklyResetsAt: number | null;
}

export interface UsageWindowReading {
  name: string;
  utilization: number;
  resetsAt: number | null;
  // A provider that reports the window's length or its own display name carries them in the
  // common reading. Claude Code currently names its windows instead; absence is information,
  // not a length inferred from that name.
  durationMinutes?: number | null;
  limitName?: string | null;
}

// The one reply language every harness hands back. A field a program does not report is null or
// empty rather than a provider-shaped guess, so callers never need to know which executable made
// the answer. Claude Code and Codex fill every field they can; dsh fills text and leaves the
// session, account windows and spend it cannot observe absent.
export interface AgentReply {
  // What actually answered this turn. The launched model remains in HarnessRun; these are the
  // answer-side facts used by the session and usage views. A harness that cannot discover them
  // from stdout is handed the common run beside the output and copies them from there.
  mode: AgentMode;
  model: string | null;
  effort: string | null;
  text: string;
  sessionId: string | null;
  contextRatio: number | null;
  contextTokens: number | null;
  sessionUsage: SessionUsage | null;
  windows: UsageWindowReading[];
  spend: ModelSpend[];
  lastRequestUsage?: { inputTokens: number; cacheReadTokens: number; cacheWriteTokens: number; outputTokens: number } | null;
  // Every request of the turn, oldest first, where the harness can read them one by one.
  requestUsages?: NonNullable<AgentReply["lastRequestUsage"]>[] | null;
  liveTasks: LiveTask[];
  workflows: RunWorkflow[];
  // The run the answer's agent log lines are kept under, where whoever launched it logged it. The
  // usage rows the answer's spend becomes carry it, so an agent's page reads each run's own figure.
  logRun?: string | null;
  // Which of the provider's logins answered. The windows a reply carries are that account's own
  // share of that account's limit — two accounts report them under the very same names — so what
  // is written down has to say whose reading it is. Absent on a road that holds no login of
  // Agency's, and on a reply nobody handed the run it came from.
  account?: string | null;
  // The answer of each turn of the program the round spanned, in order, which `text` was built from,
  // with what opened that turn; absent where the harness reads no such turns.
  answers?: TurnAnswer[];
}

// One turn's answer among those a round spanned: its last response, and whether words handed over
// opened it or the program opened it on its own.
export interface TurnAnswer {
  text: string;
  opened: "handed" | "program";
}

// A harness run that keeps a resumable session may strengthen the common reply at its boundary.
// The rest of the program still speaks AgentReply; this type states the one extra guarantee only
// where the caller actually needs to store or resume the id.
export type AgentSessionReply = AgentReply & { sessionId: string };
