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

# The proxy's router, which holds one table entry: pair-0-1 with thinking off by its template words.
class Router:
    def get_model_list(self, model_name=None):
        return [{"litellm_params": {"chat_template_kwargs": {"enable_thinking": False}}}] if model_name == "pair-0-1" else []
proxy_server = types.ModuleType("litellm.proxy.proxy_server")
proxy_server.llm_router = Router()
sys.modules["litellm.proxy"] = types.ModuleType("litellm.proxy")
sys.modules["litellm.proxy.proxy_server"] = proxy_server

spec = importlib.util.spec_from_file_location("sampling", sys.argv[1])
sampling = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sampling)

cases = json.load(sys.stdin)
answers = [asyncio.run(sampling.handler.async_pre_call_hook(None, None, case, "anthropic_messages")) for case in cases]
print(json.dumps(answers))
`;

type Body = Record<string, unknown>;

// Every case goes through one interpreter, since starting one is most of what a case costs.
async function hook(cases: Body[]): Promise<Body[]> {
  const child = Bun.spawn(["python3", "-c", driver, join(import.meta.dir, "../../../crates/llm-gateway/hooks/sampling.py")], {
    stdin: new Blob([JSON.stringify(cases)]),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);

  if (code !== 0) throw new Error(err);
  return JSON.parse(out);
}

const message = { model: "pair-0-1", max_tokens: 1024, temperature: 1, messages: [{ role: "user", content: "hi" }] };
const sent = (headers: Record<string, string>): Body => ({ ...message, proxy_server_request: { headers } });

describe("the gateway's sampling hook", () => {
  test("writes every setting its header names into the request, over the program's own", async () => {
    const [named, upper] = await hook([
      sent({ "x-bmo-sampling": JSON.stringify({ temperature: 0.7, top_k: 20, repeat_penalty: 1.1, dry_multiplier: 0.8, seed: 42 }) }),
      sent({ "X-BMO-Sampling": JSON.stringify({ min_p: 0.05 }) }),
    ]);

    expect(named).toMatchObject({ temperature: 0.7, top_k: 20, repeat_penalty: 1.1, dry_multiplier: 0.8, seed: 42, max_tokens: 1024 });
    expect(upper).toMatchObject({ temperature: 1, min_p: 0.05 });
  });

  test("passes over what is no name and number, and changes nothing without a header it can read", async () => {
    const mixedHeader = { "x-bmo-sampling": JSON.stringify({ top_p: 0.8, "Bad Name": 1, model: "other", seed: true, top_k: "20" }) };
    const [mixed, broken, list, none] = await hook([
      sent(mixedHeader),
      sent({ "x-bmo-sampling": "{not json" }),
      sent({ "x-bmo-sampling": "[1, 2]" }),
      message,
    ]);

    expect(mixed).toEqual({ ...sent(mixedHeader), top_p: 0.8 });
    expect(broken).toEqual(sent({ "x-bmo-sampling": "{not json" }));
    expect(list).toEqual(sent({ "x-bmo-sampling": "[1, 2]" }));
    expect(none).toEqual(message);
  });

  test("lays the template's words over the request's own and those over its table entry's", async () => {
    const header = { "x-bmo-sampling": JSON.stringify({ chat_template_kwargs: { preserve_thinking: true, "Bad Name": true, mode: "fast" } }) };
    const [entry, own, unknown] = await hook([
      sent(header),
      { ...sent(header), chat_template_kwargs: { enable_thinking: true } },
      { ...sent(header), model: "other" },
    ]);

    expect(entry!.chat_template_kwargs).toEqual({ enable_thinking: false, preserve_thinking: true });
    expect(own!.chat_template_kwargs).toEqual({ enable_thinking: true, preserve_thinking: true });
    expect(unknown!.chat_template_kwargs).toEqual({ preserve_thinking: true });
  });

  test("takes the thinking of every answer before the last question out where the template is told not to keep it", async () => {
    const thought = (text: string) => ({ type: "thinking", thinking: text, signature: "s" });
    const call = (id: string) => ({ type: "tool_use", id, name: "Read", input: { file_path: "/tmp/dossier créé/a b.txt" } });
    const result = (id: string) => ({ type: "tool_result", tool_use_id: id, content: "file" });
    const messages = [
      { role: "user", content: "first" },
      { role: "assistant", content: [thought("a"), call("t1")] },
      { role: "user", content: [result("t1")] },
      { role: "assistant", content: [thought("only")] },
      { role: "assistant", content: [thought("b"), { type: "text", text: "answer" }] },
      { role: "user", content: [result("t1"), { type: "text", text: "second" }] },
      { role: "assistant", content: [thought("c"), call("t2")] },
      { role: "user", content: [result("t2")] },
    ];
    const asked = (words: Record<string, boolean>) => ({ ...sent({ "x-bmo-sampling": JSON.stringify({ chat_template_kwargs: words }) }), messages });
    const chat = { ...message, chat_template_kwargs: { preserve_thinking: false }, messages: [
      { role: "user", content: "first" },
      { role: "assistant", content: "answer", reasoning_content: "a", thinking_blocks: [thought("a")] },
      { role: "user", content: "second" },
      { role: "assistant", content: "", reasoning_content: "b", tool_calls: [] },
      { role: "tool", content: "file" },
    ] };
    const [off, on, chatOff] = await hook([asked({ preserve_thinking: false }), asked({ preserve_thinking: true }), chat]);

    expect(off!.messages).toEqual([
      messages[0],
      { role: "assistant", content: [call("t1")] },
      messages[2],
      messages[3],
      { role: "assistant", content: [{ type: "text", text: "answer" }] },
      messages[5],
      messages[6],
      messages[7],
    ]);
    expect(on!.messages).toEqual(messages);
    expect(chatOff!.messages).toEqual([chat.messages[0], { role: "assistant", content: "answer" }, ...chat.messages.slice(2)]);
  });
});
