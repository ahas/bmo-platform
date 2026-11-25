// A binary of agency's own workspace as a TypeScript test starts it: the root package's copy, which Cargo
// built before its tests, read out of the directory the Rust harness hands the file in
// `AGENCY_TEST_BINARIES`, where every one stands under its own name (tests/typescript_binaries.rs).
//
// No test builds a binary. A file that starts one is named `*.cargo-test.ts`, outside Bun's own naming, so
// the suite's Bun run passes it over and the Rust harness runs it under Bun with the directory named.

import { existsSync } from "node:fs";
import { join } from "node:path";

/** The variable naming the directory the built binaries stand in. */
export const BINARIES_VARIABLE = "AGENCY_TEST_BINARIES";

/** The path of the workspace binary named `name`, as Cargo built it before the tests. */
export function workspaceBinary(name: string): string {
  const dir = process.env[BINARIES_VARIABLE];

  if (dir === undefined || dir === "") {
    throw new Error(`${name} is a built binary, and nothing names where they stand: a file starting one is a *.cargo-test.ts run by tests/typescript_binaries.rs`);
  }

  const path = join(dir, name);

  if (!existsSync(path)) {
    throw new Error(`${name} is no binary the root package builds for its tests (${dir})`);
  }

  return path;
}
