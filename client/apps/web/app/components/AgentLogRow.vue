<template>
  <li class="flex flex-col gap-1" :class="{ 'ms-4 border-s border-default ps-4': props.row.depth > 0 }">
    <div class="flex flex-wrap items-center gap-2">
      <!-- The clock is a figure and not code: it reads in the interface's own face, and the
           columns of it line up because the digits are tabular. What is monospaced on this row
           is the tool's name, its arguments and the bodies below — the places where a command,
           a path or a fragment of code actually stands. -->
      <time class="text-sm tabular-nums text-dimmed">{{ formatTime(line.at) }}</time>
      <ContextStateMark v-if="mark !== null" :look="mark" />
      <!-- A call wears the tool's own name as its chip — `Bash`, `Read`, `memory_add` — with the
           icon the tool declares, in place of a kind word. Any other line wears its kind's word, and
           a result names its tool beside it. -->
      <UBadge v-if="chip !== null" :color="look.color" variant="subtle" size="sm" :icon="chip.icon" :label="chip.label" class="font-mono" />
      <UBadge v-else :color="look.color" variant="subtle" size="sm" :label="look.label" />
      <span v-if="chip === null && line.tool !== null" class="font-mono text-sm break-all text-highlighted">{{ line.tool.name }}</span>
    </div>

    <!-- A call says on its own row what it did — the argument that tells, and the description
         it came with — so a log reads without opening anything. The row is the handle: pressing
         it lays the arguments out one under its own name, each value standing as it was sent so
         a command or a patch reads as the one thing it is. What was stored is a press further
         in. A call is a record of what was sent, so all of it is shown as text and never as markup. -->
    <UChatTool
      v-if="line.kind === 'tool'"
      :text="changed !== null ? changed.head : call.head === '' ? $t('agents.label.no_arguments') : call.head"
      :suffix="call.description ?? undefined"
      chevron="leading"
      :ui="{ label: 'font-mono break-all whitespace-normal line-clamp-2 text-start' }"
    >
      <div class="flex flex-col gap-2">
        <div
          v-for="(argument, index) in fold.args"
          :key="index"
          class="flex gap-2"
          :class="argument.block ? 'flex-col' : 'flex-wrap items-center'"
        >
          <span v-if="argument.name !== ''" class="text-sm text-dimmed">{{ argument.name }}</span>

          <div class="flex min-w-0 flex-col gap-1">
            <pre class="font-mono text-sm break-words whitespace-pre-wrap text-muted">{{ argument.value }}</pre>
            <p v-if="argument.cut" class="text-sm text-dimmed">{{ CUT }}</p>
          </div>
        </div>

        <!-- A line the cap cut, and one whose JSON never closed, are the same news to a reader:
             the record itself is short of the call. It is said once for the line, in its own
             words, so it is not mistaken for the clipping this screen did to one value. -->
        <p v-if="line.truncated || !fold.whole" class="text-sm text-dimmed">{{ SHORT }}</p>

        <UChatTool :text="$t('common.label.original')" chevron="leading">
          <pre class="font-mono text-sm break-words whitespace-pre-wrap text-muted">{{ line.text }}</pre>
        </UChatTool>
      </div>
    </UChatTool>

    <!-- Any other line is either words the run or the bot wrote, which stand as the markdown they
         were written in, or a record — how the run was opened, what it was handed — which stands
         whole as it was stored, wrapping inside the log rather than widening it. -->
    <template v-else>
      <MarkdownText v-if="look.words" :text="line.text" class="text-sm" />
      <pre v-else class="font-mono text-sm break-words whitespace-pre-wrap">{{ line.text }}</pre>
      <p v-if="line.truncated" class="text-sm text-dimmed">{{ SHORT }}</p>
    </template>

    <!-- A result reads the same way: how long it is and its first line stand on the row, and
         the whole of it is one press away. A result that carries what it changed reads as the
         terminal draws it instead: how many lines went in and out, and under it the diff of each
         file, standing open unless it is long; what the tool said is a press further in. A file
         tool's one file is already named on the call's own row; a shell command's files, and a
         patch of several, each stand under a head of their own. -->
    <template v-for="result in results" :key="result.line.id">
      <UChatTool
        v-if="result.changes !== null"
        :text="result.changes.summary"
        :default-open="result.changes.rows <= OPEN_DIFF_ROWS"
        chevron="leading"
      >
        <div class="flex flex-col gap-2">
          <div v-for="(file, index) in result.changes.files" :key="index" class="flex flex-col gap-1">
            <p v-if="file.byCommand || result.changes.files.length > 1" class="font-mono text-sm break-all text-highlighted">
              {{ file.head }}
            </p>
            <AgentLogDiff :view="file" />
          </div>

          <p v-if="result.changes.more !== null" class="text-sm text-dimmed">{{ result.changes.more }}</p>

          <UChatTool :text="$t('common.label.original')" chevron="leading">
            <pre class="font-mono text-sm break-words whitespace-pre-wrap text-muted">{{ result.line.text }}</pre>
            <p v-if="result.line.truncated" class="text-sm text-dimmed">{{ SHORT }}</p>
          </UChatTool>
        </div>
      </UChatTool>

      <UChatTool
        v-else
        :text="resultLabel(result.line)"
        :suffix="preview(result.line).head"
        chevron="leading"
        :ui="{ suffix: 'font-mono' }"
      >
        <pre class="font-mono text-sm break-words whitespace-pre-wrap text-muted">{{ result.line.text }}</pre>
        <p v-if="result.line.truncated" class="text-sm text-dimmed">{{ SHORT }}</p>
      </UChatTool>
    </template>
  </li>
