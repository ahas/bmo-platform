<template>
  <!-- One run's chip with what it spreads into, stacked: the chip on its own line, and the lines of
       the runs it adds up under it once pressed. -->
  <div class="flex flex-col items-start gap-1">
    <TokenChip :chip="props.chip" :open="open" @toggle="open = !open" />
    <AgentTurnFigures v-if="open" :lines="props.chip.lines" class="w-full max-w-2xl" />
  </div>
</template>

<script setup lang="ts">
import type { SpendChip } from "~/domain/turns";

interface Props {
  chip: SpendChip;
}

const props = defineProps<Props>();

const open = ref(false);
</script>
