<template>
  <!-- How long the work took, under the account of it. The total stands on its own line and the
       bars under it, each with what it is a bar of. -->
  <div v-if="time !== null" class="mt-2 flex flex-col gap-2">
    <p class="text-sm tabular-nums text-muted">
      {{ time.total }}<template v-if="time.attempts !== null"> · {{ time.attempts }}</template>
    </p>

    <div v-for="bar in time.bars" :key="bar.title" class="flex flex-col gap-1">
      <!-- A bar's note is a caveat on how to read it, so it sits under the title as the quieter
           line rather than running on from it. -->
      <div>
        <p class="text-sm text-default">{{ bar.title }}</p>
        <p v-if="bar.note !== null" class="text-sm text-muted">{{ bar.note }}</p>
      </div>

      <!-- The legend reads the clock rather than the percent it would otherwise carry. One of
           these bars is a partition and the other is a sum of things that overlapped, so a
           percentage would be true on one and a lie on the other; the length of time is the
           figure that means the same on both, and is what was asked for. -->
      <UProgressGroup :items="itemsOf(bar)" :max="bar.max" :size="BAR_SIZE" :ui="STACKED_LEGEND">
        <template #item-trailing="{ item }">
          {{ item.clock }}<span v-if="item.detail !== null" class="text-muted"> · {{ item.detail }}</span>
        </template>
      </UProgressGroup>
    </div>
  </div>
</template>

<script setup lang="ts">
import { BAR_SIZE, STACKED_LEGEND } from "bmo-ui/app/domain/bars";
import { spanClock } from "bmo-ui/app/domain/time";
import { todoTimeline } from "~/domain/todos";

import type { TodoItem } from "#shared/types";
import type { TimeBar } from "~/domain/todos";

interface Props {
  item: TodoItem;
}

const props = defineProps<Props>();
const { t } = useI18n();

const time = computed(() => todoTimeline(t, props.item));

// One bar's segments as the group takes them: the seconds are the width, and the clock and
// what follows it ride along for the legend to read.
function itemsOf(bar: TimeBar) {
  return bar.segments.map((one) => ({
    label: one.label,
    value: one.seconds,
    color: one.color,
    clock: spanClock(t, one.seconds),
    detail: one.detail
  }));
}
</script>
