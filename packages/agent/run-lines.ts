import { carriesLine } from "./harness";

import declaredRunLines from "@agency/policy/run-lines.toml";

import type { AgentAdapter, RunCapability, RunLineDeclaration } from "./harness";

// The lines agency runs, named for what the run is for rather than for the program that carries it: a
// room's turn of any conversational agent, a judgement, a job, a search, a browse. What a line is, and how a program is
// judged to carry one, are the agent base's (`harness.ts`).
export const runLines = ["chat", "gate", "worker", "search", "browse"] as const;

export type RunLine = (typeof runLines)[number];

// What each line asks of its program, as the policy declares it (docs/policy.md), so
// every side that composes a run of a line reads the same declaration.
interface DeclaredLine {
  needs: RunCapability[];
  keeps_session: boolean;
  composes_typed_lines?: boolean;
  holds_stops?: boolean;
  capabilities?: string[];
  namespaces?: string[];
  shown?: string[];
}

const declaredLines = declaredRunLines as Record<RunLine, DeclaredLine>;

function declarationOf(line: RunLine): RunLineDeclaration {
  const declared = declaredLines[line];

  return {
    name: line,
    needs: declared.needs,
    keepsSession: declared.keeps_session,
    ...(declared.composes_typed_lines === undefined ? {} : { composesTypedLines: declared.composes_typed_lines }),
    ...(declared.holds_stops === undefined ? {} : { holdsStops: declared.holds_stops }),
    ...(declared.capabilities === undefined ? {} : { capabilities: declared.capabilities }),
    ...(declared.namespaces === undefined ? {} : { namespaces: declared.namespaces }),
    ...(declared.shown === undefined ? {} : { shown: declared.shown }),
  };
}

export const runLineDeclarations: Readonly<Record<RunLine, RunLineDeclaration>> = runLines.reduce(
  (all, line) => ({ ...all, [line]: declarationOf(line) }),
  {} as Record<RunLine, RunLineDeclaration>,
);

// Every line, as the agent base is handed them (`line-declarations.ts`).
export function agencyRunLines(): readonly RunLineDeclaration[] {
  return runLines.map((line) => runLineDeclarations[line]);
}

// Whether a provider's program carries one of the lines.
export function carriesRunLine(adapter: Pick<AgentAdapter, "offers">, line: RunLine): boolean {
  return carriesLine(adapter, runLineDeclarations[line]);
}
