<template>
  <!-- What a memory waits on: the event it waits on, marked as one, with when it came; for a state,
       when it last moved; otherwise its condition. It stands on one line cut to its cell, or wraps
       where the place it stands in lets it. A row with none of them draws nothing, and the place it
       stands in is left out or blank. -->
  <span v-if="memory.event !== null" class="flex min-w-0 items-center gap-1" :class="{ 'flex-wrap': wrap }">
    <UBadge color="neutral" variant="subtle" size="sm" :label="$t('memories.label.event')" />
    <span class="text-highlighted" :class="{ truncate: !wrap }">{{ eventText(memory.event) }}</span>
    <span v-if="memory.eventMetAt !== null" class="shrink-0 tabular-nums text-muted">
      {{ $t("memories.label.event_came", { at: formatStamp(memory.eventMetAt) }) }}
    </span>
  </span>
  <span v-else-if="memory.type === 'state'" class="flex min-w-0 items-center gap-1">
    <UBadge color="neutral" variant="subtle" size="sm" :label="$t('memories.label.state')" />
    <span v-if="memory.movedAt !== null" class="shrink-0 tabular-nums text-muted">
      {{ $t("memories.label.state_moved", { at: formatStamp(memory.movedAt) }) }}
    </span>
  </span>
  <span v-else-if="memory.condition !== ''" class="text-highlighted" :class="wrap ? 'break-words' : 'truncate'" :title="memory.condition">{{ memory.condition }}</span>
</template>

<script setup lang="ts">
import { eventText } from "~/domain/memories";
import { formatStamp } from "bmo-ui/app/domain/time";

import type { MemoryRowView } from "#shared/types";

// Whether it wraps rather than being cut to one line.
defineProps<{ memory: MemoryRowView; wrap?: boolean }>();
</script>
