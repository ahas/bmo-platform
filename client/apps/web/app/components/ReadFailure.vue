<template>
  <!-- A read that failed: its reason in the error band, and the refresh that reads again as an
       icon at the band's trailing end, inside it, so the band and its way out are one line. -->
  <UAlert
    v-if="props.failure.shown.value"
    role="alert"
    class="shrink-0"
    color="error"
    variant="subtle"
    orientation="horizontal"
    :description="props.message"
  >
    <template #actions>
      <UTooltip :text="$t('common.action.refresh')">
        <UButton
          icon="i-material-symbols-refresh"
          color="error"
          variant="ghost"
          size="sm"
          :loading="props.reading"
          :aria-label="$t('common.action.refresh')"
          @click="emit('retry')"
        />
      </UTooltip>
    </template>
  </UAlert>

  <!-- A read that failed only because the bot is restarting: what was read stays, and this calm
       line says the screen is waiting for the bot rather than that something broke. -->
  <p v-else-if="props.failure.reconnecting.value && !props.failure.saidAround.value" role="status" class="flex shrink-0 items-center gap-2 text-sm text-muted">
    <UIcon name="i-material-symbols-progress-activity" class="size-4 shrink-0 animate-spin" />
    {{ $t("common.status.reconnecting") }}
  </p>
</template>

<script setup lang="ts">
import type { ReadFailure } from "~/domain/events";

// The screen reads its failure with useReadFailure where the screen itself stands, so a read the
// bot's return brings is made whether or not this band is on the screen at that moment.
const props = defineProps<{
  failure: ReadFailure;
  // What the band says: the reason the read failed.
  message: string;
  // Whether a read is on its way now.
  reading: boolean;
}>();

const emit = defineEmits<{ retry: [] }>();
</script>
