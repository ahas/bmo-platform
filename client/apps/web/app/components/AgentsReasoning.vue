<template>
  <!-- Each agent's own thinking and effort over its tier (the agency's docs/model-tiers.md, 「An agent's own thinking
       and effort」), one row to an agent the choice reaches. What does not reach the provider the agent
       begins on says so where it stands. -->
  <section :id="settingsAnchor('reasoning')" class="flex flex-col gap-3" aria-labelledby="reasoning-title">
    <h2 id="reasoning-title" class="text-lg font-semibold text-highlighted">{{ generalSectionTitle($t, "reasoning") }}</h2>

    <ul v-if="rows.length > 0" class="flex flex-col divide-y divide-default overflow-hidden rounded-lg border border-default">
      <li v-for="row in rows" :key="row.meta.name" class="flex min-h-12 flex-wrap items-center gap-x-3 gap-y-2 px-4 py-2">
        <UIcon :name="row.meta.icon" class="size-5 shrink-0 text-muted" />
        <div class="flex min-w-0 flex-1 flex-col">
          <span class="flex min-w-0 items-center gap-2">
            <SearchText class="truncate text-sm font-medium text-highlighted" :text="row.meta.label" :query="props.query" />
            <SearchText class="truncate font-mono text-sm text-muted" :text="row.meta.name" :query="props.query" />
          </span>
          <SearchText v-if="agentStartText($t, row.agent) !== ''" class="truncate text-sm text-muted" :text="agentStartText($t, row.agent)" :query="props.query" />
        </div>

        <!-- On a phone the two choices stand on a line of their own under the name, so neither is cut. -->
        <div class="flex shrink-0 items-center gap-3 max-sm:w-full max-sm:min-w-0 max-sm:ps-8">
          <label class="flex min-w-0 items-center gap-2 max-sm:flex-1">
            <span class="shrink-0 whitespace-nowrap text-sm text-muted">{{ $t("agents.label.column_thinking") }}</span>
            <USelect
              :model-value="thinkingValue(row.reasoning)"
              :items="thinkingItems(row.reasoning)"
              value-key="value"
              size="sm"
              class="w-24 max-sm:w-full max-sm:min-w-0"
              :content="{ align: 'end' }"
              :loading="busy === `thinking:${row.meta.name}`"
              :disabled="busy !== '' || !thinkingReaches(row.reasoning)"
              :aria-label="$t('agents.aria.thinking_of', { name: row.meta.label })"
              @update:model-value="saveThinking(row, String($event))"
            />
          </label>

          <label class="flex min-w-0 items-center gap-2 max-sm:flex-1">
            <span class="shrink-0 whitespace-nowrap text-sm text-muted">{{ $t("agents.label.column_effort") }}</span>
            <USelect
              :model-value="row.reasoning.effort ?? DEFAULT"
              :items="effortItems(row.reasoning)"
              value-key="value"
              size="sm"
              class="w-28 max-sm:w-full max-sm:min-w-0"
              :content="{ align: 'end' }"
              :loading="busy === `effort:${row.meta.name}`"
              :disabled="busy !== '' || !effortReaches(row.reasoning)"
              :aria-label="$t('agents.aria.effort_of', { name: row.meta.label })"
              @update:model-value="saveEffort(row, String($event))"
            />
          </label>

          <!-- A level the provider the agent begins on does not take leaves that provider on its tier. -->
          <UTooltip v-if="notApplied(row.reasoning)" :text="levelsText(row.reasoning)">
            <UBadge color="warning" variant="subtle" size="sm" :label="$t('agents.label.reasoning_not_applied')" />
          </UTooltip>
        </div>
      </li>
    </ul>

    <p v-if="failed !== ''" role="alert" class="text-sm text-error">{{ failed }}</p>
  </section>
</template>

<script setup lang="ts">
import { settingsAnchor } from "~/domain/agent-settings";
import { generalSectionTitle } from "~/domain/agents";
import { agentStartText } from "~/domain/status";
import { failureText } from "bmo-ui/app/domain/errors";

import type { AgentReasoningView, AgentSummary } from "#shared/types";
import type { AgentMeta } from "~/domain/agents";

interface Row {
  meta: AgentMeta;
  agent: AgentSummary;
  reasoning: AgentReasoningView;
}

