import "@agency/test-support/setup";

import { afterEach, describe, expect, test } from "bun:test";

import { defaultGatewayUrl, gatewaySamplingHeaders, gatewayUrl } from "@agency/agent/gateway/address";
import { registeredEndpoint } from "@agency/agent/registered-provider";

// Every caller of agency's gateway reads its address in one place (docs/own-llm.md, 「The gateway」).

const held = process.env.BMO_LLM_GATEWAY_URL;

afterEach(() => {
  if (held === undefined) delete process.env.BMO_LLM_GATEWAY_URL;
  else process.env.BMO_LLM_GATEWAY_URL = held;
});

describe("the gateway's address", () => {
  test("is the bot's own gateway unless the process names another", () => {
    expect(gatewayUrl({})).toBe(defaultGatewayUrl);
    expect(gatewayUrl({ BMO_LLM_GATEWAY_URL: "  " })).toBe(defaultGatewayUrl);
    expect(gatewayUrl({ BMO_LLM_GATEWAY_URL: "http://127.0.0.1:18139/" })).toBe("http://127.0.0.1:18139");
  });

  test("is read by a registered run as the process names it", () => {
    process.env.BMO_LLM_GATEWAY_URL = "http://127.0.0.1:18139";

    expect(registeredEndpoint.baseUrl).toBe("http://127.0.0.1:18139");

    delete process.env.BMO_LLM_GATEWAY_URL;

    expect(registeredEndpoint.baseUrl).toBe(defaultGatewayUrl);
  });
});

describe("a run's sampling settings", () => {
  test("go to the gateway in the one header its sampling hook reads, and a run that names none sends none", () => {
    expect(gatewaySamplingHeaders({ temperature: 0.7, top_k: 20 })).toEqual({ "x-bmo-sampling": "{\"temperature\":0.7,\"top_k\":20}" });
    expect(gatewaySamplingHeaders({ chat_template_kwargs: { preserve_thinking: false } })).toEqual({ "x-bmo-sampling": "{\"chat_template_kwargs\":{\"preserve_thinking\":false}}" });
    expect(gatewaySamplingHeaders({})).toEqual({});
    expect(gatewaySamplingHeaders(undefined)).toEqual({});
  });
});
