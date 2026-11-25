import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { join } from "node:path";

// The hook runs inside the gateway's LiteLLM, which the machine's own Python does not carry. The base
// class it extends is stood in for, and so is the engine instance: a server on a thread that answers a
// restore of `role-chat.bin` and of a file it was asked to save, refuses every other file with a 404, keeps
// each save and restore it is asked, and tells the tokens it last read as the case before it set them.
// Every case goes through the hook in turn, in one interpreter, and the restores each one made are told
// beside it.
const driver = `
import asyncio, http.server, importlib.util, json, sys, threading, types

custom_logger = types.ModuleType("litellm.integrations.custom_logger")
custom_logger.CustomLogger = type("CustomLogger", (), {})
for name in ("litellm", "litellm.integrations"):
    sys.modules[name] = types.ModuleType(name)
sys.modules["litellm.integrations.custom_logger"] = custom_logger

spec = importlib.util.spec_from_file_location("roles", sys.argv[1])
roles = importlib.util.module_from_spec(spec)
spec.loader.exec_module(roles)

asked = []
saved = set()
read = {"tokens": 0}

class Instance(http.server.BaseHTTPRequestHandler):
    def answer(self, code, body):
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(body).encode())

    def do_GET(self):
        self.answer(200, {"conversation_cache": {"last_prompt": read["tokens"]}})

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        file = body.get("filename")
        asked.append({"path": self.path, "file": file, "key": self.headers.get("Authorization")})
        if self.path.endswith("action=save") and file not in refused:
            saved.add(file)
            self.answer(200, {})
        elif self.path.endswith("action=restore") and (file == "role-chat.bin" or file in saved):
            self.answer(200, {})
        else:
            self.answer(404, {})

    def log_message(self, *args):
        pass

server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Instance)
threading.Thread(target=server.serve_forever, daemon=True).start()
root = f"http://127.0.0.1:{server.server_address[1]}"

refused = set()

async def run(cases):
    told = []
    for case in cases:
        kwargs = {"api_base": root + case.get("path", "/v1"), "api_key": "node-key", "model_info": case.get("info")}
        headers = {}
        if case.get("role") is not None:
            headers[case.get("header", "x-bmo-role")] = case["role"]
        if case.get("session") is not None:
            headers["x-claude-code-session-id"] = case["session"]
        if headers:
            kwargs["proxy_server_request"] = {"headers": headers}
        if case.get("read") is not None:
            read["tokens"] = case["read"]
        refused.clear()
        refused.update(case.get("refuses", []))
        before = len(asked)
        answer = await roles.handler.async_pre_call_deployment_hook(kwargs, "acompletion")
        told.append({"answer": answer, "asked": asked[before:]})
    return told

print(json.dumps(asyncio.run(run(json.load(sys.stdin)))))
`;

// `read` is the tokens the instance says it last read when the case's request arrives, kept for the cases
// after it; `refuses` names the files the instance refuses to save for this case alone.
type Case = {
  role?: string;
  header?: string;
  session?: string;
  read?: number;
  refuses?: string[];
  info?: Record<string, unknown> | null;
  path?: string;
};
type Told = { answer: unknown; asked: { path: string; file: string; key: string }[] };

