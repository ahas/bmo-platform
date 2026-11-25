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

spec = importlib.util.spec_from_file_location("reminders", sys.argv[1])
reminders = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reminders)

cases = json.load(sys.stdin)
answers = [asyncio.run(reminders.handler.async_pre_call_hook(None, None, case["data"], case["call_type"])) for case in cases]
print(json.dumps(answers))
`;

type Body = Record<string, unknown>;

// Every case goes through one interpreter, since starting one is most of what a case costs.
async function hook(cases: { data: Body; call_type?: string }[]): Promise<Body[]> {
  const child = Bun.spawn(["python3", "-c", driver, join(import.meta.dir, "../../../crates/llm-gateway/hooks/reminders.py")], {
    stdin: new Blob([JSON.stringify(cases.map((c) => ({ call_type: "anthropic_messages", ...c })))]),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);

  if (code !== 0) throw new Error(err);
  return JSON.parse(out);
}

const environment = { role: "system", content: [{ type: "text", text: "# Environment\nToday's date is 2026-10-05." }] };
const wrapped = { type: "text", text: "<system-reminder>\n# Environment\nToday's date is 2026-10-05.\n</system-reminder>" };
const words = { role: "user", content: "hello" };
const answer = { role: "assistant", content: [{ type: "text", text: "yes?" }] };
const body = (messages: unknown[]) => ({ model: "pair-0-1", max_tokens: 1024, system: [{ type: "text", text: "persona" }], messages });

describe("the gateway's reminder hook", () => {
  test("folds a system turn into the person's turn beside it, ahead of the words", async () => {
    const [first, later, results, leading, between, plain, chat] = await hook([
      // A session's first request, as Claude Code sends it: the environment after the first words.
      { data: body([words, environment]) },
      // The same conversation sent again on the next request is folded the same way.
      { data: body([words, environment, answer, { role: "user", content: [{ type: "text", text: "more" }] }]) },
      { data: body([{ role: "user", content: [{ type: "tool_result", tool_use_id: "t1", content: "ok" }, { type: "text", text: "report" }] }, { role: "system", content: "notice" }]) },
      { data: body([environment, words]) },
      { data: body([words, answer, { role: "system", content: "notice" }, answer]) },
      { data: body([words]) },
      { data: { model: "pair-0-1", messages: [words, environment] }, call_type: "acompletion" },
    ]);

    expect(first).toEqual(body([{ role: "user", content: [wrapped, { type: "text", text: "hello" }] }]));
    expect(later).toEqual(body([{ role: "user", content: [wrapped, { type: "text", text: "hello" }] }, answer, { role: "user", content: [{ type: "text", text: "more" }] }]));
    // Tool results stay first, as the tool calls they answer need them.
    expect(results).toEqual(body([{ role: "user", content: [{ type: "tool_result", tool_use_id: "t1", content: "ok" }, { type: "text", text: "<system-reminder>\nnotice\n</system-reminder>" }, { type: "text", text: "report" }] }]));
    // With no user turn before it, it goes into the one after it.
    expect(leading).toEqual(body([{ role: "user", content: [wrapped, { type: "text", text: "hello" }] }]));
    // With no user turn beside it, it is left as it came.
    expect(between).toEqual(body([words, answer, { role: "system", content: "notice" }, answer]));
    expect(plain).toEqual(body([words]));
    // Only the Messages path is rewritten.
    expect(chat).toEqual({ model: "pair-0-1", messages: [words, environment] });
  });

  test("leaves a run after the last answer, folds a run of several in order, and keeps a turn's other blocks", async () => {
    const picture = { type: "image", source: { type: "base64", media_type: "image/png", data: "AA==" } };
    const [afterLast, several, blocks] = await hook([
      // With no user turn after it either, a run at the end is left to LiteLLM's own turning.
      { data: body([words, answer, { role: "system", content: "notice" }]) },
      { data: body([words, { role: "system", content: "one" }, { role: "system", content: [{ type: "text", text: "two" }, { type: "text", text: "three" }] }]) },
      { data: body([words, { role: "system", content: [{ type: "text", text: "notice" }, picture] }, { role: "system", content: "" }]) },
    ]);

    expect(afterLast).toEqual(body([words, answer, { role: "system", content: "notice" }]));
    // Each message of the run is wrapped on its own, in its order, its text blocks joined by lines.
    expect(several).toEqual(body([{
      role: "user",
      content: [
        { type: "text", text: "<system-reminder>\none\n</system-reminder>" },
        { type: "text", text: "<system-reminder>\ntwo\nthree\n</system-reminder>" },
        { type: "text", text: "hello" },
      ],
    }]));
    // A block that is no text follows its turn's wrapped text, and a turn with no text adds nothing.
    expect(blocks).toEqual(body([{
      role: "user",
      content: [{ type: "text", text: "<system-reminder>\nnotice\n</system-reminder>" }, picture, { type: "text", text: "hello" }],
    }]));
  });
});
