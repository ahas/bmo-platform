<template>
  <!-- How full the session's context window stood once the turn that sent the answer ended: the
       mark and a whole percentage, named by its tooltip. It spreads nothing, so it is no button and
       answers a pointer with its tooltip alone, at the height and padding of the chips beside it. -->
  <HintTip
    :text="$t(CHIP_LABELS.context)"
    class="flex h-(--chat-meta-line) items-center gap-1 rounded-sm text-sm leading-(--chat-meta-line) tabular-nums"
    :class="STILL_CHIP_LOOK"
  >
    <UIcon :name="CHIP_ICONS.context" class="size-4 shrink-0" />
    <span class="sr-only">{{ $t(CHIP_LABELS.context) }}</span>
    <span>{{ contextChipText(props.ratio) }}</span>
  </HintTip>
</template>

<script setup lang="ts">
import { CHIP_ICONS, CHIP_LABELS, contextChipText, STILL_CHIP_LOOK } from "~/domain/chips";

interface Props {
  // The share of the window the session stood at, as a fraction.
  ratio: number;
}

const props = defineProps<Props>();
</script>
