import { readPasted } from "./pasted";

import type { PastedText } from "./pasted";
import type { Ref } from "vue";

// Where one page's draft is kept, under the same prefix the browser's other choices use.
const DRAFT_PREFIX = "bmo-web.draft.";

// The three prompts a person writes into: the room's, the gate test's, and the one that
// launches a runner or the worker. Credentials are not drafts and are kept nowhere.
export type DraftKind = "room" | "gate" | "launch";

// A draft belongs to the page it was typed on — the room to its chat number, the gate to its
// stage, the launch to the agent it speaks to — so no two prompts read each other's words.
export function draftKey(kind: DraftKind, id: string | number): string {
  return `${DRAFT_PREFIX}${kind}.${id}`;
}

// What a stored draft reads back as. A key the browser holds nothing under is no draft.
export function draftText(raw: string | null): string {
  return raw ?? "";
}

// What was typed into one prompt and not sent yet. It follows the text as it is written and
// is let go the moment the text goes out, so a send that failed keeps the words and a send
// that landed leaves nothing standing. The draft belongs to the browser it was typed in.
export function useDraft(kind: DraftKind, id: string | number): Ref<string> {
  const key = draftKey(kind, id);
  const draft = useState<string>(key, () => "");

  if (import.meta.client) {
    watch(draft, (next) => {
      writeDraft(key, next);
    });

    // The server has no browser to read and renders an empty prompt, so the draft is put back
    // once the page stands rather than while it is being rendered into place.
    onMounted(() => {
      draft.value = draftText(readDraft(key));
    });
  }

  return draft;
}

function readDraft(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeDraft(key: string, text: string): void {
  try {
    // An emptied prompt is no draft, and the sent one is emptied, so nothing is left behind.
    if (text === "") {
      window.localStorage.removeItem(key);

      return;
    }

    window.localStorage.setItem(key, text);
  } catch {
    // A browser that refuses storage still keeps what was typed for this visit.
  }
}

// The long pastes beside a draft, kept with it: the draft holds their marks, and a mark whose words
// were lost with the page would go out as the mark alone.
export function useDraftPastes(kind: DraftKind, id: string | number): Ref<PastedText[]> {
  const key = `${draftKey(kind, id)}.pasted`;
  const pasted = useState<PastedText[]>(key, () => []);

  if (import.meta.client) {
    watch(pasted, (next) => {
      writeDraft(key, next.length === 0 ? "" : JSON.stringify(next));
    });

    onMounted(() => {
      try {
        const raw = readDraft(key);

        pasted.value = readPasted(raw === null ? null : JSON.parse(raw));
      } catch {
        pasted.value = [];
      }
    });
  }

  return pasted;
}
