<template>
  <!-- Everything the agents are holding, as a table rather than a document: which kind of thing each row
       is, what it asks for and for whom, what it waits on, how it ended, the watch serving it, and
       the switch that takes one down or puts it back. A row is never deleted, so the ones that are
       finished and the ones that are down stay behind the state filter, which opens on the ones
       still open. -->
  <!-- The rows are repeated records, so the table fills the panel edge to edge and scrolls inside
       itself, its header and sideways scrollbar held to the panel's edges. -->
  <UDashboardPanel id="memories" :ui="{ body: 'p-0 sm:p-0 gap-0 overflow-hidden [&>*]:max-w-none' }">
    <template #header>
      <UDashboardNavbar :title="$t('screens.label.memories')" icon="i-material-symbols-rule-folder-outline">
        <template #right>
          <DeployMark />
          <TodoOpenButton />
          <UsageMeters />
        </template>
      </UDashboardNavbar>

      <UDashboardToolbar>
        <template #left>
          <USelect
            v-model="state"
            :items="states"
            value-key="value"
            :content="{ align: 'start' }"
            size="sm"
            class="tabular-nums"
            :aria-label="$t('memories.label.state_filter')"
          />
          <USelect v-model="kind" :items="KINDS" :content="{ align: 'start' }" size="sm" :aria-label="$t('memories.label.kind')" />
          <USelect
            v-model="agent"
            :items="agents"
            value-key="value"
            :content="{ align: 'start' }"
            size="sm"
            :aria-label="$t('memories.label.agent')"
          />
        </template>

        <template #right>
          <UTooltip :text="$t('common.action.refresh')">
            <UButton
              size="sm"
              color="neutral"
              variant="ghost"
              icon="i-material-symbols-refresh"
              :loading="reading"
              :aria-label="$t('common.action.refresh')"
              @click="reread"
            />
          </UTooltip>
        </template>
      </UDashboardToolbar>
    </template>

    <template #body>
      <div :aria-busy="reading && !ready" class="flex min-h-0 flex-1 flex-col">
        <div v-if="!ready && !failed" class="min-h-0 flex-1 overflow-y-auto">
          <ContentSkeleton v-if="narrow" class="px-4" :rows="4" list-class="flex flex-col divide-y divide-default" :label="$t('memories.status.loading')">
            <div class="flex flex-col gap-2 py-3">
              <div class="flex items-center gap-2">
                <SkeletonBar class="h-5 w-16" />
                <SkeletonBar class="h-4 flex-1" />
                <SkeletonBar class="h-5 w-8 rounded-full" />
              </div>
              <SkeletonBar class="h-4 w-1/2" />
              <SkeletonBar class="h-4 w-2/3" />
            </div>
          </ContentSkeleton>
          <DataTableSkeleton v-else :headers="COLUMNS.map((column) => String(column.header))" :rows="5" :label="$t('memories.status.loading')" />
        </div>

        <UAlert
          v-else-if="failed"
          class="m-4 w-auto sm:m-6"
          color="error"
          variant="subtle"
          icon="i-material-symbols-error-outline"
          :description="failed"
        />

        <p v-else-if="shown.length === 0" class="py-8 text-center text-sm text-muted">
          {{ nothing }}
        </p>

        <!-- At a phone's width a table shows two columns of ten, so each row stands as an item: its
             type and action on top beside the switch, and under them a label and a value for each
             field the row has. A field it lacks is left out rather than drawn blank. The whole item
             is pressed to open the row, through a button laid over it, so its words stay words to a
             reader and the switch stays above the button as a press of its own. -->
        <ul v-else-if="narrow" class="flex min-h-0 flex-1 flex-col divide-y divide-default overflow-y-auto">
          <li
            v-for="memory in shown"
            :key="memory.slug"
            data-memory-item
            class="relative flex flex-col gap-2 px-4 py-3 text-sm has-[>button:hover]:bg-elevated/50 has-[>button:active]:bg-accented/50"
          >
            <button type="button" class="absolute inset-0 cursor-pointer focus-inset" :aria-label="$t('memories.aria.open_named', { name: memory.slug })" @click="open(memory)" />

            <div class="flex items-start gap-2">
              <UBadge color="neutral" variant="subtle" size="sm" :label="memory.type" class="shrink-0" />
              <p class="min-w-0 flex-1 break-words text-highlighted">{{ memory.action }}</p>
              <div class="relative z-10 shrink-0">
                <MemoryActiveSwitch :memory="memory" :busy="busy" :retire="(reason) => takeDown(memory, reason)" :restore="() => putBack(memory)" />
              </div>
            </div>

            <dl class="pair-list items-start gap-x-3 gap-y-1">
              <dt class="text-muted">{{ $t("memories.label.agent") }}</dt>
              <dd class="min-w-0 break-words">{{ memory.agentLabel }}</dd>

              <dt class="text-muted">{{ $t("memories.label.whom") }}</dt>
              <dd class="min-w-0 break-words">{{ whomOf(t, memory) }}</dd>

              <template v-if="waits(memory)">
                <dt class="text-muted">{{ $t("memories.label.condition") }}</dt>
                <dd class="flex min-w-0"><MemoryCondition :memory="memory" wrap /></dd>
              </template>

              <dt class="text-muted">{{ $t("memories.label.created_at") }}</dt>
              <dd class="tabular-nums text-muted">{{ formatStamp(memory.setAt) }}</dd>

              <template v-if="memory.doneAt !== null">
                <dt class="text-muted">{{ $t("memories.label.ended_at") }}</dt>
                <dd class="min-w-0">
                  <span class="tabular-nums text-muted">{{ formatStamp(memory.doneAt) }}</span>
                  <p v-if="memory.doneText !== null" class="break-words">{{ memory.doneText }}</p>
                </dd>
              </template>

              <template v-if="memory.watch !== null">
                <dt class="text-muted">{{ $t("memories.label.watch") }}</dt>
                <dd class="flex min-w-0"><MemoryWatch :watch="memory.watch" /></dd>
              </template>

              <template v-if="matched(memory)">
                <dt class="text-muted">{{ $t("memories.label.hits") }}</dt>
                <dd class="tabular-nums">
                  {{ $t("memories.label.hit_count", { n: countText(memory.hits) }) }}<template v-if="memory.lastHitAt !== null"> · <span class="text-muted">{{ formatStamp(memory.lastHitAt) }}</span></template>
                </dd>
              </template>

              <template v-if="memory.candidateWhy !== null">
                <dt><UBadge color="warning" variant="subtle" size="sm" :label="$t('memories.label.candidate')" /></dt>
                <dd class="min-w-0 break-words text-muted">{{ memory.candidateWhy }}</dd>
              </template>

              <template v-if="memory.downReason !== null">
                <dt class="text-muted">{{ $t("memories.label.retired_reason") }}</dt>
                <dd class="min-w-0 break-words text-muted">{{ memory.downReason }}</dd>
              </template>
            </dl>
          </li>
        </ul>

        <!-- A cell's words wrap to further lines at its column's width, so a row is as tall as its
             fullest cell, and every line stands at one height, so a cell of one line sits level with
             the first line of its neighbours. A column of its own content's width stands up to a
             ceiling of its own until a column is dragged, and then takes whatever width it is
             given. A cell with nothing to say is blank, the same blank in every column. Pressing a
             row opens the whole of it; the switch is a press of its own. -->
        <DataTable v-else :data="shown" :columns="COLUMNS" layout="fill" selectable @select="open">
          <template #agent-cell="{ row }">
            <div class="py-2 ps-4 pe-2 leading-6">{{ row.original.agentLabel }}</div>
          </template>

          <template #type-cell="{ row }">
            <div class="px-2 py-2 leading-6">{{ row.original.type }}</div>
          </template>

          <template #action-cell="{ row, held }">
            <div :class="[WRAPPED, 'px-2 text-highlighted', held ? '' : 'min-w-80']">{{ row.original.action }}</div>
          </template>

          <template #whom-cell="{ row, held }">
            <div :class="[WRAPPED, 'px-2', held ? '' : 'w-max max-w-56']">{{ whomOf(t, row.original) }}</div>
          </template>

          <template #condition-cell="{ row, held }">
            <div :class="[WRAPPED, 'px-2', held ? '' : 'w-max max-w-64']"><MemoryCondition :memory="row.original" wrap /></div>
          </template>

          <template #set-cell="{ row }">
            <div class="px-2 py-2 leading-6 tabular-nums text-muted">{{ formatStamp(row.original.setAt) }}</div>
          </template>

          <template #done-cell="{ row, held }">
            <div :class="[WRAPPED, 'px-2', held ? '' : 'w-max max-w-72']">
              <template v-if="row.original.doneAt !== null">
                <span class="whitespace-nowrap tabular-nums text-muted">{{ formatStamp(row.original.doneAt) }}</span>
                <template v-if="row.original.doneText !== null"> · {{ row.original.doneText }}</template>
              </template>
            </div>
          </template>

          <!-- A watch's name has a settled form, so it is the one thing here cut to one line rather
               than wrapped; its whole name rests on it for a pointer. -->
          <template #watch-cell="{ row, held }">
            <div :class="['px-2 py-2', held ? '' : 'w-max max-w-56']">
              <div class="flex min-h-6 items-center">
                <MemoryWatch v-if="row.original.watch !== null" :watch="row.original.watch" />
              </div>
            </div>
          </template>

          <template #hits-cell="{ row }">
            <div class="flex min-h-6 items-center justify-end gap-1 px-2 py-2 box-content tabular-nums">
              <template v-if="matched(row.original)">
                <span>{{ $t("memories.label.hit_count", { n: countText(row.original.hits) }) }}</span>
                <span v-if="row.original.lastHitAt !== null" class="text-muted">· {{ formatStamp(row.original.lastHitAt) }}</span>
              </template>
            </div>
          </template>

          <template #active-cell="{ row }">
            <div class="flex min-h-6 items-center px-2 py-2 box-content">
              <MemoryActiveSwitch :memory="row.original" :busy="busy" :retire="(reason) => takeDown(row.original, reason)" :restore="() => putBack(row.original)" />
            </div>
          </template>

          <!-- Why a row came down, and the mark of a row raised as a candidate for it, which a person
               settles with the switch beside it. -->
          <template #down-cell="{ row, held }">
            <div :class="[WRAPPED, 'ps-2 pe-4 text-muted', held ? '' : 'w-max max-w-56']">
              <template v-if="row.original.candidateWhy !== null">
                <UBadge color="warning" variant="subtle" size="sm" :label="$t('memories.label.candidate')" class="me-1" />{{ row.original.candidateWhy }}
              </template>
              <template v-if="row.original.downReason !== null">{{ row.original.downReason }}</template>
            </div>
          </template>
        </DataTable>

        <MemoryDetail v-model:open="detailOpen" :memory="chosen" />
      </div>
    </template>
  </UDashboardPanel>
