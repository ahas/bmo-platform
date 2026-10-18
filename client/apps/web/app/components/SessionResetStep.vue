<template>
  <!-- The step a session's row grows before its reset is asked (the agency's docs/session-resets.md): one line of
       what the reset clears, the way out and the way through at the trailing edge, and the bot's
       refusal under them where it refused. -->
  <div class="flex flex-col gap-2" role="group" :aria-label="$t('agents.action.session_reset')">
    <div class="flex flex-wrap items-center gap-2">
      <p class="min-w-0 flex-1 text-sm text-toned">{{ sessionClears($t, props.session) }}</p>
      <div class="ms-auto flex shrink-0 items-center gap-2">
        <UButton size="sm" color="neutral" variant="ghost" :label="$t('common.action.cancel')" :disabled="busy" @click="emit('cancel')" />
        <UButton size="sm" color="warning" :label="$t('agents.action.session_reset_short')" :loading="busy" :disabled="busy" @click="void reset()" />
      </div>
    </div>
    <p v-if="failed !== ''" role="alert" class="break-words text-sm text-error">{{ failed }}</p>
  </div>
</template>

<script setup lang="ts">
import { failureText } from "bmo-ui/app/domain/errors";
import { sessionClears } from "~/domain/sessions";

import type { SessionView } from "#shared/agency";

const props = defineProps<{
  session: Pick<SessionView, "agent" | "chatId" | "kind" | "running" | "models" | "title" | "generation">;
}>();
const emit = defineEmits<{ done: [view: SessionView]; cancel: [] }>();
const { t } = useI18n();

const busy = ref(false);
const failed = ref("");

async function reset(): Promise<void> {
  busy.value = true;
  failed.value = "";

  try {
    emit("done", await $fetch<SessionView>("/api/session-reset", { method: "POST", body: { agent: props.session.agent, chatId: props.session.chatId } }));
  } catch (error) {
    failed.value = failureText(error, t("common.error.save_failed"));
  } finally {
    busy.value = false;
  }
}
</script>
