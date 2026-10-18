import { isAgentName } from "#shared/config";
import { wording } from "#shared/wording";

import type { KeepaliveChanged } from "#shared/types";

interface KeepaliveBody {
  agent?: unknown;
  stopped?: unknown;
  pause?: unknown;
}

// The warming stopped or started again at the cache warmer: the whole round when no agent is named, and
// one agent's subject when one is — or one worker's turned off until its next job wakes it. Nothing but
// upkeep is switched — every room is answered and every job runs exactly as before — so what is refused
// here is only a request that says nothing: no yes or no, or a name that is not an agent's.
export default defineEventHandler(async (event): Promise<KeepaliveChanged> => {
  await requireUserSession(event);

  const body = await readBody<KeepaliveBody>(event);

  // One worker's keepalive turned off until its next job: there is nothing to switch back, so the
  // request is the worker's name and nothing else.
  if (body?.pause === true) {
    if (!isAgentName(body.agent)) {
      throw fail(400, wording("agents.error.keepalive_worker"));
    }

    await pauseAgentKeepalive(consumerNamed(), body.agent);

    return { ok: true };
  }

  if (typeof body?.stopped !== "boolean") {
    throw fail(400, wording("agents.error.keepalive_direction"));
  }

  if (body.agent !== undefined && !isAgentName(body.agent)) {
    throw fail(400, wording("agents.error.no_such_agent_name"));
  }

  if (body.agent === undefined) {
    await setKeepaliveRound(body.stopped);
  } else {
    await switchAgentKeepalive(consumerNamed(), body.agent, body.stopped);
  }

  return { ok: true };
});

// An agent's own switch stands under the consumer the deployment names; with none, there is none to throw.
function consumerNamed(): string {
  const consumer = keepaliveConsumer();

  if (consumer === null) throw fail(503, wording("agents.error.keepalive_no_consumer"));

  return consumer;
}
