import { textLanguage, translationCut, translationFailureLine, translationLanguage } from "#shared/translation";

import { useLiveRefresh } from "./events";
import { failureText } from "bmo-ui/app/domain/errors";

import type { DocumentLanguage, WorkTextTranslation } from "#shared/types";
import type { Translate } from "#shared/wording";
import type { Ref } from "vue";

// One text of a work order read in the other language (`docs/work-orders.md`, 「The language of the
// prompts」). A co-worker writes a plan and a step result in English; the screen puts the text through
// the same translator a document goes through and draws what has come back so far, each part landing
// the moment it is written.

// Which of a task's two texts is being read, and the text itself.
export interface WorkTextSource {
  todoId: number;
  task: string;
  field: "instruction" | "result";
  text: string;
}

export interface WorkTextTranslated {
  // The text to draw: the translation as far as it has come, with every part still waiting standing
  // in its original, or the original itself while the toggle is off.
  shown: Ref<string>;
  // The language the text would be put into, known before the bot has answered anything.
  target: Ref<DocumentLanguage | null>;
  // Whether parts are still on their way, how many the translator could not render, and whether an
  // order is being handed over.
  translating: Ref<boolean>;
  failed: Ref<number>;
  asking: Ref<boolean>;
  // What went wrong, in one line: what the console could not do, or what the bot said of the
  // translator and the road it was asked on.
  failure: Ref<string>;
  retry: () => Promise<void>;
}

export function useWorkTextTranslation(t: Translate, source: () => WorkTextSource | null, on: () => boolean): WorkTextTranslated {
  const translation = ref<WorkTextTranslation | null>(null);
  const failure = ref("");
  const asking = ref(false);
  let reading = 0;

  const target = computed(() => {
    const held = source();

    return held === null ? null : translationLanguage(textLanguage(held.text));
  });

  // Read what stands, and where parts are missing while the bot holds none of them and reports no
  // failure, hand them over: that is a first request, a translation a restart cut off, or a text
  // rewritten since, and each carries on from the parts already kept.
  async function load(order: boolean): Promise<void> {
    const held = source();
    const mine = (reading += 1);

    if (held === null) {
      return;
    }

    try {
      const read = await $fetch<WorkTextTranslation | null>(`/api/todos/${held.todoId}/work-order/translation`, {
        query: { task: held.task, field: held.field }
      });

      if (mine !== reading) {
        return;
      }

      translation.value = read;

      const missing = read !== null && read.parts.some((part) => part.text === null && !part.failed);

      // A translation already kept reads without the bot; only what is still missing needs it.
      failure.value =
        read !== null && read.pending === null && missing ? t("todo.error.translation_unreachable") : translationFailureLine(read?.failure);

      if (order && missing && read.pending === 0 && read.failure === null && !asking.value) {
        await ask(false);
      }
    } catch (error) {
      if (mine === reading) {
        failure.value = failureText(error, t("todo.error.translation_read_failed"));
      }
    }
  }

  async function ask(again: boolean): Promise<void> {
    const held = source();

    if (held === null) {
      return;
    }

    asking.value = true;
    failure.value = "";

    try {
      await $fetch(`/api/todos/${held.todoId}/work-order/translation`, {
        method: "POST",
        body: { task: held.task, field: held.field, retry: again }
      });
      await load(false);
    } catch (error) {
      failure.value = failureText(error, t("todo.error.translation_ask_failed"));
    } finally {
      asking.value = false;
    }
  }

  // Another text, or the toggle turned on: what stood is not this text's, so it goes before anything
  // of the new one arrives.
  watch(
    [() => on(), () => source()?.todoId, () => source()?.task, () => source()?.field, () => source()?.text],
    () => {
      reading += 1;
      translation.value = null;
      failure.value = "";

      if (on()) {
        void load(true);
      }
    },
    { immediate: true }
  );

  // A word that a part of this text arrived reads it again, and shows it at once.
  useLiveRefresh(
    () => {
      if (on()) {
        void load(true);
      }
    },
    {
      on: ["document-translation"],
      where: (event) =>
        event.type === "document-translation" && translation.value !== null && event.document === translation.value.document
    }
  );

  const shown = computed(() => {
    const held = source();

    if (held === null) {
      return "";
    }

    const read = translation.value;

    if (!on() || read === null) {
      return held.text;
    }

    const cut = translationCut(held.text);

    // A reading of another cut of the text says nothing about this one, so the original stands.
    return cut.parts.length === read.parts.length ? cut.join(read.parts.map((part) => part.text)) : held.text;
  });

  const translating = computed(() => (translation.value?.parts ?? []).some((part) => part.text === null && !part.failed));
  const failed = computed(() => (translation.value?.parts ?? []).filter((part) => part.failed).length);

  return { shown, target, translating, failed, asking, failure, retry: () => ask(true) };
}

const LANGUAGE_NAMES: Record<DocumentLanguage, string> = { ko: "documents.label.language_ko", en: "documents.label.language_en" };

// What a language is called on the switch that puts a text into it.
export function languageName(t: Translate, language: DocumentLanguage): string {
  return t(LANGUAGE_NAMES[language]);
}
