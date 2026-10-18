import { readGroupFolds } from "./agents";

import type { GroupFolds } from "./agents";
import type { Ref } from "vue";

// Which of the sidebar's agent groups the owner has folded away. Every group is remembered, not only the ones
// folded, so a group deliberately opened stays open on the next visit; a group never touched is not remembered
// at all and stands open. The choice belongs to the browser it was made in, kept under the key every other
// choice of this console's is kept beside.

const FOLDS_KEY = "bmo-web.agent-folds";

export interface AgentFoldSettings {
  folds: Ref<GroupFolds>;
  setFolds: (next: GroupFolds) => void;
}

let restoredFolds = false;

export function useAgentFolds(): AgentFoldSettings {
  const folds = useState<GroupFolds>("bmo-agent-folds", () => ({}));

  if (import.meta.client && !restoredFolds) {
    restoredFolds = true;
    folds.value = readFolds();
  }

  function setFolds(next: GroupFolds): void {
    folds.value = next;

    if (import.meta.client) {
      try {
        window.localStorage.setItem(FOLDS_KEY, JSON.stringify(next));
      } catch {
        // A browser that refuses storage still keeps the sidebar as it stands for this visit.
      }
    }
  }

  return { folds, setFolds };
}

function readFolds(): GroupFolds {
  try {
    const raw = window.localStorage.getItem(FOLDS_KEY);

    return readGroupFolds(raw === null ? null : JSON.parse(raw));
  } catch {
    return {};
  }
}
