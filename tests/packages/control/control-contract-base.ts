// Where the control API's contract tests reach agency's routes (docs/api.md): agency's own server,
// standing on the store the test hands it, its control port and its quota source each on a port the
// machine picks.

import { workspaceBinary } from "@agency/test-support/binaries";
import { codeRoot, dataRoot } from "@agency/support/roots";

/** The port a contract test speaks to, and how it is taken down. */
export interface ControlBase {
  port: number;
  close(): Promise<void>;
}

/** The binary a file standing the server runs, as Cargo built it before the tests. */
export function frontBinary(): string {
  return workspaceBinary("agency-server");
}

/**
 * Agency's server under `token`, on the test's own data root. `env` is what the test stands it on beyond
 * its own environment; `BMO_ARCHIVE_URL` among it names the store it keeps rows in.
 */
export async function frontAlone(binary: string, token: string, env: Record<string, string> = {}): Promise<ControlBase> {
  // It stands in the checkout, so the agents' documents it writes as it starts are this checkout's,
  // written into the test home's agents directory.
  const standing = Bun.spawn([binary, "--data-root", dataRoot()], {
    cwd: codeRoot,
    env: { ...process.env, ...env, AGENCY_CONTROL_PORT: "0", AGENCY_CONTROL_TOKEN: token, AGENCY_QUOTA_PORT: "0" },
    stdin: "ignore",
    stdout: "pipe",
    stderr: "inherit",
  });
  const close = async (): Promise<void> => {
    standing.kill();
    await standing.exited;
  };

  try {
    return { port: await listening(standing.stdout), close };
  } catch (error) {
    await close();
    throw error;
  }
}

// The port the server says its control routes are on, on the line it prints once they listen; what it says
// of its other listeners before that is passed over.
async function listening(output: ReadableStream<Uint8Array>): Promise<number> {
  const decoder = new TextDecoder();
  const reader = output.getReader();
  let said = "";

  for (;;) {
    const { done, value } = await reader.read();

    if (value !== undefined) said += decoder.decode(value, { stream: true });

    const port = said
      .split("\n")
      .slice(0, -1)
      .map((line) => /^agency: control routes on http:\/\/[^\s]+:(\d+)\s*$/.exec(line)?.[1])
      .find((found) => found !== undefined);

    if (port !== undefined) {
      reader.releaseLock();

      return Number(port);
    }

    if (done) throw new Error(`the server did not say where its control routes are: ${said.trim() || "nothing"}`);
  }
}
