<template>
  <section :id="settingsAnchor('engines')" class="flex min-w-0 flex-col gap-3 p-4" aria-labelledby="engine-health-title">
    <h2 id="engine-health-title" class="text-base font-semibold text-highlighted">{{ $t("research.title.engines") }}</h2>

    <p v-if="settings.engines.length === 0" class="text-sm text-muted">{{ $t("research.status.no_engines") }}</p>

    <div v-for="group in groups" :key="group.category" class="flex min-w-0 flex-col gap-1">
      <h3 class="text-sm font-semibold text-muted">{{ categoryLabel($t, group.category) }}</h3>

      <ul class="flex min-w-0 flex-col gap-1" :aria-label="$t('research.aria.engines_of', { category: categoryLabel($t, group.category) })">
        <li
          v-for="engine in group.engines"
          :key="engine.engine"
          class="flex min-w-0 flex-wrap items-center gap-2 rounded-md border border-default px-2 py-2"
        >
          <UIcon :name="engineIcon(engine)" class="size-4 shrink-0" :class="toneOf(engine)" aria-hidden="true" />
          <SearchText class="font-mono text-sm text-highlighted" :text="engine.engine" :query="props.query" />
          <UBadge v-if="engine.korean" color="neutral" variant="subtle" size="sm" :label="$t('research.label.korean_first')" />
          <span class="text-sm" :class="toneOf(engine)">{{ engineStateLabel($t, engine) }}</span>
          <span class="min-w-0 break-words text-sm text-muted">{{ engineDetail($t, engine) }}</span>
        </li>
      </ul>
    </div>

    <div class="flex min-w-0 flex-col gap-1">
      <h3 class="text-sm font-semibold text-muted">{{ $t("research.title.cooldowns") }}</h3>
      <ul class="flex flex-wrap gap-x-4 gap-y-1 text-sm text-toned" :aria-label="$t('research.aria.cooldowns')">
        <li v-for="rule in settings.cooldowns" :key="rule.reason">{{ rule.reason }} {{ durationText($t, rule.ms) }}</li>
        <li>{{ $t("research.message.retry_after") }}</li>
      </ul>
    </div>
  </section>
</template>

<script setup lang="ts">
import { settingsAnchor } from "~/domain/agent-settings";
import { categoryLabel, durationText, engineDetail, engineIcon, engineStateLabel } from "~/domain/research";

import type { EngineView, ResearchSettingsView } from "#shared/research";

const props = defineProps<{ settings: ResearchSettingsView; query?: string }>();

const groups = computed(() => {
  const held = new Map<string, EngineView[]>();

  for (const engine of props.settings.engines) {
    held.set(engine.category, [...(held.get(engine.category) ?? []), engine]);
  }

  return [...held.entries()].map(([category, engines]) => ({ category, engines }));
});

// Colour repeats what the words already say; it never says it alone.
function toneOf(engine: EngineView): string {
  if (engine.state === "healthy") return "text-success";
  if (engine.state === "degraded" || engine.resting) return "text-warning";

  return "text-muted";
}
</script>
