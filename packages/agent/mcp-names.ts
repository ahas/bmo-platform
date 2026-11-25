// The MCP server and tool a logged call was, read out of the name its road wrote the line under:
// the Claude road's `mcp__<server>__<tool>`, and Codex's `<server>/<tool>`, whose result line is
// named `MCP`. The run log writes both shapes (`runlog.ts`), and this is the one place that reads
// them back; a reader of the log — the bot's or the console's — asks it rather than spelling either
// shape itself. It is also the one place that knows the toolbook's proxy calls, so a call made
// through the toolbook is written and read as the tool it reached. It stands in a module of its
// own, holding nothing else, so a screen can ask it too.

export interface LoggedMcpCall {
  server: string;
  tool: string;
}

const claudeMcpName = /^mcp__([^_]+(?:_[^_]+)*)__(.+)$/;
const codexMcpName = /^([A-Za-z0-9][\w.-]*)\/([\w.-]+)$/;

export function mcpCallOf(name: string | null | undefined): LoggedMcpCall | null {
  const found = name === null || name === undefined ? null : (claudeMcpName.exec(name) ?? codexMcpName.exec(name));

  return found === null ? null : { server: found[1]!, tool: found[2]! };
}

// A call as the run log records it: the tool it reached and the arguments that tool was handed.
export interface CalledTool {
  name: string;
  input: unknown;
  throughToolbook: boolean;
}

// The toolbook reaches a booked tool through two proxy tools of its own, one
// for read-only tools and one for tools that act. Each is handed the tool's full name — its
// namespace and its name joined by a slash — and the tool's own arguments as one object.
const toolbookServer = "toolbook";
const toolbookProxies = new Set(["call", "act"]);
const innerFullName = /^([A-Za-z0-9][\w.-]*)\/([\w.-]+)$/;

/**
 * The tool a call reached, read from the name it was made under and what it was handed. A
 * toolbook proxy call is answered as the inner call, its name in the same road's spelling as a
 * direct call would have, so every reader of a logged name sees the tool and never the proxy.
 * Every other call — `list` and `describe` among them — comes back as it was made, and so does a
 * proxy call too malformed to name a tool, since the proxy is then the truest name it has.
 */
export function calledTool(name: string, input: unknown): CalledTool {
  const asMade = { name, input, throughToolbook: false };
  const proxy = mcpCallOf(name);

  if (proxy === null || proxy.server !== toolbookServer || !toolbookProxies.has(proxy.tool)) return asMade;
  if (!isObject(input) || typeof input["tool"] !== "string" || !isObject(input["arguments"])) return asMade;

  const inner = innerFullName.exec(input["tool"]);

  if (inner === null) return asMade;

  const [, server, tool] = inner;
  const claudeRoad = claudeMcpName.test(name);

  return {
    name: claudeRoad ? `mcp__${server}__${tool}` : `${server}/${tool}`,
    input: input["arguments"],
    throughToolbook: true,
  };
}

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