</template>

<script setup lang="ts">
import { memoryState, memoryStates, whomOf } from "~/domain/memories";
import { countText } from "~/domain/numbers";
import { useNarrow } from "bmo-ui/app/domain/screen";
import { formatStamp } from "bmo-ui/app/domain/time";

import type { MemoryState } from "~/domain/memories";
import type { TableColumn } from "@nuxt/ui";
import type { MemoriesView, MemoryChanged, MemoryRowView } from "#shared/types";

const { t } = useI18n();
useHead({ title: t("screens.label.memories") });

const narrow = useNarrow();

// The action is the row's title — the words every row has, saying what it asks for — so it is the
// one column that takes the width left over. Every other column is laid out at the width of its own
// content, which is what a width of one pixel beside cells that set their own width comes to.
const OWN_WIDTH = { style: { th: { width: "1px" } } };

// A cell of words: one line of the one height every cell's line stands at, and further lines where
// its width does not hold them.
const WRAPPED = "py-2 leading-6 whitespace-normal break-words";

const COLUMNS: TableColumn<MemoryRowView>[] = [
  { id: "agent", header: t("memories.label.agent"), meta: OWN_WIDTH },
  { id: "type", header: t("memories.label.kind"), meta: OWN_WIDTH },
  { id: "action", header: t("memories.label.action") },
  { id: "whom", header: t("memories.label.whom"), meta: OWN_WIDTH },
  { id: "condition", header: t("memories.label.condition"), meta: OWN_WIDTH },
  { id: "set", header: t("memories.label.created_at"), meta: OWN_WIDTH },
  { id: "done", header: t("memories.label.ended_at"), meta: OWN_WIDTH },
  { id: "watch", header: t("memories.label.watch"), meta: OWN_WIDTH },
  { id: "hits", header: t("memories.label.hits"), meta: OWN_WIDTH },
  { id: "active", header: t("memories.label.active"), meta: OWN_WIDTH },
  { id: "down", header: t("memories.label.retired_reason"), meta: OWN_WIDTH }
];

