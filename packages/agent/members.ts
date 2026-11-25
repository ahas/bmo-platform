import { readFileSync, statSync } from "node:fs";

import { dataPath } from "@agency/support/roots";

// The gateway's members as the deployment declares them (docs/own-llm.md, 「The gateway's members」,
// docs/harness-machine.md, 「The members」): the model names served beside the registry, the road each
// belongs to, its transport, and what a run on it is sized to. The file is the deployment's; nothing here
// writes it.

export interface Member {
  name: string;
  road?: string;
  label?: string;
  provider: string;
  model: string;
  api_base?: string;
  key_file: string;
  context_tokens?: number;
  answer_tokens?: number;
  thinking?: boolean;
}

// Where the file stands: the path BMO_LLM_GATEWAY_MEMBERS names, or `<data root>/llm-gateway/members.json`.
export function membersPath(): string {
  const named = process.env.BMO_LLM_GATEWAY_MEMBERS?.trim() ?? "";

  return named === "" ? dataPath("llm-gateway", "members.json") : named;
}

// Every member the file holds; a file that does not read, or holds no list, is no members.
export function readMembers(): Member[] {
  try {
    const parsed: unknown = JSON.parse(readFileSync(membersPath(), "utf8"));

    return Array.isArray(parsed) ? (parsed as Member[]) : [];
  } catch {
    return [];
  }
}

// The members of one road.
export function roadMembers(road: string): Member[] {
  return readMembers().filter((member) => member.road === road);
}

// The member of a road one name holds.
export function memberNamed(road: string, name: string): Member | undefined {
  return roadMembers(road).find((member) => member.name === name);
}

// The key file a member names, under the data root's env/.
export function memberKeyPath(member: Member): string {
  return dataPath("env", member.key_file);
}

// Whether a key file may be used: a file of its own, private to its owner, not empty.
export function usableKeyFile(path: string): boolean {
  try {
    const stat = statSync(path);

    return stat.isFile() && (stat.mode & 0o777) === 0o600 && stat.size > 0;
  } catch {
    return false;
  }
}
