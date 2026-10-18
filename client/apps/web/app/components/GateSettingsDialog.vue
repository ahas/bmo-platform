<template>
  <!-- One gate's settings: its own row of the agent management screen's gate table, the same cells
       saving through the same paths, so a gate is edited one way wherever it is opened. -->
  <SettingsDialog v-model:open="open" :title="title" :description="props.gate" wide>
    <AgentsRouteTable gates :only="[props.gate]" />
  </SettingsDialog>
</template>

<script setup lang="ts">
import { useAgentCatalogue } from "~/domain/catalogue";

interface Props {
  // The gate's work key, which is also its agent name.
  gate: string;
}

const props = defineProps<Props>();
const { t } = useI18n();

const open = defineModel<boolean>("open", { required: true });

const { agents } = useAgentCatalogue();

const title = computed(() => agents.value.agents.find((agent) => agent.name === props.gate)?.label ?? props.gate);
</script>
