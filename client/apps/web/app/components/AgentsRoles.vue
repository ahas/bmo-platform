<template>
  <!-- Each agent's role and what the role gives it (the agency's docs/agent-roles.md), one row to an agent in the
       order the agent list stands in. Which roles there are is the bot's word, so while it is not
       answering a row shows the role the archive holds and nothing can be picked. -->
  <section :id="settingsAnchor('roles')" class="flex flex-col gap-3" aria-labelledby="roles-title">
    <h2 id="roles-title" class="text-lg font-semibold text-highlighted">{{ generalSectionTitle($t, "roles") }}</h2>

    <ul v-if="props.agents.length > 0" class="flex flex-col divide-y divide-default overflow-hidden rounded-lg border border-default">
      <li v-for="row in props.agents" :key="row.meta.name" class="flex min-h-12 flex-wrap items-center gap-x-3 gap-y-2 px-4 py-2">
        <UIcon :name="row.meta.icon" class="size-5 shrink-0 text-muted" />
        <div class="flex min-w-0 flex-1 flex-col">
          <span class="flex min-w-0 items-center gap-2">
            <SearchText class="truncate text-sm font-medium text-highlighted" :text="row.meta.label" :query="props.query" />
            <SearchText class="truncate font-mono text-sm text-muted" :text="row.meta.name" :query="props.query" />
          </span>
          <SearchText class="truncate text-sm text-muted" :text="givesText(row.agent.role ?? null)" :query="props.query" />
        </div>

        <!-- On a phone the choice stands on a line of its own under the name, so it is never cut. -->
        <div class="flex shrink-0 items-center max-sm:w-full max-sm:min-w-0 max-sm:ps-8">
          <USelect
            :model-value="row.agent.role ?? undefined"
            :items="itemsOf(row.agent.role ?? null)"
            value-key="value"
            size="sm"
            class="w-32 max-sm:w-full max-sm:min-w-0"
            :content="{ align: 'end' }"
            :placeholder="$t('agents.label.role_none')"
            :loading="busy === row.meta.name"
            :disabled="busy !== '' || props.roles.length === 0"
            :aria-label="$t('agents.aria.role_of', { name: row.meta.label })"
            @update:model-value="save(row, String($event))"
          />
        </div>
      </li>
    </ul>

    <p v-if="failed !== ''" role="alert" class="text-sm text-error">{{ failed }}</p>
  </section>
</template>

<script setup lang="ts">
import { settingsAnchor } from "~/domain/agent-settings";
import { generalSectionTitle } from "~/domain/agents";
import { failureText } from "bmo-ui/app/domain/errors";

import type { AgentSummary, RoleView } from "#shared/types";
import type { AgentMeta } from "~/domain/agents";

interface Row {
  meta: AgentMeta;
  agent: AgentSummary;
}

const props = withDefaults(
  defineProps<{
    // The agents in the order the agent list stands in, with what the bot says of each.
    agents: readonly Row[];
    // Every role the bot declares, in its order; empty while it is not answering.
    roles: readonly RoleView[];
    // The bot is asked to save and the page then reads the list again, which this waits for.
    reread: () => Promise<void>;
    query?: string;
  }>(),
  { query: "" }
);

const { t } = useI18n();

const busy = ref("");
const failed = ref("");

// Every role the bot declares; a role the row holds that the bot no longer declares stands as its own
// name, so the row still reads what it holds.
function itemsOf(held: string | null) {
  const items = props.roles.map((role) => ({ value: role.name, label: role.label }));

  return held === null || items.some((item) => item.value === held) ? items : [{ value: held, label: held }, ...items];
}

// What the role gives, the bundles by their short names; while the bot is not answering, the role's
// own name alone.
function givesText(held: string | null): string {
  if (held === null) return t("agents.label.role_none");

  const role = props.roles.find((one) => one.name === held);

  if (role === undefined) return held;

  return role.bundles.length === 0 ? t("agents.label.role_bundles_none") : role.bundles.map((bundle) => bundle.label).join(" · ");
}

async function save(row: Row, role: string): Promise<void> {
  if (role === row.agent.role) return;

  busy.value = row.meta.name;
  failed.value = "";

  try {
    await $fetch("/api/agent-role", { method: "POST", body: { agent: row.meta.name, role } });
    await props.reread();
  } catch (error) {
    failed.value = failureText(error, t("common.error.save_failed"));
  } finally {
    busy.value = "";
  }
}
</script>
