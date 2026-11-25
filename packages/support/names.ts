// Who the agent this deployment runs is, and who its owner is, in the words its text uses
// (docs/policy.md, 「The deployment's names」). Each name is the policy's neutral default, or the
// environment variable beside it where that is set and not empty, as agency's Rust side reads them; a
// process that knows better registers its own. Nothing here knows whose names they are.

import declared from "@agency/policy/deployment.toml";

export interface DeploymentNames {
  // The name the agent is called by in a room.
  agent: string;
  // The agent's name in the records: the agent row a unified chat stands under.
  agentKey: string;
  // The name the agent's own messages are stored under.
  sender: string;
  // The owner's name, as a line addressed to the owner or about the owner writes it.
  owner: string;
}

interface DeclaredName {
  default: string;
  variable: string;
}

let registered: DeploymentNames | null = null;

/** The names a process registers in place of the policy's. */
export function registerNames(names: DeploymentNames): void {
  registered = names;
}

function named(name: DeclaredName, env: NodeJS.ProcessEnv): string {
  const set = env[name.variable]?.trim();

  return set === undefined || set === "" ? name.default : set;
}

/** The deployment's names: the registered ones, or the policy's as the environment names them. */
export function deploymentNames(env: NodeJS.ProcessEnv = process.env): DeploymentNames {
  if (registered !== null) return registered;

  return {
    agent: named(declared.agent_name as DeclaredName, env),
    agentKey: named(declared.agent_key as DeclaredName, env),
    sender: named(declared.sender_name as DeclaredName, env),
    owner: named(declared.owner_name as DeclaredName, env),
  };
}
