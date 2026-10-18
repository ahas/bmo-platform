<template>
  <!-- The searchers, read together and kept fresh while their tab stands: a profile to a row, a
       setting to a column, each cell editing its own value. -->
  <div class="flex min-w-0 flex-col">
    <DataTableSkeleton v-if="settings === null && readFailed === ''" :headers="tableHeaders(columns)" :rows="6" :label="$t('research.status.settings_loading')" />

    <ReadFailure class="mx-4 my-4 w-auto" :failure="readFailure" :message="readFailed" :reading="reading" @retry="reload()" />

    <template v-if="settings !== null">
      <!-- Values read before a failed reread stay on screen, named as old, and nothing is saved
           over them until a reread succeeds: a save made from them could undo a newer change. -->
      <UAlert
        v-if="stale"
        class="mx-4 my-4 w-auto"
        color="warning"
        variant="subtle"
        :title="$t('research.title.read_at', { time: clockText(readAt) })"
        :description="readFailed === '' ? $t('research.status.refreshing_locked') : $t('research.status.save_locked', { reason: readFailed })"
        :actions="[{ label: $t('common.action.refresh'), color: 'neutral', variant: 'outline', loading: reading, onClick: () => reload() }]"
      />

      <DataTable :data="settings.profiles" :columns="columns">
        <template #key-cell="{ row }">
          <span :id="settingsAnchor(`research-${row.original.key}`)" :class="CELL_TEXT" class="ps-4">
            <span class="flex min-w-0 items-center gap-2">
              <UIcon
                :name="row.original.error === null ? 'i-material-symbols-check-circle-outline' : 'i-material-symbols-error-outline'"
                class="size-4 shrink-0"
                :class="row.original.error === null ? 'text-success' : 'text-error'"
              />
              <SearchText class="font-medium text-highlighted" :text="profileLabel($t, row.original.key)" :query="props.query" />
            </span>
            <SearchText class="font-mono text-muted" :text="row.original.key" :query="props.query" />
          </span>
        </template>

        <template #tier-cell="{ row }">
          <SettingsChoiceCell
            :value="row.original.tier"
            :items="tierItems"
            :label="$t('research.aria.profile_field', { profile: profileLabel($t, row.original.key), field: $t(HEADERS.tier) })"
            :disabled="stale"
            :save="(tier) => assignTier(row.original.key, String(tier))"
          >
            <SearchText class="text-highlighted" :text="row.original.tier ?? $t('research.label.no_assignment')" :query="props.query" />
            <SearchText :class="row.original.error === null ? 'text-muted' : 'text-error'" :text="tierRunText($t, row.original)" :query="props.query" />
          </SettingsChoiceCell>
        </template>

        <template #childRole-cell="{ row }">
          <SettingsChoiceCell
            :value="row.original.childRole"
            :items="roleItems(row.original)"
            :label="$t('research.aria.profile_field', { profile: profileLabel($t, row.original.key), field: $t(HEADERS.childRole) })"
            :disabled="stale || row.original.key === 'search:basic'"
            :save="(role) => saveField(row.original, 'childRole', String(role))"
          >
            <SearchText class="font-mono text-highlighted" :text="row.original.childRole" :query="props.query" />
          </SettingsChoiceCell>
        </template>

        <template #maxChildren-cell="{ row }">
          <SettingsInputCell
            :value="row.original.maxChildren"
            :label="$t('research.aria.profile_field', { profile: profileLabel($t, row.original.key), field: $t(HEADERS.maxChildren) })"
            :min="0"
            :max="settings.limits.maxChildren.max"
            :step="1"
            :disabled="stale || row.original.key === 'search:basic'"
            :check="(text: string) => fieldFault($t, settings!, row.original, 'maxChildren', wholeOf(text))"
            :save="(text: string) => saveField(row.original, 'maxChildren', wholeOf(text))"
          >
            <span class="tabular-nums text-highlighted">{{ row.original.maxChildren === 0 ? $t("common.label.none") : row.original.maxChildren.toLocaleString("ko-KR") }}</span>
            <span v-if="row.original.room !== null" class="text-muted">{{ roomLine(row.original) }}</span>

            <template v-if="row.original.room !== null" #extra>
              <p class="break-words text-sm text-muted">{{ roomText($t, row.original.room, row.original.key) }}</p>
            </template>
          </SettingsInputCell>
        </template>

        <template #waitLimitMs-cell="{ row }">
          <SettingsInputCell
            :value="secondsOf(row.original.waitLimitMs)"
            :label="$t('research.aria.profile_field_seconds', { profile: profileLabel($t, row.original.key), field: $t(HEADERS.waitLimitMs) })"
            :min="0"
            :step="1"
            :disabled="stale"
            :check="(text: string) => fieldFault($t, settings!, row.original, 'waitLimitMs', msOf(text))"
            :save="(text: string) => saveField(row.original, 'waitLimitMs', msOf(text))"
          >
            <span class="tabular-nums text-highlighted">{{ durationText($t, row.original.waitLimitMs) }}</span>

            <template #extra>
              <p class="break-words text-sm text-muted">{{ demandText($t, row.original, row.original.key) }}</p>
            </template>
          </SettingsInputCell>
        </template>

        <template #strategy-cell="{ row }">
          <SettingsChoiceCell
            :value="row.original.strategy"
            :items="row.original.strategies.map((strategy) => ({ value: strategy, label: strategyLabel($t, strategy) }))"
            :label="$t('research.aria.profile_field', { profile: profileLabel($t, row.original.key), field: $t(HEADERS.strategy) })"
            :disabled="stale"
            :save="(strategy) => saveField(row.original, 'strategy', String(strategy))"
          >
            <SearchText class="text-highlighted" :text="strategyLabel($t, row.original.strategy)" :query="props.query" />
          </SettingsChoiceCell>
        </template>

        <template v-for="field in INTERVAL_FIELDS" :key="field" #[`${field}-cell`]="{ row }">
          <SettingsInputCell
            :value="row.original[field]"
            :label="$t('research.aria.profile_field_ms', { profile: profileLabel($t, row.original.key), field: $t(HEADERS[field]) })"
            :min="0"
            :step="100"
            :disabled="stale"
            :check="(text: string) => fieldFault($t, settings!, row.original, field, wholeOf(text))"
            :save="(text: string) => saveField(row.original, field, wholeOf(text))"
          >
            <span class="tabular-nums text-highlighted">{{ row.original[field].toLocaleString("ko-KR") }}ms</span>
          </SettingsInputCell>
        </template>

        <template v-for="field in CLOCK_FIELDS" :key="field" #[`${field}-cell`]="{ row }">
          <SettingsInputCell
            v-if="hasResearchClock(row.original)"
            :value="secondsOf(row.original[field] ?? 0)"
            :label="$t('research.aria.profile_field_seconds', { profile: profileLabel($t, row.original.key), field: $t(HEADERS[field]) })"
            :min="field === 'capMs' ? 30 : 10"
            :step="1"
            :disabled="stale"
            :check="(text: string) => fieldFault($t, settings!, row.original, field, msOf(text))"
            :save="(text: string) => saveField(row.original, field, msOf(text))"
          >
            <span class="tabular-nums text-highlighted">{{ durationText($t, row.original[field] ?? 0) }}</span>
            <span v-if="field === 'capMs'" class="text-muted">{{ researchWindowText($t, row.original) }}</span>
          </SettingsInputCell>
          <span v-else :class="CELL_TEXT" class="text-muted">{{ $t("research.label.no_limit") }}</span>
        </template>

        <template #actions-cell="{ row }">
          <span class="flex min-h-12 items-center justify-end pe-2">
            <UDropdownMenu :items="menuOf(row.original)" :content="{ align: 'end' }">
              <UTooltip :text="$t('common.aria.menu_of', { name: profileLabel($t, row.original.key) })">
                <UButton color="neutral" variant="ghost" size="sm" icon="i-material-symbols-more-vert" :aria-label="$t('common.aria.menu_of', { name: profileLabel($t, row.original.key) })" />
              </UTooltip>
            </UDropdownMenu>
          </span>
        </template>
      </DataTable>

      <EngineHealthList :settings="settings" :query="props.query" />

      <SettingsDialog v-if="resetKey !== null" v-model:open="resetting" :title="$t('research.title.reset_named', { name: profileLabel($t, resetKey) })">
        <p class="text-sm text-toned">{{ $t("research.message.reset_confirm") }}</p>
        <p v-if="resetFailed !== ''" role="alert" class="break-words text-sm text-error">{{ resetFailed }}</p>

        <template #footer>
          <UButton :label="$t('common.action.cancel')" color="neutral" variant="ghost" :disabled="resetBusy" @click="resetting = false" />
          <UButton :label="$t('research.action.reset')" color="error" :loading="resetBusy" :disabled="stale || resetBusy" @click="reset" />
        </template>
      </SettingsDialog>
    </template>
  </div>
