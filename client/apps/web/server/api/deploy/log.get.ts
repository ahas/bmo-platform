import type { DeployLogLine } from "#shared/types";

// The last lines of the deployment's own log, its unit's journal, for the title bar's popover. A
// record that names no unit has no journal to read.
const LINES = 80;

export default defineEventHandler(async (event): Promise<DeployLogLine[]> => {
  await requireAdministrator(event);

  const unit = readDeployReading().record?.unit ?? null;

  return unit === null ? [] : deployJournal(unit, LINES);
});