const props = withDefaults(
  defineProps<{
    // The agents in the order the agent list stands in, with what the bot says of each.
    agents: readonly { meta: AgentMeta; agent: AgentSummary }[];
    // The bot is asked to save and the page then reads the list again, which this waits for.
    reread: () => Promise<void>;
    query?: string;
  }>(),
  { query: "" }
);

const { t } = useI18n();

const DEFAULT = "default";
const ON = "on";
const OFF = "off";
// The one scale an agent's level is named on, lowest first (agents/agent/reasoning.ts).
const LEVELS = ["low", "medium", "high", "xhigh", "max"] as const;

const busy = ref("");
const failed = ref("");

// Only the agents a choice of their own reaches have one: the bot names the rest with none.
const rows = computed<Row[]>(() =>
  props.agents.flatMap(({ meta, agent }) => (agent.reasoning == null ? [] : [{ meta, agent, reasoning: agent.reasoning }]))
);

function thinkingValue(reasoning: AgentReasoningView): string {
  return reasoning.thinking === null ? DEFAULT : reasoning.thinking ? ON : OFF;
}

// Whether the switch reaches the provider the agent begins on. A switch already set stays open to
// be given back, wherever it stands now.
function thinkingReaches(reasoning: AgentReasoningView): boolean {
  return reasoning.reaches === null || reasoning.reaches.thinking || reasoning.thinking !== null;
}

function effortReaches(reasoning: AgentReasoningView): boolean {
  return reasoning.reaches === null || reasoning.reaches.levels.length > 0 || reasoning.effort !== null;
}

function thinkingItems(reasoning: AgentReasoningView) {
  if (!thinkingReaches(reasoning)) return [{ value: DEFAULT, label: t("agents.label.reasoning_unsupported") }];

  return [
    { value: DEFAULT, label: t("agents.label.reasoning_default") },
    { value: ON, label: t("agents.label.reasoning_on") },
    { value: OFF, label: t("agents.label.reasoning_off") }
  ];
}

// Every level stands, since an agent that walks may land on a provider that takes it; one the
// provider it begins on does not take is marked so in the list.
function effortItems(reasoning: AgentReasoningView) {
  if (!effortReaches(reasoning)) return [{ value: DEFAULT, label: t("agents.label.reasoning_unsupported") }];

  return [
    { value: DEFAULT, label: t("agents.label.reasoning_default") },
    ...LEVELS.map((level) => ({
      value: level,
      label: level,
      ...(reasoning.reaches === null || reasoning.reaches.levels.includes(level) ? {} : { description: t("agents.label.reasoning_not_applied") })
    }))
  ];
}

function notApplied(reasoning: AgentReasoningView): boolean {
  const reaches = reasoning.reaches;

  if (reaches === null) return false;

  return (reasoning.effort !== null && !reaches.levels.includes(reasoning.effort)) || (reasoning.thinking !== null && !reaches.thinking);
}

function levelsText(reasoning: AgentReasoningView): string {
  const levels = reasoning.reaches?.levels ?? [];

  return levels.length === 0 ? t("agents.label.reasoning_no_levels") : t("agents.label.reasoning_levels", { levels: levels.join(", ") });
}

async function save(row: Row, what: "thinking" | "effort", body: { thinking: boolean | null; effort: string | null }): Promise<void> {
  busy.value = `${what}:${row.meta.name}`;
  failed.value = "";

  try {
    await $fetch("/api/agent-reasoning", { method: "POST", body: { agent: row.meta.name, ...body } });
    await props.reread();
  } catch (error) {
    failed.value = failureText(error, t("common.error.save_failed"));
  } finally {
    busy.value = "";
  }
}

async function saveThinking(row: Row, value: string): Promise<void> {
  if (value === thinkingValue(row.reasoning)) return;

  await save(row, "thinking", { thinking: value === DEFAULT ? null : value === ON, effort: row.reasoning.effort });
}

async function saveEffort(row: Row, value: string): Promise<void> {
  if (value === (row.reasoning.effort ?? DEFAULT)) return;

  await save(row, "effort", { thinking: row.reasoning.thinking, effort: value === DEFAULT ? null : value });
}
</script>
