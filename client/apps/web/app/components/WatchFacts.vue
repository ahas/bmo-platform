<template>
  <!-- Everything about a watch beyond its name, which is what its detail holds: where it stands,
       what it waits for, how its last check went, then the memory it serves or that it serves none,
       the whole of what it promised, the check and the moment. The two figure lines have no other
       place to be read, so they move where they do not fit. -->
  <p v-if="props.context !== null" class="text-muted">{{ props.context }}</p>
  <MarqueeLine class="tabular-nums text-muted">{{ watchWait($t, props.watch, props.now) }}</MarqueeLine>
  <MarqueeLine v-if="lastCheck !== null" class="tabular-nums text-muted">{{ lastCheck }}</MarqueeLine>

  <dl class="mt-1 flex flex-col gap-2">
    <div class="flex flex-col gap-1">
      <dt class="text-muted">{{ $t("live.label.watch_memory") }}</dt>
      <dd class="tabular-nums text-toned">
        {{ props.watch.memoryId === null ? $t("live.label.watch_memory_none") : $t("live.label.watch_memory_id", { id: props.watch.memoryId }) }}
      </dd>
    </div>
    <div class="flex flex-col gap-1">
      <dt class="text-muted">{{ $t("live.label.watch_content") }}</dt>
      <dd class="whitespace-pre-wrap break-words text-toned">{{ props.watch.note }}</dd>
    </div>
    <div v-if="props.watch.condition !== null" class="flex flex-col gap-1">
      <dt class="text-muted">{{ $t("live.label.watch_condition") }}</dt>
      <dd class="whitespace-pre-wrap break-all font-mono text-toned">{{ props.watch.condition }}</dd>
    </div>
    <div class="flex flex-col gap-1">
      <dt class="text-muted">{{ props.watch.kind === "timer" ? $t("live.label.watch_time") : $t("live.label.watch_deadline") }}</dt>
      <dd class="tabular-nums text-toned">{{ formatStamp(props.watch.dueAt ?? props.watch.deadlineAt) }}</dd>
    </div>
  </dl>
</template>

<script setup lang="ts">
import { formatStamp } from "bmo-ui/app/domain/time";
import { watchLastCheck, watchWait } from "~/domain/watches";

import type { WatchView } from "#shared/types";

const props = defineProps<{ watch: WatchView; now: number; context: string | null }>();

const { t } = useI18n();
const lastCheck = computed<string | null>(() => watchLastCheck(t, props.watch, props.now));
</script>
