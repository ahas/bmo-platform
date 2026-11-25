// The deployment's own verbs the agent base starts by their path from a tree's root: the command a
// place's status hooks call. They are the deployment's scripts, so the deployment names them as its configuration loads (`registrations.ts` for agency's)
// and declares that it runs them; a read with nothing composed fails, naming what is missing.

export interface AgentVerbs {
  // The command a place's status line and its Stop, UserPromptSubmit and SessionStart hooks call: a
  // binary's path from a tree's root, then the words it is started with before the call's own.
  statusLine: readonly [string, ...string[]];
}

export class AgentVerbsUncomposed extends Error {
  constructor() {
    super("no agent verbs are composed: a process reads its configuration before it launches a job or writes a place's settings, or imports the deployment's registrations itself");
  }
}

let composed: AgentVerbs | null = null;

export function composeAgentVerbs(verbs: AgentVerbs): void {
  composed = verbs;
}

export function agentVerbs(): AgentVerbs {
  if (composed === null) {
    throw new AgentVerbsUncomposed();
  }

  return composed;
}
