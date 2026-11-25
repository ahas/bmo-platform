// The published package's client (docs/api.md, 「The clients」): a call reaches its route with the token,
// reads its answer as the contract's type, and reads a refusal in the contract's error shape.

import { expect, test } from "bun:test";

import { type AgencyFetch, type Health, agencyClient } from "@agency/client";

const health: Health = { ready: true, reason: null, version: "0.1.0-rev.7" };

test("a call reaches its route with the token and reads the answer", async () => {
  const asked: Request[] = [];
  const standIn: AgencyFetch = async (input) => {
    asked.push(input);

    return Response.json(health);
  };
  const client = agencyClient("http://agency.test", "the-token", standIn);
  const { data, error } = await client.GET("/health");

  expect(error).toBeUndefined();
  expect(data?.ready).toBe(true);
  expect(asked.length).toBe(1);
  expect(asked[0]!.url).toBe("http://agency.test/health");
  expect(asked[0]!.headers.get("Authorization")).toBe("Bearer the-token");
});

test("a refusal is read in the error shape", async () => {
  const standIn: AgencyFetch = async () =>
    new Response(JSON.stringify({ code: "not-found", error: "There is no supervisor tester." }), {
      status: 404,
      headers: { "Content-Type": "application/json" },
    });
  const client = agencyClient("http://agency.test", "the-token", standIn);
  const { data, error } = await client.GET("/supervisors/{supervisorName}", {
    params: { path: { supervisorName: "tester" } },
  });

  expect(data).toBeUndefined();
  expect(error?.code).toBe("not-found");
  expect(error?.error).toBe("There is no supervisor tester.");
});
