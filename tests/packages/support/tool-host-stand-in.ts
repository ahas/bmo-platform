import { join } from "node:path";

// The tool host a launch asks for its run's token, stood in on a socket in `dir` and named to the code
// under test until it is stopped: every record is issued a token of its own, and every revocation is
// taken. The issuer token the launcher sends is named to it here, so a test needs no data tree of its own.
export function toolHostStandIn(dir: string): { issued: Record<string, unknown>[]; revoked: string[]; stop: () => void } {
  const previous = { socket: process.env.BMO_MCP_HOST_SOCKET, issuer: process.env.BMO_MCP_HOST_ISSUER_TOKEN };
  const socket = join(dir, "mcp-host.sock");
  const issued: Record<string, unknown>[] = [];
  const revoked: string[] = [];
  const server = Bun.serve({
    unix: socket,
    async fetch(request) {
      const path = new URL(request.url).pathname;

      if (request.method === "POST" && path === "/tool-sessions") {
        const record = (await request.json()) as Record<string, unknown>;
        const token = `token-${issued.length + 1}`;

        issued.push(record);
        return Response.json(
          {
            token,
            holderName: record.holderName ?? "",
            namespaces: record.namespaces ?? [],
            capabilities: record.capabilities ?? [],
            grants: record.grants ?? [],
            shown: record.shown ?? [],
            variables: record.variables ?? {},
            variablesFile: record.variablesFile ?? null,
            labels: record.labels ?? {},
            expiresAt: null,
            createdAt: Date.now(),
          },
          { status: 201 },
        );
      }

      if (request.method === "DELETE" && path.startsWith("/tool-sessions/")) {
        revoked.push(decodeURIComponent(path.slice("/tool-sessions/".length)));
        return new Response(null, { status: 204 });
      }

      return new Response(null, { status: 404 });
    },
  });

  process.env.BMO_MCP_HOST_SOCKET = socket;
  process.env.BMO_MCP_HOST_ISSUER_TOKEN = "stand-in-issuer";

  return {
    issued,
    revoked,
    stop: () => {
      server.stop(true);
      if (previous.socket === undefined) delete process.env.BMO_MCP_HOST_SOCKET; else process.env.BMO_MCP_HOST_SOCKET = previous.socket;
      if (previous.issuer === undefined) delete process.env.BMO_MCP_HOST_ISSUER_TOKEN; else process.env.BMO_MCP_HOST_ISSUER_TOKEN = previous.issuer;
    },
  };
}

// Run as a process of its own, for a test whose code under test runs while the test itself waits on
// it: the stand-in serves on the socket in the directory named until it is killed.
if (import.meta.main) {
  const dir = process.argv[2];

  if (dir === undefined) throw new Error("usage: bun testbed/testkit/tool-host-stand-in.ts <dir>");

  toolHostStandIn(dir);
}
