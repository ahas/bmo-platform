import type { DbTable } from "#shared/types";

// Every table of the console's database with how many rows it holds. It is the whole of what the DB screen
// opens on, and like everything else here it stands behind the session rule.
export default defineEventHandler(async (event): Promise<DbTable[]> => {
  await requireUserSession(event);

  try {
    return await listDbTables();
  } catch (error) {
    throw fail(dbStatus(error), dbReason(error));
  }
});
