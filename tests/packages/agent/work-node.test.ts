import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";

import { nodeHeadEnv } from "@agency/agent/work-node";

// A worker's place on a work node as this machine reaches it (docs/work-nodes.md, 「The link」).

const roots = { node: "n1", root: "/n/home/.local/share/bmo-node", home: "/n/home" };

describe("this machine's paths on a node", () => {
  test("a head on the node starts on the node's own environment, nothing of this machine's carried", () => {
    const env = nodeHeadEnv(roots, "bmo-head-w");

    expect(env).toEqual(expect.objectContaining({ HOME: "/n/home", BMO_DATA_DIR: "/n/home/.local/share/bmo-node/data", BMO_HOME_BRIDGE: "/n/home/.local/share/bmo-node/data/bridge.sock", BMO_NODE: "n1" }));
    expect(Object.keys(env).some((name) => name.includes("TOKEN"))).toBe(false);
  });
});

