import { failureText } from "bmo-ui/app/domain/errors";

import type { ResearchSettingsView } from "#shared/research";
import type { Translate } from "#shared/wording";

// The pool, the research profiles and the engines, read once for the whole screen. What was last
// read stays standing when a reread fails: the screen calls it stale and stops saving over it,
// rather than blanking values the owner was reading (docs/web.md).
export function useResearchSettings(t: Translate) {
  const settings = useState<ResearchSettingsView | null>("research-settings", () => null);
  const readFailed = useState<string>("research-settings-failed", () => "");
  const reading = useState<boolean>("research-settings-reading", () => false);
  const readAt = useState<number | null>("research-settings-read-at", () => null);

  async function reload(): Promise<void> {
    if (reading.value) return;
    reading.value = true;

    try {
      settings.value = await $fetch<ResearchSettingsView>("/api/research-settings");
      readAt.value = Date.now();
      readFailed.value = "";
    } catch (error) {
      readFailed.value = failureText(error, t("research.error.settings_read"));
    } finally {
      reading.value = false;
    }
  }

  // One change asked of the bot, then read back. The bot's refusal comes back as its own sentence;
  // null is a change the bot took.
  async function ask(body: Record<string, unknown>): Promise<string | null> {
    try {
      await $fetch("/api/research-settings", { method: "POST", body });
    } catch (error) {
      return failureText(error, t("common.error.save_failed"));
    }

    await reload();

    return null;
  }

  return { settings, readFailed, reading, readAt, reload, ask };
}

// How often the pool's counts are read again while a screen that shows them stands.
const POLL_MS = 5_000;

// The same read, kept fresh while the calling component stands, with the clock its stale mark
// is judged by.
export function usePolledResearchSettings(t: Translate) {
  const held = useResearchSettings(t);
  const now = ref(Date.now());
  let timer: ReturnType<typeof setInterval> | null = null;

  onMounted(() => {
    void held.reload();
    timer = setInterval(() => {
      now.value = Date.now();

      if (document.visibilityState === "visible") {
        void held.reload();
      }
    }, POLL_MS);
  });

  onBeforeUnmount(() => {
    if (timer !== null) clearInterval(timer);
  });

  return { ...held, now };
}
