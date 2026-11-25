<template>
  <!-- The side part of the document viewer: the question box over the open document's contents. -->
  <div class="flex min-h-0 flex-1 flex-col">
    <div class="flex shrink-0 flex-col gap-3 border-b border-default p-3">
      <DocumentSeek :document="props.selected" :heading-text="props.headingText" @go="emit('go', $event)" />
    </div>
    <p v-if="!props.opened" class="p-3 text-sm text-muted">{{ $t("documents.status.no_toc") }}</p>
    <DocumentToc v-else class="min-h-0 flex-1 px-1 py-2" :headings="props.headings" :active="props.active" @select="emit('select', $event)" />
  </div>
</template>

<script setup lang="ts">
import type { ViewerHeading } from "~/domain/documents";
import type { DocumentSeekPick } from "#shared/types";

interface Props {
  selected: { root: string; path: string } | null;
  opened: boolean;
  headings: readonly ViewerHeading[];
  active: string | null;
  headingText: (pick: DocumentSeekPick) => string | null;
}

const props = defineProps<Props>();
const emit = defineEmits<{ go: [pick: DocumentSeekPick]; select: [slug: string] }>();
</script>
