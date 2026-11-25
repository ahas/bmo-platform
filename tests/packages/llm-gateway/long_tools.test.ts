import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { join } from "node:path";

// The hook runs inside the gateway's LiteLLM, which the machine's own Python does not carry. The base
// class it extends is stood in for, so what is checked is the hook's own reading of a request.
const driver = `
import asyncio, importlib.util, json, sys, types

custom_logger = types.ModuleType("litellm.integrations.custom_logger")
custom_logger.CustomLogger = type("CustomLogger", (), {})
for name in ("litellm", "litellm.integrations"):
    sys.modules[name] = types.ModuleType(name)
sys.modules["litellm.integrations.custom_logger"] = custom_logger

spec = importlib.util.spec_from_file_location("long_tools", sys.argv[1])
long_tools = importlib.util.module_from_spec(spec)
spec.loader.exec_module(long_tools)

cases = json.load(sys.stdin)
answers = [asyncio.run(long_tools.handler.async_pre_call_hook(None, None, case, "anthropic_messages")) for case in cases]
print(json.dumps(answers))
`;

type Body = Record<string, unknown>;

// Every case goes through one interpreter, since starting one is most of what a case costs.
async function hook(cases: Body[]): Promise<Body[]> {
  const child = Bun.spawn(["python3", "-c", driver, join(import.meta.dir, "../../../crates/llm-gateway/hooks/long_tools.py")], {
    stdin: new Blob([JSON.stringify(cases)]),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);

  if (code !== 0) throw new Error(err);
  return JSON.parse(out);
}

const message = { model: "pair-0-1", max_tokens: 1024, messages: [{ role: "user", content: "hi" }] };
const sent = (headers: Record<string, string>): Body => ({ ...message, proxy_server_request: { headers } });

describe("the gateway's long-tools hook", () => {
  test("copies its header's value unchanged into the field the engines read, whatever the header's case", async () => {
    const [named, upper, odd] = await hook([
      sent({ "x-bmo-long-tools": "Bash,Agent,Task,WebFetch,WebSearch" }),
      sent({ "X-BMO-Long-Tools": "Bash" }),
      sent({ "x-bmo-long-tools": " Bash, mcp__web__web_fetch ,," }),
    ]);

    expect(named).toEqual({ ...sent({ "x-bmo-long-tools": "Bash,Agent,Task,WebFetch,WebSearch" }), bmo_long_tools: "Bash,Agent,Task,WebFetch,WebSearch" });
    expect(upper?.bmo_long_tools).toBe("Bash");
    expect(odd?.bmo_long_tools).toBe(" Bash, mcp__web__web_fetch ,,");
  });

  test("writes no field where the request carries no such header or an empty one", async () => {
    const [none, empty, bare] = await hook([sent({ "x-bmo-sampling": "{}" }), sent({ "x-bmo-long-tools": "" }), message]);

    expect(none).toEqual(sent({ "x-bmo-sampling": "{}" }));
    expect(empty).toEqual(sent({ "x-bmo-long-tools": "" }));
    expect(bare).toEqual(message);
  });
});