</template>

<script setup lang="ts">
import { contextStateLook, lineLook, resultPreview, toolArguments, toolCall, toolChip } from "~/domain/log";
import { catalogueAgents } from "~/domain/agents";
import { countText } from "~/domain/numbers";
import { changesView, OPEN_DIFF_ROWS } from "~/domain/patch";
import { formatTime } from "bmo-ui/app/domain/time";

import type { AgentLogLine } from "#shared/types";
import type { LogRow } from "~/domain/log";
import type { ChangesView, DiffView } from "~/domain/patch";

interface Props {
  row: LogRow;
}

const props = defineProps<Props>();
const { t } = useI18n();

// What one value says when this screen would not stand the whole of it. The rest of that value
// is in the record and the record is right here, one press further in.
const CUT = t("agents.label.cut");

// And what the line says when the record itself is short of the call: the writer cut it at its
// own cap, so what stands in the fold is all there ever was of it. It is a different piece of
// news from a value this screen clipped, and the two stand together on the very line where the
// cap fell — one word said twice would read as the screen stuttering rather than as two facts.
const SHORT = t("agents.label.record_ends");

const line = computed(() => props.row.line);

const look = computed(() => lineLook(t, line.value));
// Whether a reference document went in full or as an already-loaded mark.
const mark = computed(() => contextStateLook(t, line.value));

const call = computed(() => toolCall(line.value.name, line.value.text));

// What the fold reads out of the call. It is only ever asked for under a call's own fold, so a
// line of any other kind never pays for it.
const fold = computed(() => toolArguments(line.value.text));

// The checkout the agent's runs work in, so a path inside it reads the way the run wrote it.
const root = computed<string | null>(
  () => catalogueAgents().find((agent) => agent.name === line.value.agent)?.cwd ?? null
);

// Each result with what it changed drawn out, where it carries any.
const results = computed<Array<{ line: AgentLogLine; changes: ChangesView | null }>>(() =>
  props.row.results.map((result) => ({ line: result, changes: changesView(t, result.patches, result.patchesMore, root.value) }))
);

// The first file a file tool's results changed heads the call as the terminal heads it. A shell
// command keeps its own line, and its files stand under it.
const changed = computed<DiffView | null>(
  () => results.value.flatMap((result) => result.changes?.files ?? []).find((file) => !file.byCommand) ?? null
);

// The chip a call wears in place of its kind's word.
const chip = computed(() => toolChip(line.value));

function preview(result: AgentLogLine): { head: string; lines: number } {
  return resultPreview(result.text);
}

// How much came back, so a glance says whether it is worth opening.
function resultLabel(result: AgentLogLine): string {
  const { lines } = preview(result);

  return lines > 1 ? t("agents.label.result_lines", { n: countText(lines) }) : t("agents.label.result");
}
</script>
