import "@agency/test-support/setup";

import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { codeRoot } from "@agency/support/roots";
import { SERVER_UNIT, runPath, servicePath } from "@agency/support/service-path";

// The PATH a run apart is handed (docs/agent-control.md): the launcher's own directories first, then every
// directory agency's server's unit names that the launcher lacks.

describe("the PATH a run apart is handed", () => {
  const unit = readFileSync(join(codeRoot, "deploy", `${SERVER_UNIT}.service`), "utf8");

  test("reads the directories the server's unit names, its home in place of %h", () => {
    const dirs = servicePath(unit, "/home/someone");

    expect(dirs).toContain("/home/someone/.local/bin");
    expect(dirs).toContain("/home/someone/.bun/bin");
    expect(dirs.at(-1)).toBe("/bin");
  });

  test("keeps the launcher's own directories first and adds the unit's it lacks", () => {
    const path = runPath("/opt/stand-in:/usr/bin", servicePath(unit, "/home/someone")).split(":");

    expect(path.slice(0, 2)).toEqual(["/opt/stand-in", "/usr/bin"]);
    expect(path).toContain("/home/someone/.bun/bin");
    expect(path.filter((dir) => dir === "/usr/bin")).toHaveLength(1);
  });
});
