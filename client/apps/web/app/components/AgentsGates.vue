<template>
  <!-- Each agent's own gates (the gate unit's docs/gates.md, 「Each agent's own gates」): one block to an agent that
       answers in a room, its gates under the turn's parts in the turn's order, one switch to a gate.
       A dialog opening one agent's block alone leaves out the titles the dialog already shows. On
       the tab a switch saves as it is turned; inside a form the switches wait for its save
       (docs/web.md, 「Forms」). -->
  <section :id="props.bare ? undefined : settingsAnchor('agent-gates')" class="flex flex-col gap-3" :aria-labelledby="props.bare ? undefined : 'agent-gates-title'">
    <h2 v-if="!props.bare" id="agent-gates-title" class="text-lg font-semibold text-highlighted">{{ generalSectionTitle($t, "agent-gates") }}</h2>

    <div v-for="row in rows" :key="row.meta.name" class="flex flex-col gap-1">
      <h3 v-if="!props.bare" class="flex min-w-0 items-center gap-2 text-sm font-medium text-muted">
        <UIcon :name="row.meta.icon" class="size-5 shrink-0" />
        <SearchText class="truncate" :text="row.meta.label" :query="props.query" />
        <SearchText class="truncate font-mono" :text="row.meta.name" :query="props.query" />
      </h3>

      <div class="flex flex-col gap-3">
        <div v-for="part in partsOf(row.gates)" :key="part.part" class="flex flex-col gap-1">
          <h4 class="text-sm text-muted">{{ $t(GATE_PART_LABELS[part.part]) }}</h4>

          <ul class="flex flex-col divide-y divide-default overflow-hidden rounded-lg border border-default">
            <li v-for="gate in part.gates" :key="gate.key" class="flex min-h-12 items-center gap-3 px-4 py-2">
              <!-- On a phone the key stands on the line under the label, so neither is cut. -->
              <span class="flex min-w-0 flex-1 flex-col sm:flex-row sm:items-center sm:gap-2">
                <SearchText class="truncate text-sm font-medium text-highlighted" :text="agentLabel($t, gate.key)" :query="props.query" />
                <SearchText class="truncate font-mono text-sm text-muted" :text="gate.key" :query="props.query" />
              </span>
              <USwitch
                :model-value="shown(row.meta.name, gate)"
                :aria-label="$t('agents.aria.gate_of', { name: row.meta.label, gate: agentLabel($t, gate.key) })"
                :loading="busy === `${row.meta.name}:${gate.key}`"
                :disabled="busy !== ''"
                @update:model-value="turn(row.meta.name, gate, $event as boolean)"
              />
            </li>
          </ul>
        </div>
      </div>
    </div>

    <p v-if="failed !== ''" role="alert" class="text-sm text-error">{{ failed }}</p>
  </section>
</template>

<script setup lang="ts">
import { settingsAnchor } from "~/domain/agent-settings";
import { agentLabel, generalSectionTitle } from "~/domain/agents";
import { failureText } from "bmo-ui/app/domain/errors";
import { useFormPart } from "bmo-ui/app/domain/form-save";
import { GATE_PART_LABELS } from "~/domain/gate-order";

import type { AgentGateView, AgentSummary } from "#shared/types";
import type { AgentMeta } from "~/domain/agents";

interface Row {
  meta: AgentMeta;
  gates: AgentGateView[];
}

const props = withDefaults(
  defineProps<{
    // The agents in the order the agent list stands in, with what the bot says of each.
    agents: readonly { meta: AgentMeta; agent: AgentSummary }[];
    // The bot is asked to save and the screen then reads the list again, which this waits for.
    reread: () => Promise<void>;
    query?: string;
    // One agent's block alone, under a dialog's own title.
    bare?: boolean;
  }>(),
  { query: "", bare: false }
);

const { t } = useI18n();

const busy = ref("");
const failed = ref("");

// Only an agent whose turn passes gates has any: the bot names the rest with none.
const rows = computed<Row[]>(() =>
  props.agents.flatMap(({ meta, agent }) => (agent.gates == null || agent.gates.length === 0 ? [] : [{ meta, gates: agent.gates }]))
);

// The gates under the parts they stand in, in the order the bot listed them.
function partsOf(gates: readonly AgentGateView[]): { part: AgentGateView["part"]; gates: AgentGateView[] }[] {
  return (["pre", "post"] as const)
    .map((part) => ({ part, gates: gates.filter((gate) => gate.part === part) }))
    .filter((part) => part.gates.length > 0);
}

async function save(agent: string, gate: string, on: boolean): Promise<boolean> {
  busy.value = `${agent}:${gate}`;
  failed.value = "";

  try {
    await $fetch("/api/agent-gate", { method: "POST", body: { agent, gate, on } });
    await props.reread();

    return true;
  } catch (error) {
    failed.value = failureText(error, t("common.error.save_failed"));

    return false;
  } finally {
    busy.value = "";
  }
}

// The switches turned and not yet saved, by agent and gate, where a form holds this block.
const turned = ref<Record<string, boolean>>({});
const keyOf = (agent: string, gate: AgentGateView): string => JSON.stringify([agent, gate.key]);

const inForm = useFormPart({
  dirty: () => Object.keys(turned.value).length > 0,
  save: async () => {
    for (const [key, on] of Object.entries(turned.value)) {
      const [agent, gate] = JSON.parse(key) as [string, string];

      if (!(await save(agent, gate, on))) return false;

      const { [key]: _saved, ...rest } = turned.value;

      turned.value = rest;
    }

    return true;
  },
  discard: () => {
    turned.value = {};
    failed.value = "";
  },
});

function shown(agent: string, gate: AgentGateView): boolean {
  return turned.value[keyOf(agent, gate)] ?? gate.on;
}

// A switch turned back to what is saved holds no change.
function turn(agent: string, gate: AgentGateView, on: boolean): void {
  if (!inForm) {
    void save(agent, gate.key, on);
    return;
  }

  const { [keyOf(agent, gate)]: _held, ...rest } = turned.value;

  turned.value = on === gate.on ? rest : { ...rest, [keyOf(agent, gate)]: on };
}
</script>
