<template>
  <!-- The machine's room in one line, as the bot's minute watch last read it (docs/web.md, 「Live
       status screen」). The line opens what stands behind it: what holds a launch, whose memory it
       is, and the largest processes, with the way on to the hardware monitor. -->
  <SkeletonStatus v-if="!loaded" :label="$t('live.status.machine_loading')"><SkeletonBar class="h-4 w-48" /></SkeletonStatus>
  <UPopover v-else-if="standing !== null" :content="{ align: 'start' }" :ui="{ content: 'w-96 max-w-[calc(100vw-1rem)] p-3' }">
    <UButton
      :icon="held ? 'i-material-symbols-warning-outline' : 'i-material-symbols-memory'"
      color="neutral"
      variant="ghost"
      size="sm"
      class="min-w-0 max-w-full"
      :aria-label="$t('live.aria.machine', { line: parts.join(' · ') })"
      :ui="{ label: 'tabular-nums truncate', leadingIcon: held ? 'text-warning' : undefined }"
      :label="(narrow ? shortParts : parts).join(' · ')"
    />

    <template #content>
      <div class="flex flex-col gap-4 text-sm">
        <div class="flex flex-col gap-1">
          <p class="font-medium">{{ $t("live.label.machine_room") }}</p>
          <p v-if="asOf !== ''" class="text-muted tabular-nums">{{ $t("live.label.as_of", { time: asOf }) }}</p>
          <ul class="flex flex-col gap-1 tabular-nums">
            <li v-for="part in parts" :key="part">{{ part }}</li>
          </ul>
        </div>

        <div v-if="held" class="flex flex-col gap-1">
          <p class="flex items-center gap-2 font-medium">
            <UIcon name="i-material-symbols-warning-outline" class="size-5 shrink-0 text-warning" />{{ $t("live.label.machine_held") }}
          </p>
          <ul class="flex flex-col gap-1 tabular-nums">
            <li v-for="reason in standing.held" :key="reason">{{ reason }}</li>
          </ul>
        </div>

        <div v-if="standing.holders.length > 0" class="flex flex-col gap-1">
          <p class="font-medium">{{ $t("live.label.machine_holders") }}</p>
          <ul class="flex flex-col gap-1">
            <li v-for="holder in standing.holders.slice(0, 6)" :key="holder.place" class="flex items-center justify-between gap-3">
              <ItemTip :id="holder.todoId" class="min-w-0 truncate">{{ holderName(t, holder) }}</ItemTip>
              <span class="shrink-0 text-muted tabular-nums">
                {{ memoryAmount(holder.megabytes) }}<template v-if="holder.browsers > 0"> · {{ $t("live.label.machine_browsers", { n: holder.browsers }) }}</template>
              </span>
            </li>
          </ul>
        </div>

        <div v-if="standing.testbed !== undefined" class="flex flex-col gap-1">
          <p class="font-medium">{{ $t("live.label.machine_testbed") }}</p>
          <p v-if="testbed.length === 0" class="text-muted">{{ $t("live.label.machine_testbed_idle") }}</p>
          <ul v-else class="flex flex-col gap-1">
            <li v-for="row in testbed" :key="row.key" class="flex min-w-0 flex-col">
              <span class="truncate"><span class="text-muted tabular-nums">{{ row.label }} · </span>{{ row.who }}</span>
              <span class="truncate text-muted">{{ row.kept ? $t("live.label.machine_testbed_kept") : row.what }}</span>
            </li>
          </ul>
        </div>

        <!-- The machines work goes round, each with what it was given and its room
             (the agency's docs/work-nodes.md, 「The rotation」), where any node takes work. -->
        <div v-if="machines.length > 0" class="flex flex-col gap-1">
          <p class="font-medium">{{ $t("live.label.machine_nodes") }}</p>
          <ul class="flex flex-col gap-1">
            <li v-for="machine in machines" :key="machine.name" class="flex min-w-0 flex-col">
              <span class="flex items-center justify-between gap-3">
                <span class="min-w-0 truncate">{{ machine.label }}</span>
                <span class="shrink-0 text-muted tabular-nums">{{ $t("live.label.machine_node_counts", { jobs: count(machine.jobs), runs: count(machine.runs) }) }}</span>
              </span>
              <span class="truncate text-muted" :class="machine.room.held.length > 0 ? 'text-warning' : undefined">{{ machine.room.held.length === 0 ? $t("live.label.machine_node_room") : $t("live.label.machine_node_held", { reasons: machine.room.held.join(", ") }) }}</span>
            </li>
          </ul>
          <p v-if="nodes?.nextRun" class="text-muted">{{ $t("live.label.machine_next_run", { machine: nodes.nextRun }) }}</p>
        </div>

        <div v-if="standing.largest.length > 0" class="flex flex-col gap-1">
          <p class="font-medium">{{ $t("live.label.machine_largest") }}</p>
          <ul class="flex flex-col gap-1">
            <li v-for="process in standing.largest" :key="process.pid" class="flex items-center justify-between gap-3">
              <span class="min-w-0 truncate">{{ process.name }}<span class="text-muted"> · {{ process.place ?? process.pid }}</span></span>
              <span class="shrink-0 text-muted tabular-nums">{{ memoryAmount(process.megabytes) }}</span>
            </li>
          </ul>
        </div>

        <UButton :to="LOCAL_MONITOR_PATH" :label="$t('live.action.open_hardware')" icon="i-material-symbols-monitor-heart-outline" color="neutral" variant="outline" size="sm" class="self-start" />
      </div>
    </template>
  </UPopover>
</template>

<script setup lang="ts">
import { holderName, LOCAL_MONITOR_PATH, machineParts, machineShortParts, memoryAmount, testbedRows, type TestbedRow } from "~/domain/machine";
import { useNarrow } from "bmo-ui/app/domain/screen";

import type { MachineStanding, NodesReading } from "#shared/types";

const props = defineProps<{
  standing: MachineStanding | null;
  // The machines work goes round, as the same watch read them; null where it has not.
  nodes?: NodesReading | null;
  // When the bot read it, already worded; empty where it could not say.
  asOf: string;
  loaded: boolean;
}>();

const { t } = useI18n();
const narrow = useNarrow();

const held = computed<boolean>(() => (props.standing?.held.length ?? 0) > 0);
const parts = computed<string[]>(() => (props.standing === null ? [] : machineParts(t, props.standing)));
// A phone's toolbar has room for the pressure and the jobs alone; the rest is a press away.
const shortParts = computed<string[]>(() => (props.standing === null ? [] : machineShortParts(t, props.standing)));
// The machine's one queue for heavy commands, the holder first.
const testbed = computed<TestbedRow[]>(() => (props.standing === null ? [] : testbedRows(t, props.standing)));
// Shown only where any machine besides this one takes work.
const machines = computed(() => {
  const all = props.nodes?.machines ?? [];

  return all.some((machine) => machine.node !== null) ? all : [];
});
const counted = new Intl.NumberFormat("ko-KR");

function count(value: number): string {
  return counted.format(value);
}
</script>
