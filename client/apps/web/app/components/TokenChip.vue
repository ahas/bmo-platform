<template>
  <!-- What a run, or a round of runs, wrote: one figure beside the token mark, 「~」 on it while
       the turn is still running. The four kinds of every run stand in the spread under it. -->
  <LineChip :icon="CHIP_ICONS.tokens" :label="$t(CHIP_LABELS.tokens)" :open="props.open" @toggle="emit('toggle')">
    <RollingNumber v-if="props.chip.figure !== null" :value="props.chip.figure" :text="props.chip.text" />
    <span v-else>{{ props.chip.text }}</span>
  </LineChip>
</template>

<script setup lang="ts">
import { CHIP_ICONS, CHIP_LABELS } from "~/domain/chips";

import type { SpendChip } from "~/domain/turns";

interface Props {
  chip: SpendChip;
  open: boolean;
}

const props = defineProps<Props>();

const emit = defineEmits<{ toggle: [] }>();
</script>
