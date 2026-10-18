<template>
  <span v-if="failed || !attachment.available">{{ fallback }}</span>
  <video v-else-if="attachment.type === 'video/webm'" :src="url" muted loop autoplay playsinline
    :aria-label="fallback" :class="size" @error="failed = true" />
  <img v-else :src="url" :alt="fallback" :class="size" loading="lazy" @error="failed = true" />
</template>
<script setup lang="ts">
import { mediaUrl } from "../domain/player";
import type { MessageAttachment } from "#shared/types";
const props = defineProps<{ attachment: MessageAttachment; chatId: number; messageId: number; fallback: string; inline?: boolean }>();
const failed = ref(false);
const url = computed(() => `${mediaUrl('sticker', props.chatId, props.messageId)}?item=${props.attachment.id}`);
const size = computed(() => props.inline ? 'inline-block size-[1.3em] align-text-bottom object-contain' : 'block max-h-40 max-w-40 object-contain');
watch(url, () => { failed.value = false; });
</script>
