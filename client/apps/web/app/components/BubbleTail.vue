<template>
  <!-- The tail a run's last bubble hangs off its foot on the photo's side, in the bubble's own
       colour (docs/web.md, 「The run's last bubble carries the tail」). It fills the step between the
       bubble and the photo up to the height of the photo's centre, less the photo circle grown by
       that step, so its concave edge runs at one distance round the photo. It reaches a pixel into
       the bubble under the bubble's square corner, in the bubble's own ground, so the two are one
       shape at every pixel ratio rather than two shapes meeting along an edge the browser may draw a
       seam on. -->
  <span
    aria-hidden="true"
    class="bubble-tail pointer-events-none absolute bottom-0"
    :class="props.side === 'right' ? '-scale-x-100' : ''"
    :data-side="props.side"
  />
</template>

<script setup lang="ts">
import type { MessageSide } from "~/domain/room";

interface Props {
  side: MessageSide;
}

const props = defineProps<Props>();
</script>

<style scoped>
/* Every measure of the tail is read off the photo and its step from the bubble. The photo's foot
   stands level with the bubble's foot, so its centre is a photo's radius up from that foot, which is
   the tail's top edge, and a photo's radius out past the step, which is outside the tail's leading
   edge. The arc on that centre is the photo's radius and the step together; it meets the bubble's
   side at the top, running straight down it, and the half pixel either side of it is the arc's own
   smoothing. The tail is the bubble's own ground, the very colour painted the very way, and the arc is
   only cut out of it, so tail and bubble cannot differ by a shade where they meet. The right side's tail
   is the left one turned about. */
.bubble-tail {
  --tail-face-radius: calc(var(--chat-face) / 2);
  --tail-arc: radial-gradient(
    circle calc(var(--tail-face-radius) + var(--chat-face-gap)) at calc(var(--tail-face-radius) * -1) 0,
    transparent calc(100% - 0.5px),
    #000 calc(100% + 0.5px)
  );
  width: calc(var(--chat-face-gap) + 1px);
  height: var(--tail-face-radius);
  background-color: var(--bubble);
  -webkit-mask-image: var(--tail-arc);
  mask-image: var(--tail-arc);
}

.bubble-tail[data-side="left"] {
  inset-inline-start: calc(var(--chat-face-gap) * -1);
}

.bubble-tail[data-side="right"] {
  inset-inline-end: calc(var(--chat-face-gap) * -1);
}
</style>
