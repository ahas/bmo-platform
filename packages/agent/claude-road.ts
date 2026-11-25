import { dataPath } from "@agency/support/roots";

import { gatewayUrl } from "./gateway/address";

// Where the registered models keep a program's records, and the gateway they are served through
// (`docs/own-llm.md`, 「How requests go」): the folder `registered-provider.ts` gives every
// registration's home.
const registeredHomes = () => `${dataPath("claude-roads", "registered")}/`;

// Whether a Claude Code program's environment makes it the registered models' road rather than the
// Claude Code road's own (docs/agent-control.md, 「The registered models through Claude Code on the
// gateway」): its records home is a registration's, or it is pointed at agency's gateway. An
// endpoint that is a Claude account's own — another provider's Anthropic-compatible endpoint under
// that account's home — is not this road: it stays on the Claude Code road, opens its sessions
// there and is closed when that road's provider is off.
export function registeredRoad(env: Readonly<Record<string, string | undefined>>): boolean {
  const home = env.CLAUDE_CONFIG_DIR;
  const underHome = home !== undefined && home.startsWith(registeredHomes());
  const named = env.ANTHROPIC_BASE_URL?.trim().replace(/\/+$/, "");

  return underHome || (named !== undefined && named !== "" && named === gatewayUrl(env));
}
