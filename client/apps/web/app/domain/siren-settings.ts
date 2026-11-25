import { SIREN_ALL } from "./siren";

import type { Ref } from "vue";

// Which agent, kind and model the cache siren's list is standing on, kept in the browser it was made in, the way
// every other choice on this console is kept.

const SIREN_KEY = "bmo-web.siren-filter";

export interface SirenFilters {
  agent: Ref<string>;
  kind: Ref<string>;
  model: Ref<string>;
  setAgent: (next: string) => void;
  setKind: (next: string) => void;
  setModel: (next: string) => void;
}

let restoredSiren = false;

// The cache siren's two pickers. Everything to begin with; a choice belongs to the browser it
// was made in, and a name it no longer finds in the list is still a name the picker offers.
export function useSirenFilters(): SirenFilters {
  const agent = useState<string>("bmo-siren-agent", () => SIREN_ALL);
  const kind = useState<string>("bmo-siren-kind", () => SIREN_ALL);
  const model = useState<string>("bmo-siren-model", () => SIREN_ALL);

  if (import.meta.client && !restoredSiren) {
    restoredSiren = true;

    const held = readSirenFilters();

    agent.value = held.agent;
    kind.value = held.kind;
    model.value = held.model;
  }

  function store(): void {
    if (!import.meta.client) {
      return;
    }

    try {
      window.localStorage.setItem(SIREN_KEY, JSON.stringify({ agent: agent.value, kind: kind.value, model: model.value }));
    } catch {
      // A browser that refuses storage still shows what was picked for this visit.
    }
  }

  function setAgent(next: string): void {
    agent.value = next;
    store();
  }

  function setKind(next: string): void {
    kind.value = next;
    store();
  }

  function setModel(next: string): void {
    model.value = next;
    store();
  }

  return { agent, kind, model, setAgent, setKind, setModel };
}

function readSirenFilters(): { agent: string; kind: string; model: string } {
  try {
    const raw = window.localStorage.getItem(SIREN_KEY);
    const parsed: unknown = raw === null ? null : JSON.parse(raw);
    const held = parsed as { agent?: unknown; kind?: unknown; model?: unknown } | null;

    return {
      agent: typeof held?.agent === "string" ? held.agent : SIREN_ALL,
      kind: typeof held?.kind === "string" ? held.kind : SIREN_ALL,
      model: typeof held?.model === "string" ? held.model : SIREN_ALL
    };
  } catch {
    return { agent: SIREN_ALL, kind: SIREN_ALL, model: SIREN_ALL };
  }
}
