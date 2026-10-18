<template>
  <!-- What a turn has done, oldest at the top, each line the badge its sort wears and what it was
       about. It is the room's own reading of the very lines the agent's log keeps, so a turn read
       here and the same turn read on the agent's page say one thing. -->
  <ul class="flex min-w-0 flex-col gap-1">
    <!-- A turn that has done more than the block stands says so once, above what it stands. -->
    <li v-if="props.dropped > 0" class="text-sm text-dimmed">{{ $t("room.label.earlier_count", { n: countText(props.dropped) }) }}</li>

    <li v-for="(action, index) in shown" :key="index" class="flex min-w-0 flex-wrap items-baseline gap-2">
      <!-- A reference document says, left of its sort, whether it was read on this turn or was
           already in the session. -->
      <ContextStateMark v-if="action.mark !== null" :look="action.mark" class="self-center" />
      <!-- A call wears its tool's own name as its chip, with the icon the tool declares, as the
           agent's log shows it; what was handed over wears its sort's word. -->
      <UBadge v-if="action.chip !== null" :color="action.look.color" variant="subtle" size="sm" :icon="action.chip.icon" :label="action.chip.label" class="font-mono" />
      <UBadge v-else :color="action.look.color" variant="subtle" size="sm" :label="action.look.label" />
      <!-- A path and a command carry runs with nowhere to break — a hundred characters of
           directories and dashes — and on a phone such a run breaks inside itself or it takes the
           whole page sideways. Measured at 360 CSS pixels: breaking only between words leaves this
           list wider than the screen and the room scrolling to the side; breaking anywhere keeps
           every line inside it. -->
      <span class="min-w-0 font-mono text-sm break-all text-muted">{{ action.text }}</span>
      <UBadge v-if="action.failed" color="warning" variant="subtle" size="sm" :label="FAILED" />
    </li>
  </ul>
</template>

<script setup lang="ts">
import { countText } from "~/domain/numbers";
import { actionLook, useToolIcons } from "~/domain/working";

import type { TurnAction } from "#shared/types";

interface Props {
  actions: TurnAction[];
  // How many actions stand above what is shown, which the turn made before these.
  dropped?: number;
}

const { t } = useI18n();
const props = withDefaults(defineProps<Props>(), { dropped: 0 });

// What a call that came back a failure wears. A call still out and one that came back well both
// stand plainly: the room is watching the turn, not auditing it.
const FAILED = t("room.status.failed");

// A call read off the bot's live picture carries no icon of its own, so the one its tool declares is read here.
const icons = useToolIcons();
const shown = computed(() => props.actions.map((action) => actionLook(t, action, icons.value)));
</script>
