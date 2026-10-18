<template>
  <UDashboardGroup :style="shell">
    <UDashboardSidebar
      id="default"
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
        <!-- The console's own mark rather than a stock icon: the tab's icon in the console's own
             colours, so the picture on a tab, on a home screen and at the top of the sidebar are read
             as one. It is pressed like every row under it, so it wears their box — the same height and
             the same inner inset, which puts the mark on the column the sidebar's icons stand on and
             answers a hover and a press in their colours. -->
        <NuxtLink
          v-if="!collapsed"
          to="/"
          class="flex items-center gap-2 rounded-md px-2.5 py-1 transition-colors hover:bg-elevated active:bg-accented"
        >
          <BmoMark class="size-5 shrink-0" />
          <span class="font-bold text-highlighted">{{ $t("app.label.name") }}</span>
        </NuxtLink>

        <!-- Collapsing is a desktop affair; the drawer below lg is closed by tapping away. -->
        <UTooltip :text="collapsed ? $t('common.action.expand') : $t('common.action.collapse')" :content="{ side: 'right' }">
          <UDashboardSidebarCollapse class="ms-auto hidden lg:flex" />
        </UTooltip>
      </template>

      <template #default="{ collapsed }">
        <UContentSearchButton :collapsed="collapsed" :tooltip="collapsed" class="shrink-0" />
        <!-- A folded row is its icon alone, and its mark is drawn over that icon rather than given a
             cell of its own, so the folded column keeps one centre. The menu is told which rows
             wear the mark that way, which is why the fold is read out here. Folded, every row names
             itself in a tooltip; spread, its name stands and no tooltip repeats it. -->
        <UNavigationMenu :items="namedWhenFolded(screenEntries, collapsed)" :collapsed="collapsed" tooltip orientation="vertical" />
      </template>

      <template #footer="{ collapsed }">
        <div class="flex w-full flex-col gap-1">
          <!-- Whether the receiver's stream is carrying anything. Nothing is said while it is, or
               before it has first spoken; a break the receiver announced reads as a restart. -->
          <UTooltip v-if="streamNotice !== null" :text="streamNotice.label" :disabled="!collapsed" :content="{ side: 'right' }">
            <div
              role="status"
              class="flex min-h-8 items-center gap-2 rounded-md px-2.5 text-sm"
              :class="[streamNotice.tone, collapsed ? 'justify-center' : '']"
            >
              <UIcon :name="streamNotice.icon" class="size-5 shrink-0" :class="streamNotice.spin ? 'animate-spin' : ''" />
              <span v-if="!collapsed" class="truncate">{{ streamNotice.label }}</span>
            </div>
          </UTooltip>
          <UserMenu :collapsed="collapsed" />
        </div>
      </template>
    </UDashboardSidebar>

    <div class="flex min-w-0 flex-1 flex-col">
      <div class="flex min-h-0 min-w-0 flex-1">
        <div class="relative flex min-h-0 min-w-0 flex-1">
          <slot />
        </div>
      </div>
    </div>
    <UContentSearch
      v-model:search-term="searchTerm"
      :groups="searchGroups"
      :color-mode="false"
      shortcut="meta_k"
      :placeholder="$t('screens.placeholder.find')"
      :title="$t('screens.label.search')"
      :description="$t('screens.label.find_description')"
    />
  </UDashboardGroup>
</template>

<script setup lang="ts">
import { useBotEvents } from "~/domain/events";
import { jumpGroups, screenGroup } from "~/domain/jump";
import { SCREEN_FRAME, useVisibleScreen } from "bmo-ui/app/domain/screen";
import { namedWhenFolded, screenItems } from "~/domain/screens";

import type { NavigationMenuItem } from "@nuxt/ui";
import type { JumpGroup } from "~/domain/jump";

const route = useRoute();

// The shell of the console fills what is on the screen rather than what the window says it is.
// The two are the same until something is laid over the page — a virtual keyboard is, on a phone
// — and a shell pinned to a window whose bottom is behind the keys puts the box being typed in there
// with it. What is visible is kept in the two custom properties, so the shell reads them and
// anything else that has to fit on the screen reads the same pair.
const shell = SCREEN_FRAME;

useVisibleScreen();

const open = ref(false);
const { t } = useI18n();

// Whether the sidebar stands folded. The menu's rows are built from it: a folded row is its icon
// alone, and a mark it carries is drawn over that icon rather than given a cell of its own.
const sidebarFolded = ref(false);
const screenEntries = computed<NavigationMenuItem[]>(() => screenItems(t, route.path, undefined, sidebarFolded.value));

// The console search: what this sidebar lists, and the shortcut that opens it from anywhere.
// Ctrl+K is ⌘K on a Mac; the shortcut layer reads `meta` as Ctrl elsewhere.
const searchTerm = ref("");
const jumpSources = computed<JumpGroup[]>(() => [screenGroup(t)]);
const searchGroups = computed(() => jumpGroups(t, jumpSources.value, searchTerm.value));

const stream = useBotEvents(() => {});

interface StreamNotice {
  label: string;
  icon: string;
  tone: string;
  spin: boolean;
}

// The one line the sidebar keeps about the stream, standing only while it is not connected. A
// restart the receiver announced is expected and passes on its own, so it is said quietly; any other
// break means what the screens show may be stale, so it is said in the error colour.
const streamNotice = computed<StreamNotice | null>(() => {
  if (stream.restarting.value) {
    return { label: t("screens.status.restarting"), icon: "i-material-symbols-restart-alt", tone: "text-muted", spin: true };
  }

  if (stream.dropped.value) {
    return { label: t("screens.status.disconnected"), icon: "i-material-symbols-link-off", tone: "text-error", spin: false };
  }

  return null;
});
</script>