async function hook(cases: Case[]): Promise<Told[]> {
  const child = Bun.spawn(["python3", "-c", driver, join(import.meta.dir, "../../../crates/llm-gateway/hooks/roles.py")], {
    stdin: new Blob([JSON.stringify(cases)]),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);

  if (code !== 0) throw new Error(err);
  return JSON.parse(out);
}

const keeps = { role_files: true };

describe("the gateway's role hook", () => {
  test("restores a role's file before the role's first request on an instance and not again while it is on it", async () => {
    const [first, again, upper] = await hook([
      { role: "chat", info: keeps },
      { role: "chat", info: keeps },
      { role: "chat", header: "X-BMO-Role", info: keeps },
    ]);

    expect(first?.asked).toEqual([{ path: "/slots/0?action=restore", file: "role-chat.bin", key: "Bearer node-key" }]);
    expect(first?.answer).toBeNull();
    expect(again?.asked).toEqual([]);
    expect(upper?.asked).toEqual([]);
  });

  test("takes the instance off its role on a request naming none, so the role's next request restores again", async () => {
    const [, none, back] = await hook([{ role: "chat", info: keeps }, { info: keeps }, { role: "chat", info: keeps }]);

    expect(none?.asked).toEqual([]);
    expect(back?.asked.map((one) => one.file)).toEqual(["role-chat.bin"]);
  });

  test("lets a request whose restore was refused go as it stands, and does not ask that role of the instance again", async () => {
    const [refused, again, other] = await hook([
      { role: "gate", info: keeps },
      { role: "gate", info: keeps },
      { role: "chat", info: keeps },
    ]);

    expect(refused?.asked.map((one) => one.file)).toEqual(["role-gate.bin"]);
    expect(refused?.answer).toBeNull();
    expect(again?.asked).toEqual([]);
    expect(other?.asked.map((one) => one.file)).toEqual(["role-chat.bin"]);
  });

  test("asks nothing of an instance that keeps no role files, nor for a name that is no role", async () => {
    const told = await hook([
      { role: "chat", info: null },
      { role: "chat", info: { role_files: false } },
      { role: "../chat", info: keeps },
      { role: "chat", info: keeps, path: "/v1/chat/completions" },
    ]);

    expect(told.slice(0, 3).map((one) => one.asked)).toEqual([[], [], []]);
    expect(told[3]?.asked.map((one) => one.path)).toEqual(["/slots/0?action=restore"]);
  });
});

const did = (one: Told | undefined) => one?.asked.map((asked) => `${asked.path.split("action=")[1]} ${asked.file}`);

describe("the gateway's session files", () => {
  test("saves a long session when another takes the instance and restores it, not its role's file, when it returns", async () => {
    const [long, other, back, again] = await hook([
      { session: "long", role: "chat", info: keeps },
      { session: "other", info: keeps, read: 160_000 },
      { session: "long", role: "chat", info: keeps, read: 500 },
      { session: "long", role: "chat", info: keeps },
    ]);

    expect(did(long)).toEqual(["restore role-chat.bin"]);
    expect(did(other)).toEqual(["save session-0.bin"]);
    expect(other?.asked[0]?.key).toBe("Bearer node-key");
    expect(did(back)).toEqual(["restore session-0.bin"]);
    expect(did(again)).toEqual([]);
  });

  test("saves a long session before a request that names no session", async () => {
    const [, none, back] = await hook([
      { session: "long", info: keeps },
      { info: keeps, read: 40_000 },
      { session: "long", info: keeps },
    ]);

    expect(did(none)).toEqual(["save session-0.bin"]);
    expect(did(back)).toEqual(["restore session-0.bin"]);
  });

  test("leaves a short session to the instance, and a file it had is no longer its own", async () => {
    const [, left, returned, short, stale] = await hook([
      { session: "one", info: keeps },
      { session: "two", info: keeps, read: 40_000 },
      { session: "one", info: keeps, read: 300 },
      { session: "two", info: keeps, read: 31_999 },
      { session: "one", info: keeps, read: 300 },
    ]);

    expect(did(left)).toEqual(["save session-0.bin"]);
    expect(did(returned)).toEqual(["restore session-0.bin"]);
    expect(did(short)).toEqual([]);
    expect(did(stale)).toEqual([]);
  });

  test("keeps three files for an instance and writes a fourth session over the one least recently used", async () => {
    const sessions = ["a", "b", "c", "d", "e"];
    const told = await hook(sessions.map((session) => ({ session, info: keeps, read: 50_000 })).concat([{ session: "a", info: keeps, read: 50_000 }]));

    expect(told.map(did)).toEqual([
      [],
      ["save session-0.bin"],
      ["save session-1.bin"],
      ["save session-2.bin"],
      ["save session-0.bin"],
      ["save session-1.bin"],
    ]);
  });

  test("lets a request go as it stands where its session's save was refused", async () => {
    const [, refused, back] = await hook([
      { session: "long", role: "chat", info: keeps },
      { session: "other", info: keeps, read: 50_000, refuses: ["session-0.bin"] },
      { session: "long", role: "chat", info: keeps, read: 300 },
    ]);

    expect(did(refused)).toEqual(["save session-0.bin"]);
    expect(did(back)).toEqual(["restore role-chat.bin"]);
  });

  test("asks nothing of an instance that keeps no role files, nor for a name that is no session", async () => {
    const told = await hook([
      { session: "long", info: null },
      { session: "other", info: null, read: 50_000 },
      { session: "a b", info: keeps },
      { session: "c", info: keeps },
    ]);

    expect(told.map(did)).toEqual([[], [], [], []]);
  });
});
