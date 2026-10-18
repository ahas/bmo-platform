<template>
  <!-- What one turn ran and spent: the turn itself, then each gate its messages and its answer
       went through. The stage and the road it took lead a line with how long it took; under it the
       four kinds stand as one track with each kind named beside its figure, and a run never
       measured stands four dashes. -->
  <ul class="flex flex-col gap-2 text-sm" :class="props.boxed ? 'rounded-lg border border-default px-3 py-2' : ''">
    <li v-for="line in props.lines" :key="line.key" class="flex flex-col gap-1">
      <span class="flex min-w-0 flex-wrap items-baseline gap-x-2">
        <span class="text-highlighted">{{ line.stage }}</span>
        <span class="font-mono break-all text-muted">{{ line.road }}</span>
        <span v-if="line.outcome !== null" class="text-warning">{{ line.outcome }}</span>
        <span v-if="line.took !== ''" class="ms-auto whitespace-nowrap text-muted tabular-nums">{{ line.took }}</span>
      </span>

      <UsageSplit :tally="line.tally" />
    </li>
  </ul>
</template>

<script setup lang="ts">
import type { TurnLine } from "~/domain/turns";

interface Props {
  lines: TurnLine[];
  // Whether the figures stand in a box of their own: not inside a bubble, which is already one.
  boxed?: boolean;
}

const props = withDefaults(defineProps<Props>(), { boxed: true });
</script>