// The types a row may be, as the table itself spells them. They are the column's own values rather
// than words of the interface, so they stand as they are written; the one that narrows nothing
// names the column instead, since a bare "all" beside a switch says nothing about what it is all of.
const KINDS = [t("memories.label.all_kinds"), "promise", "check", "followup", "situation", "state"];
const EVERY = KINDS[0]!;

// The states a row stands in (`docs/memories.md`, 「The console screen」), each choice with its
// count, and what the list says when a state has no row.
const STATE_LABELS = {
  open: "memories.label.state_open",
  done: "memories.label.state_done",
  down: "memories.label.state_down",
  all: "memories.label.state_all"
} as const;
const STATE_NOTHING = {
  open: "memories.status.none_open",
  done: "memories.status.none_done",
  down: "memories.status.none_down"
} as const;

const memories = ref<MemoryRowView[]>([]);
const ready = ref(false);
const reading = ref(false);
const failed = ref("");
const state = ref<MemoryState | "all">("open");
const kind = ref<string>(EVERY);
// Whose rows are shown: every agent's at first, or one agent's. The agents are the ones the rows name,
// under what the agents table calls them, so a new agent stands here with its first row
// (docs/memories.md, 「The console screen」).
// The choice of every agent is a value no agent is named by; the select library refuses an empty one.
const ALL_AGENTS = "*";
const agent = ref<string>(ALL_AGENTS);
const agents = computed(() => [
  { value: ALL_AGENTS, label: t("memories.label.all_agents") },
  ...[...new Map(memories.value.map((memory) => [memory.agent, memory.agentLabel])).entries()].map(([value, label]) => ({ value, label }))
]);
const busy = ref("");
// The row the dialog shows, held past the press that closes it so the dialog keeps what it drew
// until it is off the screen.
const chosen = ref<MemoryRowView | null>(null);
const detailOpen = ref(false);

