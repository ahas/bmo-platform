<template>
  <!-- What an item is, part by part. Each part of the instruction has a place of its own, so the
       two that say what the work is are never pushed out of sight by a long conversation: the
       conversation scrolls inside its own place instead. The names and the order are the
       instruction's own. -->
  <div class="flex min-w-0 flex-col gap-2">
    <template v-for="(part, index) in parts" :key="index">
      <div v-if="part.kind === 'block'" class="rounded-md border border-default bg-elevated/40 p-3">
        <p class="text-sm font-medium text-muted">{{ part.label }}</p>
        <p class="text-sm text-muted">{{ readableSource(part.source) }}</p>
        <MarkdownText :text="part.text" class="mt-1 text-sm text-highlighted" />
      </div>

      <div v-else-if="part.kind === 'conversation'" class="rounded-md border border-default p-3">
        <p class="text-sm font-medium text-muted">{{ part.title }}</p>

        <div class="mt-1">
          <div v-for="(entry, at) in part.entries" :key="at" class="mb-2 last:mb-0">
            <p class="text-sm">
              <span class="font-medium" :class="entry.reply ? 'text-muted' : 'text-primary'">{{ entry.label }}</span>
              <span class="text-muted"> · {{ readableSource(entry.source) }}</span>
            </p>
            <MarkdownText :text="entry.text" class="text-sm text-highlighted" />
          </div>
        </div>
      </div>

      <MarkdownText v-else :text="part.text" class="rounded-md border border-default bg-elevated/40 p-3 text-sm text-highlighted" />
    </template>

    <!-- The owner's own word on this item, kept apart from the instruction it rides with. -->
    <div v-if="item.note !== null" class="rounded-md border border-primary/30 bg-primary/5 p-3">
      <p class="text-sm font-medium text-primary">{{ $t("todo.label.owner_note") }}</p>
      <MarkdownText :text="item.note" class="mt-1 text-sm text-highlighted" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { instructionParts, readableSource } from "~/domain/todos";

import type { TodoItem } from "#shared/types";

interface Props {
  item: TodoItem;
}

const props = defineProps<Props>();

const { t } = useI18n();
const parts = computed(() => instructionParts(t, props.item.instruction));
</script>
