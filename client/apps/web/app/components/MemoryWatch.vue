<template>
  <!-- The latest watch serving a memory, on one line: its number and name, how it stands, and how
       many more served the row before it. -->
  <span class="flex min-w-0 items-center gap-1">
    <span class="truncate text-highlighted" :title="name">{{ name }}</span>
    <UBadge :color="STATES[watch.state].color" variant="subtle" size="sm" :label="$t(STATES[watch.state].label)" class="shrink-0" />
    <span v-if="watch.others > 0" class="shrink-0 tabular-nums text-muted">
      {{ $t("memories.label.watch_others", { n: countText(watch.others) }) }}
    </span>
  </span>
</template>

<script setup lang="ts">
import { countText } from "~/domain/numbers";

import type { MemoryWatchView } from "#shared/types";

const props = defineProps<{ watch: MemoryWatchView }>();

const { t } = useI18n();

// How each state a watch ends in reads: whether it is still waiting, and how it ended where it has.
const STATES = {
  active: { label: "memories.label.watch_active", color: "info" },
  fired: { label: "memories.label.watch_fired", color: "success" },
  lapsed: { label: "memories.label.watch_lapsed", color: "warning" },
  broken: { label: "memories.label.watch_broken", color: "error" },
  cancelled: { label: "memories.label.watch_cancelled", color: "neutral" }
} as const satisfies Record<MemoryWatchView["state"], { label: string; color: string }>;

const name = computed(() => t("memories.label.watch_named", { id: props.watch.id, name: props.watch.name }));
</script>
