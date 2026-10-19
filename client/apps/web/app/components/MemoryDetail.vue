<template>
  <!-- One memory, whole: everything its row on the table cuts to a column or leaves out. The same
       dialog in the two shapes this console uses everywhere: in the middle of a wide screen, up
       from the bottom edge of a narrow one. The short facts stand two to a line where the width
       allows, and every passage of words stands across the whole width under its label. A field
       the row lacks is left out rather than drawn blank. -->
  <component :is="narrow ? UDrawer : UModal" v-bind="shell" :open="props.open" @update:open="emit('update:open', $event)">
    <template #content>
      <div v-if="memory" class="flex h-full min-h-0 flex-col">
        <div class="flex items-center gap-2 border-b border-default px-4 py-3">
          <UBadge color="neutral" variant="subtle" size="sm" :label="memory.type" class="shrink-0" />
          <p class="min-w-0 flex-1 truncate text-sm font-medium text-highlighted">{{ memory.slug }}</p>

          <UTooltip :text="$t('common.action.close')">
            <UButton
              size="sm"
              color="neutral"
              variant="ghost"
              icon="i-material-symbols-close"
              :aria-label="$t('common.action.close')"
              @click="emit('update:open', false)"
            />
          </UTooltip>
        </div>

        <div class="min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 py-3">
          <dl class="flex flex-col gap-4 text-sm">
            <div class="flex flex-col gap-1">
              <dt class="text-muted">{{ $t("memories.label.action") }}</dt>
              <dd class="break-words whitespace-pre-wrap text-highlighted">{{ memory.action }}</dd>
            </div>

            <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <div class="flex flex-col gap-1">
                <dt class="text-muted">{{ $t("memories.label.agent") }}</dt>
                <dd>{{ memory.agentLabel }}</dd>
              </div>

              <div class="flex flex-col gap-1">
                <dt class="text-muted">{{ $t("memories.label.kind") }}</dt>
                <dd>{{ memory.type }}</dd>
              </div>

              <div class="flex flex-col gap-1">
                <dt class="text-muted">{{ $t("memories.label.whom") }}</dt>
                <dd class="break-words">{{ whomOf($t, memory) }}</dd>
              </div>

              <div v-if="memory.setByName !== null" class="flex flex-col gap-1">
                <dt class="text-muted">{{ $t("memories.label.set_by") }}</dt>
                <dd class="break-words">{{ memory.setByName }}</dd>
              </div>

              <div class="flex flex-col gap-1">
                <dt class="text-muted">{{ $t("memories.label.created_at") }}</dt>
                <dd class="tabular-nums">{{ formatStamp(memory.setAt) }}</dd>
              </div>

              <div v-if="memory.doneAt !== null" class="flex flex-col gap-1">
                <dt class="text-muted">{{ $t("memories.label.ended_at") }}</dt>
                <dd class="tabular-nums">{{ formatStamp(memory.doneAt) }}</dd>
              </div>

              <div v-if="memory.event !== null" class="flex flex-col gap-1">
                <dt class="text-muted">{{ $t("memories.label.event") }}</dt>
                <dd class="flex flex-wrap items-center gap-x-2">
                  <span>{{ eventText(memory.event) }}</span>
                  <span v-if="memory.eventMetAt !== null" class="tabular-nums text-muted">{{ $t("memories.label.event_came", { at: formatStamp(memory.eventMetAt) }) }}</span>
                </dd>
              </div>

              <div v-if="memory.type === 'state' && memory.movedAt !== null" class="flex flex-col gap-1">
                <dt class="text-muted">{{ $t("memories.label.state") }}</dt>
                <dd class="tabular-nums">{{ $t("memories.label.state_moved", { at: formatStamp(memory.movedAt) }) }}</dd>
              </div>

              <div v-if="memory.watch !== null" class="flex flex-col gap-1">
                <dt class="text-muted">{{ $t("memories.label.watch") }}</dt>
                <dd class="flex min-w-0"><MemoryWatch :watch="memory.watch" /></dd>
              </div>

              <div v-if="memory.condition !== ''" class="flex flex-col gap-1">
                <dt class="text-muted">{{ $t("memories.label.hits") }}</dt>
                <dd class="tabular-nums">
                  {{ $t("memories.label.hit_count", { n: countText(memory.hits) }) }}<template v-if="memory.lastHitAt !== null"> · <span class="text-muted">{{ formatStamp(memory.lastHitAt) }}</span></template>
                </dd>
              </div>

              <div v-if="memory.todoId !== null" class="flex flex-col gap-1">
                <dt class="text-muted">{{ $t("memories.label.todo") }}</dt>
                <ItemTip :id="memory.todoId" as="dd" class="self-start tabular-nums">{{ $t("memories.label.todo_named", { id: memory.todoId }) }}</ItemTip>
              </div>
            </div>

            <div v-if="memory.condition !== ''" class="flex flex-col gap-1">
              <dt class="text-muted">{{ $t("memories.label.condition") }}</dt>
              <dd class="break-words whitespace-pre-wrap">{{ memory.condition }}</dd>
            </div>

            <div v-if="memory.doneText !== null" class="flex flex-col gap-1">
              <dt class="text-muted">{{ $t("memories.label.done_text") }}</dt>
              <dd class="break-words whitespace-pre-wrap">{{ memory.doneText }}</dd>
            </div>

            <div v-if="memory.downReason !== null" class="flex flex-col gap-1">
              <dt class="text-muted">{{ $t("memories.label.retired_reason") }}</dt>
              <dd class="break-words whitespace-pre-wrap">{{ memory.downReason }}</dd>
              <dd v-if="memory.downAt !== null" class="tabular-nums text-muted">{{ formatStamp(memory.downAt) }}</dd>
            </div>

            <div v-if="memory.candidateWhy !== null" class="flex flex-col gap-1">
              <dt><UBadge color="warning" variant="subtle" size="sm" :label="$t('memories.label.candidate')" /></dt>
              <dd class="break-words whitespace-pre-wrap">{{ memory.candidateWhy }}</dd>
              <dd v-if="memory.candidateAt !== null" class="tabular-nums text-muted">{{ formatStamp(memory.candidateAt) }}</dd>
            </div>

            <!-- The message the row was set from: who sent it and when over its words where the
                 receiver keeps it, and its number alone where it does not. -->
            <div v-if="memory.messageId !== null" class="flex flex-col gap-1">
              <dt class="text-muted">{{ $t("memories.label.message") }}</dt>
              <template v-if="memory.message !== null">
                <dd class="flex flex-wrap items-center gap-x-2 text-muted">
                  <span>{{ memory.message.senderName }}</span>
                  <span class="tabular-nums">{{ formatStamp(memory.message.sentAt) }}</span>
                </dd>
                <dd class="break-words whitespace-pre-wrap">{{ memory.message.text }}</dd>
              </template>
              <dd v-else class="tabular-nums">{{ $t("memories.label.message_named", { id: memory.messageId }) }}</dd>
            </div>
          </dl>
        </div>
      </div>
    </template>
  </component>
</template>

<script setup lang="ts">
import { UDrawer, UModal } from "#components";
import { eventText, whomOf } from "~/domain/memories";
import { countText } from "~/domain/numbers";
import { useNarrow } from "bmo-ui/app/domain/screen";
import { formatStamp } from "bmo-ui/app/domain/time";

import type { MemoryRowView } from "#shared/types";

interface Props {
  open: boolean;
  // The row it shows. It is held by the screen past the press that closes the dialog, so what the
  // dialog draws stays until it is off the screen.
  memory: MemoryRowView | null;
}

const props = defineProps<Props>();

const emit = defineEmits<{ "update:open": [open: boolean] }>();

const narrow = useNarrow();

const shell = computed(() =>
  narrow.value
    ? { ui: { content: "max-h-[85svh]" } }
    : { ui: { content: "max-w-2xl max-h-[80vh]" } }
);
</script>
