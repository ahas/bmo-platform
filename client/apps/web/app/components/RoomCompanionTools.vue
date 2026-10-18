<template>
  <!-- What a room's menu opens for a companion answering in it, for the administrator alone
       (docs/web.md, 「Room screen」): its settings, in the very dialog the agent list
       opens, and its reset. Each keeps the companion it was opened on while it closes. -->
  <CompanionSettingsDialog v-if="settingsMeta !== null && settingsRow !== null" v-model:open="settingsOpen" :meta="settingsMeta" :agent="settingsRow" :reread="refresh" />
  <CompanionResetDialog v-if="resetAgent !== null" v-model:open="resetOpen" :agent="resetAgent" :label="labelOf(resetAgent)" :chat-id="props.chatId" />
</template>

<script setup lang="ts">
import { agentMetaOf } from "~/domain/agents";
import { useAgentCatalogue } from "~/domain/catalogue";

import type { AgentMeta } from "~/domain/agents";

const props = defineProps<{ chatId: number }>();

const { t } = useI18n();
const { agents: catalogue, refresh } = useAgentCatalogue();

const settingsAgent = ref<string | null>(null);
const settingsOpen = ref(false);
const resetAgent = ref<string | null>(null);
const resetOpen = ref(false);

const metaOf = (name: string): AgentMeta => agentMetaOf(t, name, catalogue.value.agents, []);
const settingsMeta = computed(() => (settingsAgent.value === null ? null : metaOf(settingsAgent.value)));
const settingsRow = computed(() => catalogue.value.agents.find((one) => one.name === settingsAgent.value) ?? null);

function labelOf(name: string): string {
  return metaOf(name).label;
}

defineExpose({
  labelOf,
  settings(agent: string): void {
    settingsAgent.value = agent;
    settingsOpen.value = true;
  },
  reset(agent: string): void {
    resetAgent.value = agent;
    resetOpen.value = true;
  },
});
</script>
