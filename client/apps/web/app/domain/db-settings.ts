import { isDbMode } from "./dbview";

import type { DbMode } from "./dbview";
import type { Ref } from "vue";

const DB_KEY = "bmo-web.db-view";

let restoredDb = false;

// Which table the DB screen was left on and whether it was walking a table or asking a question
// of it. Kept the way every other choice on this console is kept, so the screen opens where it
// was left; the page, the order and the search are not kept, since those belong to one reading
// rather than to the screen.
export interface DbViewSettings {
  table: Ref<string | null>;
  mode: Ref<DbMode>;
  setTable: (next: string | null) => void;
  setMode: (next: DbMode) => void;
}

export function useDbView(): DbViewSettings {
  const table = useState<string | null>("bmo-db-table", () => null);
  const mode = useState<DbMode>("bmo-db-mode", () => "table");

  if (import.meta.client && !restoredDb) {
    restoredDb = true;

    const held = readDbView();

    table.value = held.table;
    mode.value = held.mode;
  }

  function store(): void {
    if (!import.meta.client) {
      return;
    }

    try {
      window.localStorage.setItem(DB_KEY, JSON.stringify({ table: table.value, mode: mode.value }));
    } catch {
      // A browser that refuses storage still shows what was picked for this visit.
    }
  }

  function setTable(next: string | null): void {
    table.value = next;
    store();
  }

  function setMode(next: DbMode): void {
    mode.value = next;
    store();
  }

  return { table, mode, setTable, setMode };
}

function readDbView(): { table: string | null; mode: DbMode } {
  try {
    const raw = window.localStorage.getItem(DB_KEY);
    const parsed: unknown = raw === null ? null : JSON.parse(raw);
    const held = parsed as { table?: unknown; mode?: unknown } | null;

    return {
      table: typeof held?.table === "string" ? held.table : null,
      mode: isDbMode(held?.mode) ? held.mode : "table"
    };
  } catch {
    return { table: null, mode: "table" };
  }
}
