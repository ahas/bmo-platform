import { getCurrentInstance, onMounted } from "vue";

import type { TurnAction } from "#shared/types";
import type { Ref } from "vue";

// Which of a running turn's two lists the reader last left folded away, one choice per sort, kept in the browser it
// was made in. It is where the next running turn starts, and nothing more: every row, and every chip on it, folds
// and spreads its own list on its own.

const TURN_ACTIONS_KEY = "bmo-web.turn-actions";

export type TurnActionsFolds = Record<TurnAction["kind"], boolean>;

export interface TurnActionsFold {
  folded: Ref<TurnActionsFolds>;
  setFolded: (kind: TurnAction["kind"], next: boolean) => void;
}

let restoredTurnActions = false;

// A choice kept in this browser is put back once the component asking for it has mounted, so the attributes the
// server drew are not left standing on the elements.
function afterMount(restore: () => void): void {
  if (getCurrentInstance() === null) {
    restore();
    return;
  }

  onMounted(restore);
}

export function useTurnActionsFold(): TurnActionsFold {
  const folded = useState<TurnActionsFolds>("bmo-turn-actions", () => ({ context: true, tool: true }));

  if (import.meta.client && !restoredTurnActions) {
    restoredTurnActions = true;
    afterMount(() => {
      folded.value = readTurnActionsFold();
    });
  }

  function setFolded(kind: TurnAction["kind"], next: boolean): void {
    folded.value = { ...folded.value, [kind]: next };

    if (import.meta.client) {
      try {
        window.localStorage.setItem(TURN_ACTIONS_KEY, JSON.stringify(folded.value));
      } catch {
        // A browser that refuses storage still keeps the fold as it stands for this visit.
      }
    }
  }

  return { folded, setFolded };
}

function readTurnActionsFold(): TurnActionsFolds {
  try {
    const raw = window.localStorage.getItem(TURN_ACTIONS_KEY);

    return readTurnActionsFolds(raw === null ? null : JSON.parse(raw));
  } catch {
    return { context: true, tool: true };
  }
}

// What was kept, read back one sort at a time: anything that is not a kept choice of that sort stands it folded.
export function readTurnActionsFolds(value: unknown): TurnActionsFolds {
  const kept = typeof value === "object" && value !== null ? (value as Partial<Record<string, unknown>>) : {};

  return { context: kept.context !== false, tool: kept.tool !== false };
}