</template>

<script setup lang="ts">
import { tableHeaders } from "bmo-ui/app/domain/column-widths";
import { CELL_TEXT, settingsAnchor } from "~/domain/agent-settings";
import { failureText } from "bmo-ui/app/domain/errors";
import {
  clockText,
  demandText,
  durationText,
  fieldFault,
  hasResearchClock,
  isStale,
  msOf,
  pooledRoles,
  profileLabel,
  researchWindowText,
  roomText,
  secondsOf,
  strategyLabel,
  tierRunText,
  wholeOf,
  withField
} from "~/domain/research";
import { useReadFailure } from "~/domain/events";
import { usePolledResearchSettings } from "~/domain/research-settings";

import type { CellChoice } from "~/domain/agent-settings";
import type { ResearchField, ResearchKey, ResearchProfileValues } from "#shared/research";
import type { ResearchProfileView } from "#shared/research";
import type { TierSettingsView } from "#shared/agency";
import type { DropdownMenuItem, TableColumn } from "@nuxt/ui";

const props = withDefaults(defineProps<{ query?: string }>(), { query: "" });

const INTERVAL_FIELDS = ["hostIntervalMs", "engineIntervalMs"] as const;
const CLOCK_FIELDS = ["capMs", "synthesisReserveMs"] as const;

