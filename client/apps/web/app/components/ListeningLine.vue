<template>
  <!-- The chat agent's row while it only listens (docs/web.md, 「The listening row」): three plain lines under
       the row's name, still, in the room's muted words — that the room's chat is on, that it
       listens, and the window's time left turning down by the second. None of them wraps, so the row
       keeps its height. -->
  <p class="min-w-0 text-sm leading-(--chat-meta-line) text-muted whitespace-nowrap" data-listening-enabled>{{ lines.enabled }}</p>
  <p class="min-w-0 text-sm leading-(--chat-meta-line) text-muted whitespace-nowrap" data-listening>{{ lines.state }}</p>
  <RollingSentence tag="p" class="min-w-0 text-sm leading-(--chat-meta-line) text-muted whitespace-nowrap" data-listening-left :keypath="lines.left.keypath" :figures="lines.left.figures" />
</template>

<script setup lang="ts">
import { useNowSeconds } from "bmo-ui/app/domain/time";
import { listeningLines } from "~/domain/working";

interface Props {
  name: string;
  // When the window closes, in unix seconds on this screen's clock.
  closesAt: number;
}

const props = defineProps<Props>();

const { t } = useI18n();
const { now, sync } = useNowSeconds(1000);
// A window moved on is counted from the moment it was read, not from the last tick before it.
watch(() => props.closesAt, sync);
const lines = computed(() => listeningLines(t, props.name, props.closesAt - now.value));
</script>
