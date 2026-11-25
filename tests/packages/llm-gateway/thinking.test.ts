import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { join } from "node:path";

// The hook runs inside the gateway's LiteLLM, which the machine's own Python does not carry. The base
// class it extends is stood in for, so what is checked is the hook's own reading of a request body.
const driver = `
import asyncio, importlib.util, json, sys, types

custom_logger = types.ModuleType("litellm.integrations.custom_logger")
custom_logger.CustomLogger = type("CustomLogger", (), {})
for name in ("litellm", "litellm.integrations"):
    sys.modules[name] = types.ModuleType(name)
sys.modules["litellm.integrations.custom_logger"] = custom_logger

spec = importlib.util.spec_from_file_location("thinking", sys.argv[1])
thinking = importlib.util.module_from_spec(spec)
spec.loader.exec_module(thinking)

cases = json.load(sys.stdin)
answers = [asyncio.run(thinking.handler.async_pre_call_hook(None, None, case["data"], case["call_type"])) for case in cases]
print(json.dumps(answers))
`;

type Body = Record<string, unknown>;

// Every case goes through one interpreter, since starting one is most of what a case costs.
async function hook(cases: { data: Body; call_type?: string }[]): Promise<Body[]> {
  const child = Bun.spawn(["python3", "-c", driver, join(import.meta.dir, "../../../crates/llm-gateway/hooks/thinking.py")], {
    stdin: new Blob([JSON.stringify(cases.map((c) => ({ call_type: "anthropic_messages", ...c })))]),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);

  if (code !== 0) throw new Error(err);
  return JSON.parse(out);
}

const message = { model: "pair-0-1", max_tokens: 1024, messages: [{ role: "user", content: "hi" }] };

describe("the gateway's thinking hook", () => {
  test("carries the caller's choice into the chat template and leaves the rest to the table", async () => {
    const [enabled, adaptive, disabled, absent, effortAlone, named, format, chat] = await hook([
      { data: { ...message, thinking: { type: "enabled", budget_tokens: 2048 } } },
      { data: { ...message, thinking: { type: "adaptive", display: "omitted" }, output_config: { effort: "medium" } } },
      { data: { ...message, thinking: { type: "disabled" } } },
      { data: { ...message } },
      { data: { ...message, output_config: { effort: "low" } } },
      { data: { ...message, thinking: { type: "enabled", budget_tokens: 2048 }, chat_template_kwargs: { enable_thinking: false, other: 1 } } },
      { data: { ...message, thinking: { type: "adaptive" }, output_config: { effort: "high", format: { type: "json_schema" } } } },
      { data: { model: "pair-0-1", messages: [], thinking: { type: "enabled" } }, call_type: "acompletion" },
    ]);

    expect(enabled).toEqual({ ...message, chat_template_kwargs: { enable_thinking: true } });
    expect(adaptive).toEqual({ ...message, chat_template_kwargs: { enable_thinking: true } });
    expect(disabled).toEqual({ ...message, chat_template_kwargs: { enable_thinking: false } });
    // Naming nothing keeps the table's off, and the effort alone never turns thinking on.
    expect(absent).toEqual(message);
    expect(effortAlone).toEqual(message);
    // A request that names the template's switch itself is asked as it named it.
    expect(named).toEqual({ ...message, chat_template_kwargs: { enable_thinking: false, other: 1 } });
    expect(format).toEqual({ ...message, output_config: { format: { type: "json_schema" } }, chat_template_kwargs: { enable_thinking: true } });
    // Only the Messages path is rewritten; a chat request is the gates' and is asked as it came.
    expect(chat).toEqual({ model: "pair-0-1", messages: [], thinking: { type: "enabled" } });
  });
});
