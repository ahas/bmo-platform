<template>
  <!-- What a turn wrote down, in the order it wrote it: where each record landed as a badge — a note
       of the room, an item of its ledger, a new TODO row or words appended to one — and what it
       said. A record written over several lines keeps them. -->
  <ul class="flex min-w-0 flex-col gap-1">
    <li v-for="(record, index) in shown" :key="index" class="flex min-w-0 flex-wrap items-baseline gap-2">
      <UBadge :color="record.look.color" variant="subtle" size="sm" :label="record.look.label" />
      <span class="min-w-0 text-sm break-words whitespace-pre-wrap text-muted">{{ record.text }}</span>
    </li>
  </ul>
</template>

<script setup lang="ts">
import { recordLook } from "~/domain/chips";

import type { TurnRecord } from "#shared/types";

interface Props {
  records: TurnRecord[];
}

const props = defineProps<Props>();

const { t } = useI18n();
const shown = computed(() => props.records.map((record) => ({ look: recordLook(t, record), text: record.text })));
</script>
