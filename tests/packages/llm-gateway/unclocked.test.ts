import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { join } from "node:path";

// The hook runs inside the gateway's LiteLLM, which the machine's own Python does not carry. The base
// class it extends and the wait it hands over are stood in for, so what is checked is the hook's own
// reading of a request: a wait with no limit reads back as the word `unlimited`.
const driver = `
import asyncio, importlib.util, json, sys, types

custom_logger = types.ModuleType("litellm.integrations.custom_logger")
custom_logger.CustomLogger = type("CustomLogger", (), {})
for name in ("litellm", "litellm.integrations"):
    sys.modules[name] = types.ModuleType(name)
sys.modules["litellm.integrations.custom_logger"] = custom_logger

class Timeout:
    def __init__(self, limit):
        self.limit = limit
httpx = types.ModuleType("httpx")
httpx.Timeout = Timeout
sys.modules["httpx"] = httpx

spec = importlib.util.spec_from_file_location("unclocked", sys.argv[1])
unclocked = importlib.util.module_from_spec(spec)
spec.loader.exec_module(unclocked)

def read(answer):
    if answer is None:
        return None
    if isinstance(answer.get("timeout"), Timeout):
        answer["timeout"] = "unlimited" if answer["timeout"].limit is None else answer["timeout"].limit
    return answer

cases = json.load(sys.stdin)
answers = [read(asyncio.run(unclocked.handler.async_pre_call_deployment_hook(case, None))) for case in cases]
print(json.dumps(answers))
`;

type Kwargs = Record<string, unknown>;

// Every case goes through one interpreter, since starting one is most of what a case costs.
async function hook(cases: Kwargs[]): Promise<(Kwargs | null)[]> {
  const child = Bun.spawn(["python3", "-c", driver, join(import.meta.dir, "../../../crates/llm-gateway/hooks/unclocked.py")], {
    stdin: new Blob([JSON.stringify(cases)]),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);

  if (code !== 0) throw new Error(err);
  return JSON.parse(out);
}

// A call as the router hands it to the deployment: the wait it resolved, the entry's model information and
// the request's headers.
const call = (info: Kwargs | undefined, headers: Record<string, string>): Kwargs => ({
  model: "openai/served",
  timeout: 6000,
  stream_timeout: 6000,
  ...(info === undefined ? {} : { model_info: info }),
  proxy_server_request: { headers },
});

describe("the gateway's unclocked hook", () => {
  test("hands every request a wait with no limit, whatever its entry and its headers say", async () => {
    const answers = await hook([
      call(undefined, {}),
      call({ role_files: true }, { "x-bmo-role": "translator" }),
      call(undefined, { "x-litellm-timeout": "1800" }),
      call(undefined, { "X-LiteLLM-Stream-Timeout": "1800" }),
    ]);

    for (const answer of answers) {
      expect(answer?.timeout).toBe("unlimited");
      expect(answer).not.toHaveProperty("stream_timeout");
    }
  });
});
