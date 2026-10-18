<template>
  <UDashboardPanel :id="`placeholder-${screen.to.slice(1)}`">
    <template #header>
      <UDashboardNavbar :title="$t(screen.label)" :icon="screen.icon" />
    </template>

    <template #body>
      <div class="flex grow flex-col items-center justify-center gap-3 py-16 text-center">
        <UIcon :name="screen.icon" class="size-8 text-dimmed" aria-hidden="true" />
        <p class="text-sm text-muted">{{ $t("screens.message.not_available") }}</p>
      </div>
    </template>
  </UDashboardPanel>
</template>

<script setup lang="ts">
import { computed } from "vue";

import { SCREENS } from "~/domain/screens";

import type { Screen } from "~/domain/screens";

// The page standing in a screen whose unit is not yet split (docs/boundary.md, 「Screens of units not yet
// split」): the screen's own name and icon, and the one sentence saying the feature is not available in this
// console. It makes no call. The page names its screen by the address its entry stands at.
const props = defineProps<{ to: string }>();

const screen = computed<Screen>(() => {
  const found = SCREENS.find((entry) => entry.to === props.to && entry.placeholder === true);

  if (found === undefined) throw new Error(`no placeholder screen stands at ${props.to}`);

  return found;
});
</script>
