// The MCP server and tool a logged call was, read out of the name its road wrote the line under: the Claude
// road's `mcp__<server>__<tool>`, and Codex's `<server>/<tool>`. The agency's run log writes both shapes, and a
// screen reading a log line asks this rather than spelling either shape itself.

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
