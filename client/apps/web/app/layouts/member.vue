<template>
  <UDashboardGroup :style="shell">
    <UDashboardSidebar
      id="member"
      v-model:open="open"
      v-model:collapsed="sidebarFolded"
      collapsible
      resizable
      :max-size="Infinity"
      mode="drawer"
      :menu="{ direction: 'left', handle: false, handleOnly: true }"
      :ui="{ content: 'flex-col w-4/5 max-w-xs' }"
    >
      <template #header="{ collapsed }">
        <NuxtLink
          v-if="!collapsed"
          :to="MEMBER_HOME"
          class="flex items-center gap-2 rounded-md px-2.5 py-1 transition-colors hover:bg-elevated active:bg-accented"
        >
          <BmoMark class="size-5 shrink-0" />
          <span class="font-bold text-highlighted">{{ $t("app.label.name") }}</span>
        </NuxtLink>

        <UTooltip :text="collapsed ? $t('common.action.expand') : $t('common.action.collapse')" :content="{ side: 'right' }">
          <UDashboardSidebarCollapse class="ms-auto hidden lg:flex" />
        </UTooltip>
      </template>

      <template #footer="{ collapsed }">
        <UserMenu :collapsed="collapsed" />
      </template>
    </UDashboardSidebar>

    <div class="flex min-h-0 min-w-0 flex-1">
      <slot />
    </div>
  </UDashboardGroup>
</template>

<script setup lang="ts">
import { MEMBER_HOME } from "~/domain/access";
import { SCREEN_FRAME, useVisibleScreen } from "bmo-ui/app/domain/screen";

// A member's shell (docs/web.md, 「Access」): the rooms they may read and their own menu, and
// nothing that reads a route opened to the administrator alone. It frames the screen as the
// administrator's shell does, so a member's page stands the same way.
const shell = SCREEN_FRAME;

useVisibleScreen();

const open = ref(false);
const sidebarFolded = ref(false);
</script>
