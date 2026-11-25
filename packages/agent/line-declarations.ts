
import type { RunLineDeclaration } from "./harness";

// The lines the deployment runs, each declared with what it asks of its program. What a line is,
// and how a program is judged to carry one, are the harness's; which lines there are is the
// deployment's own (`run-lines.ts` for agency's), handed over as a function and read the first time it
// is asked for. A read with nothing composed fails, naming what is missing.

export class RunLinesUncomposed extends Error {
  constructor() {
    super("no run lines are composed: a process reads its configuration before it asks what a line keeps, or imports the deployment's registrations itself");
  }
}

let composed: (() => readonly RunLineDeclaration[]) | null = null;

export function composeRunLines(lines: () => readonly RunLineDeclaration[]): void {
  composed = lines;
}

export function runLineDeclarations(): readonly RunLineDeclaration[] {
  if (composed === null) {
    throw new RunLinesUncomposed();
  }

  return composed();
}

// Whether a line's turns follow one another on one session. A line the deployment never declared
// keeps none.
export function lineKeepsSession(line: string): boolean {
  return runLineDeclarations().some((one) => one.name === line && one.keepsSession);
}

// What a line's runs may reach of the tool host. A line the deployment never declared, or a
// process that composed no lines, reaches nothing.
export function lineCapabilities(line: string): readonly string[] {
  return declared(line)?.capabilities ?? [];
}

// The tool host namespaces a line's runs are issued. A line the deployment never declared, or a
// process that composed no lines, is issued none.
export function lineNamespaces(line: string): readonly string[] {
  return declared(line)?.namespaces ?? [];
}

// The entries a line's runs are shown by full name. A line the deployment never declared, or a
// process that composed no lines, is shown none by name.
export function lineShown(line: string): readonly string[] {
  return declared(line)?.shown ?? [];
}

function declared(line: string): RunLineDeclaration | undefined {
  try {
    return runLineDeclarations().find((one) => one.name === line);
  } catch (error) {
    if (error instanceof RunLinesUncomposed) return undefined;
    throw error;
  }
}

// Whether a line's standing program hands a person's typed line the composition the deployment's
// own turns carry. A line the deployment never declared hands it none.
export function lineComposesTypedLines(line: string): boolean {
  return runLineDeclarations().some((one) => one.name === line && one.composesTypedLines === true);
}

// Whether a line's place holds a bot's turn's end on its background work, a tool call written as text or a
// banned word. A line the deployment never declared holds none.
export function lineHoldsStops(line: string): boolean {
  return runLineDeclarations().some((one) => one.name === line && one.holdsStops === true);
}
