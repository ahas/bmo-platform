<template>
  <!-- A video a message carried, played where it stands with the browser's own controls, and one
       at a time across the screen with the sounds. Only its length and size are fetched before it
       is pressed. Its address asks for a moment just past the start, so the first frame stands as
       its preview on a phone too; where Telegram said what shape it is, the player stands in that
       shape from the first frame, no taller than 20rem and no wider than the bubble, so nothing
       under it moves when the file answers. -->
  <video
    data-player
    :src="`${url}#t=0.1`"
    controls
    preload="metadata"
    playsinline
    :aria-label="props.attachment.name"
    :style="box"
    class="block max-w-full rounded-lg border border-default bg-black"
    :class="box === undefined ? 'max-h-80 w-auto' : 'h-auto'"
    @click.stop
    @play="alone"
  />
</template>

<script setup lang="ts">
import { mediaUrl, playAlone } from "../domain/player";
import type { MessageAttachment } from "#shared/types";

const props = defineProps<{ attachment: MessageAttachment; chatId: number; messageId: number }>();

const url = computed(() => `${mediaUrl(props.attachment.kind, props.chatId, props.messageId)}?item=${props.attachment.id}`);

const box = computed(() => {
  const shape = props.attachment.shape;

  return shape === undefined
    ? undefined
    : { aspectRatio: `${shape.width} / ${shape.height}`, width: `min(100%, ${((20 * shape.width) / shape.height).toFixed(3)}rem)` };
});

function alone(event: Event): void {
  // Only the message players: a moving sticker is a picture that loops, never a sound to stop.
  playAlone(event.target as HTMLMediaElement, document.querySelectorAll<HTMLMediaElement>("[data-player]"));
}
</script>