// Each column's pack key, by the field it shows: the field names the column, the key its words.
const HEADERS = {
  key: "research.label.header_key",
  tier: "research.label.header_tier",
  childRole: "research.label.header_child_role",
  maxChildren: "research.label.header_max_children",
  waitLimitMs: "research.label.header_wait_limit",
  strategy: "research.label.header_strategy",
  hostIntervalMs: "research.label.header_host_interval",
  engineIntervalMs: "research.label.header_engine_interval",
  capMs: "research.label.header_cap",
  synthesisReserveMs: "research.label.header_synthesis_reserve"
} as const;

const { t } = useI18n();

const columns: TableColumn<ResearchProfileView>[] = [
  ...(Object.keys(HEADERS) as (keyof typeof HEADERS)[]).map((id) => ({ id, header: t(HEADERS[id]) })),
  { id: "actions", header: "", meta: { class: { th: "w-12" } } }
];
const { settings, readFailed, reading, readAt, reload, ask, now } = usePolledResearchSettings(t);
const readFailure = useReadFailure({ failed: () => settings.value === null && readFailed.value !== "", reading: () => reading.value, reread: () => void reload() });
const { data: tiers, refresh: rereadTiers } = useFetch<TierSettingsView>("/api/model-tiers", { key: "model-tiers", lazy: true, server: false });

const stale = computed(() => isStale(readAt.value, readFailed.value !== "", now.value));

const tierItems = computed<CellChoice[]>(() => (tiers.value?.tiers ?? []).map((tier) => ({ value: tier, label: tier })));

function roleItems(profile: ResearchProfileView): CellChoice[] {
  const lent = pooledRoles(settings.value!).map((role) => ({ value: role, label: role }));

  return lent.some((role) => role.value === profile.childRole) ? lent : [{ value: profile.childRole, label: t("research.label.role_off_in_pool", { role: profile.childRole }), disabled: true }, ...lent];
}

// The branch place in one short line under the count; the dropdown carries the whole sentence.
function roomLine(profile: ResearchProfileView): string {
  const room = profile.room;

  if (room === null || room.capacity === null) return "";

  return `${room.tier ?? ""} ${room.active.toLocaleString("ko-KR")}/${room.capacity.toLocaleString("ko-KR")}`.trim();
}

async function saveField<F extends ResearchField>(profile: ResearchProfileView, field: F, value: ResearchProfileValues[F]): Promise<string | null> {
  const fault = fieldFault(t, settings.value!, profile, field, value);

  if (fault !== null) return fault;

  return await ask({ action: "set-profile", key: profile.key, values: withField(profile, field, value) });
}

async function assignTier(key: ResearchKey, tier: string): Promise<string | null> {
  try {
    await $fetch("/api/model-tiers", { method: "POST", body: { action: "assign", key, tier } });
  } catch (failure) {
    return failureText(failure, t("common.error.save_failed"));
  }

  await Promise.all([reload(), rereadTiers()]);

  return null;
}

const resetKey = ref<ResearchKey | null>(null);
const resetting = ref(false);
const resetBusy = ref(false);
const resetFailed = ref("");

function menuOf(profile: ResearchProfileView): DropdownMenuItem[] {
  return [
    {
      label: t("research.action.reset_to_default"),
      icon: "i-material-symbols-restart-alt",
      onSelect: () => {
        resetKey.value = profile.key;
        resetFailed.value = "";
        resetting.value = true;
      }
    }
  ];
}

async function reset(): Promise<void> {
  if (resetKey.value === null || stale.value || resetBusy.value) return;

  resetBusy.value = true;

  const refused = await ask({ action: "reset-profile", key: resetKey.value });

  resetBusy.value = false;

  if (refused === null) {
    resetting.value = false;
  } else {
    resetFailed.value = refused;
  }
}
</script>