// Each state choice carries how many rows it would show within the type filter, so the count of the
// open ones is what the agents still hold, at every width.
// Until the rows are read there is nothing to count, so no choice claims a nought.
const ofKind = computed(() => memories.value.filter((memory) => (kind.value === EVERY || memory.type === kind.value) && (agent.value === ALL_AGENTS || memory.agent === agent.value)));
const states = computed(() => [...memoryStates, "all" as const].map((value) => ({
  value,
  label: t(STATE_LABELS[value], {
    n: !ready.value ? "" : countText(value === "all" ? ofKind.value.length : ofKind.value.filter((memory) => memoryState(memory) === value).length)
  })
})));
const shown = computed(() => ofKind.value.filter((memory) => state.value === "all" || memoryState(memory) === state.value));
const nothing = computed(() => {
  if (state.value !== "all") {
    return t(STATE_NOTHING[state.value]);
  }

  return kind.value === EVERY ? t("memories.status.none") : t("memories.status.none_of_kind", { kind: kind.value });
});

async function reread(): Promise<void> {
  reading.value = true;

  try {
    memories.value = (await $fetch<MemoriesView>("/api/memories")).memories;
    failed.value = "";
    ready.value = true;
  } catch (error) {
    failed.value = (error as { data?: { message?: string } }).data?.message ?? t("memories.error.read_failed");
  } finally {
    reading.value = false;
  }
}

// One row's flag, which is the only write this screen makes. The row is read again from the answer
// rather than from a second fetch, so nothing on the screen is taken down and put back.
async function setActive(memory: MemoryRowView, active: boolean, reason: string): Promise<void> {
  busy.value = memory.slug;

  try {
    const changed = await $fetch<MemoryChanged>("/api/memories", {
      method: "POST",
      body: { slug: memory.slug, active, why: reason }
    });

    memories.value = memories.value.map((one) => (one.slug === memory.slug ? changed.memory : one));
    if (chosen.value?.slug === memory.slug) chosen.value = changed.memory;
    failed.value = "";
  } catch (error) {
    failed.value = (error as { data?: { message?: string } }).data?.message ?? t("memories.error.change_failed");
  } finally {
    busy.value = "";
  }
}

async function takeDown(memory: MemoryRowView, reason: string): Promise<void> {
  await setActive(memory, false, reason);
}

async function putBack(memory: MemoryRowView): Promise<void> {
  await setActive(memory, true, "");
}

// Pressing a row opens the whole of it, from the table and from the phone's list alike.
function open(memory: MemoryRowView): void {
  chosen.value = memory;
  detailOpen.value = true;
}

// Whether a row waits on anything the condition's place can show.
function waits(memory: MemoryRowView): boolean {
  return memory.event !== null || memory.type === "state" || memory.condition !== "";
}

// Whether the matcher is ever asked about the row, which is what its match count counts: only a row
// with a condition goes to it.
function matched(memory: MemoryRowView): boolean {
  return memory.condition !== "";
}

onMounted(reread);
</script>
