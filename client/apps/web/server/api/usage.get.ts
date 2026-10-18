import type { UsageView } from "#shared/types";

// What the accounts are using and what each session is carrying: the usage windows, the tallies by model, by
// agent and by user, and the sessions' context. No unit serves them yet, so the view stands empty and every
// meter reads as having nothing to say (docs/boundary.md, 「Gaps the console meets」).
export default defineEventHandler(async (event): Promise<UsageView> => {
  await requireUserSession(event);

  return { windows: [], fiveHour: [], weekly: [], agents: [], users: [], sessions: [] };
});
