import type { Archive } from "@agency/archive/archive";
import type { ExecutionRecord } from "@agency/archive/execution-store";
import { nativeExecution } from "./execution-identity";
import { providerImplementations } from "./agent-providers";
import type { NativeChildEvidence, NativeChildReader } from "./harness";

/** Provider evidence only: a tool finishing is not evidence that its child finished. */
export class NativeExecutions {
  readonly children = new Map<string, ExecutionRecord>();
  // How this run's own program is read. Which words in a stream mean a child was asked for, started
  // or ended is the program's to say, so the reader comes from the harness carrying the run; a
  // program that opens no children of its own, and a provider this build does not carry, give none
  // and nothing is read at all.
  private readonly reader: NativeChildReader | null;
  constructor(private archive: Archive, private parent: ExecutionRecord) {
    const road = providerImplementations().find((one) => one.name === parent.provider);
    this.reader = road?.adapter.nativeChildReader?.() ?? null;
  }
  private async update({ id, state, evidence, session }: NativeChildEvidence) {
    const child = this.children.get(id) ?? nativeExecution(this.parent, id);
    if (child.endedAt !== null) return;
    if (state === "requested" && child.nativeState && child.nativeState !== "requested") return;
    child.nativeState = state; child.nativeEvidence = evidence;
    child.state = state === "requested" ? "launching" : state === "started" ? "running"
      : state === "completed" || state === "failed" ? "exited" : "unknown";
    if (state === "completed" || state === "failed") {
      child.endedAt = Date.now(); child.exitCode = state === "completed" ? 0 : 1;
    }
    if (session === true) child.sessionId = id;
    this.children.set(id, child); await this.archive.executions().saveExecution(child);
  }
  async observe(event: unknown) {
    if (this.reader === null || !event || typeof event !== "object") return;
    // Read one piece at a time: a piece of evidence from later in the same event is entitled to see
    // the child an earlier piece of it established.
    for (const one of this.reader.read(event, (id) => this.children.has(id))) {
      await this.update(one);
    }
  }
  async finish() {
    for (const [id, child] of this.children) {
      if (child.endedAt === null) await this.update({ id, state: "unresolved", evidence: "collector ended without terminal evidence" });
    }
  }
}
