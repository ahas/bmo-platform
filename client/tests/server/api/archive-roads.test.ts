import { afterAll, describe, expect, test } from "bun:test";

import { asked, routeOf } from "../../support/nitro";

import type { RouteEvent } from "../../support/nitro";

// The archive's and the share page's roads, handed on whole to a stood-in bmo-artifact: what is pinned here is that
// a road reaches bmo-artifact with its method, its query, its body, the headers it was asked with and the console's
// token; that the answer comes back as bmo-artifact gave it, bytes and headers alike; that a share link is made by
// the signed-in user whoever the browser names; that a refusal with one of bmo-artifact's codes is said in the pack's
// words; that the share roads take no sign-in and only read; and that an unset address is the unit not there.

interface Heard {
  method: string;
  path: string;
  headers: Record<string, string>;
  body: string;
}

const heard: Heard[] = [];
const PICTURE = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 1, 2, 3]);

const artifact = Bun.serve({
  hostname: "127.0.0.1",
  port: 0,
  async fetch(request) {
    const url = new URL(request.url);

    heard.push({ method: request.method, path: `${url.pathname}${url.search}`, headers: Object.fromEntries(request.headers), body: await request.text() });

    if (url.pathname === "/api/archive") return Response.json({ items: [{ id: 7 }], hasMore: false });
    if (url.pathname === "/api/archive/7/attachments/2") {
      return new Response(PICTURE, { status: 206, headers: { "content-type": "image/jpeg", "content-disposition": 'inline; filename="a.jpg"', "content-range": "bytes 0-6/7", "x-unit-only": "1" } });
    }
    if (url.pathname === "/api/archive/7/share" && request.method === "POST") return Response.json({ share: { token: "t".repeat(22), expiresAt: null } });
    if (url.pathname === "/api/archive/8/share") return Response.json({ error: "a link already stands", code: "already_shared" }, { status: 409 });
    if (url.pathname === "/api/archive/9") return Response.json({ error: "something no code names", code: "unheard_of" }, { status: 422 });
    if (url.pathname.startsWith("/api/share/")) return Response.json({ title: "A shared document" }, { headers: { "cache-control": "private, no-store" } });

    return Response.json({ error: "no route", code: "not_found" }, { status: 404 });
  }
});

process.env.ARTIFACT_ADDRESS = `http://127.0.0.1:${artifact.port}`;
process.env.ARTIFACT_TOKEN = "artifact-token";

afterAll(() => {
  void artifact.stop(true);
  delete process.env.ARTIFACT_ADDRESS;
  delete process.env.ARTIFACT_TOKEN;
});

const { wording } = await import("../../../apps/web/shared/wording");
const archiveList = await routeOf<Response>("../../apps/web/server/api/archive/index.ts");
const archive = await routeOf<Response>("../../apps/web/server/api/archive/[...path].ts");
const share = await routeOf<Response>("../../apps/web/server/api/share/[...path].ts");

const last = () => heard.at(-1)!;

describe("the archive's roads", () => {
  test("reach bmo-artifact with their query and the console's token, and answer as it answered", async () => {
    const answer = await archiveList({ query: { kind: "record", q: "a word", limit: "30" } });

    expect(last()).toMatchObject({ method: "GET", path: "/api/archive?kind=record&q=a+word&limit=30" });
    expect(last().headers.authorization).toBe("Bearer artifact-token");
    expect(answer.status).toBe(200);
    expect(await answer.json()).toEqual({ items: [{ id: 7 }], hasMore: false });
  });

  test("hand a file back as its bytes, with the headers the browser reads and the range it asked for", async () => {
    const answer = await archive({ params: { path: "7/attachments/2" }, headers: { range: "bytes=0-6", cookie: "a-session" } });

    expect(last()).toMatchObject({ method: "GET", path: "/api/archive/7/attachments/2" });
    expect(last().headers.range).toBe("bytes=0-6");
    expect(last().headers.cookie).toBeUndefined();
    expect(answer.status).toBe(206);
    expect(new Uint8Array(await answer.arrayBuffer())).toEqual(PICTURE);
    expect(answer.headers.get("content-type")).toBe("image/jpeg");
    expect(answer.headers.get("content-disposition")).toBe('inline; filename="a.jpg"');
    expect(answer.headers.get("content-range")).toBe("bytes 0-6/7");
    expect(answer.headers.get("x-unit-only")).toBeNull();
  });

  test("make a share link as the signed-in user, whoever the browser names as its maker", async () => {
    const answer = await archive({ method: "POST", params: { path: "7/share" }, headers: { "content-type": "application/json" }, rawBody: JSON.stringify({ span: "7d", by: "someone-else" }) });

    expect(last()).toMatchObject({ method: "POST", path: "/api/archive/7/share" });
    expect(JSON.parse(last().body)).toEqual({ span: "7d", by: "tester" });
    expect(last().headers["content-type"]).toBe("application/json");
    expect((await answer.json()).share.token).toHaveLength(22);
  });

  test("hand a body on as it came on any other road, and take a link away", async () => {
    await archive({ method: "DELETE", params: { path: "7/share" } });

    expect(last()).toMatchObject({ method: "DELETE", path: "/api/archive/7/share", body: "" });
  });

  test("say a refusal with one of bmo-artifact's codes in the pack's words, and hand any other on as it is", async () => {
    const refused = await asked(archive, { method: "POST", params: { path: "8/share" }, rawBody: JSON.stringify({ span: "1h" }) });

    expect(refused).toMatchObject({ status: 409, message: wording("archive.error.already_shared"), data: { code: "already_shared" } });

    const other = await archive({ params: { path: "9" } });

    expect(other.status).toBe(422);
    expect((await other.json()).code).toBe("unheard_of");
  });

  test("ask the session first", async () => {
    const globals = globalThis as unknown as Record<string, unknown>;
    const signed = globals.requireUserSession;

    globals.requireUserSession = async () => {
      throw Object.assign(new Error("no session"), { statusCode: 401 });
    };

    try {
      const before = heard.length;

      expect((await asked(archive, { params: { path: "7" } })).status).toBe(401);
      expect(heard.length).toBe(before);
    } finally {
      globals.requireUserSession = signed;
    }
  });
});

describe("the share page's roads", () => {
  test("take no sign-in and are read whole, the token in the road", async () => {
    const globals = globalThis as unknown as Record<string, unknown>;
    const signed = globals.requireUserSession;

    globals.requireUserSession = async () => {
      throw new Error("the share page's roads ask no session");
    };

    try {
      const answer = await share({ params: { path: `${"t".repeat(22)}/head` } });

      expect(last()).toMatchObject({ method: "GET", path: `/api/share/${"t".repeat(22)}/head` });
      expect(answer.headers.get("cache-control")).toBe("private, no-store");
      expect(await answer.json()).toEqual({ title: "A shared document" });
    } finally {
      globals.requireUserSession = signed;
    }
  });

  test("only read", async () => {
    const before = heard.length;

    expect(await asked(share, { method: "POST", params: { path: "t/head" } } satisfies RouteEvent)).toMatchObject({ status: 405, message: wording("common.error.read_only_road") });
    expect(heard.length).toBe(before);
  });
});

test("a console whose settings name no bmo-artifact says the unit is not there", async () => {
  const address = process.env.ARTIFACT_ADDRESS;

  delete process.env.ARTIFACT_ADDRESS;

  try {
    expect((await asked(archiveList, {})).status).toBe(503);
  } finally {
    process.env.ARTIFACT_ADDRESS = address;
  }
});
