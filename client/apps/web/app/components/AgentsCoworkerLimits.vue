<template>
  <!-- How many co-workers of each tier may be alive on each provider: a co-worker tier to a row, a
       provider to a column, only those the bot sends: turned on, and lending co-workers. A cell's
       dropdown sets that one limit. -->
  <section class="flex min-w-0 flex-col" aria-labelledby="coworker-limits-title">
    <div class="flex min-h-12 items-center gap-2 border-y border-default px-4 py-2">
      <h2 id="coworker-limits-title" class="text-base font-semibold text-highlighted">{{ $t("agents.title.coworker_limits") }}</h2>
      <UButton
        v-if="settings !== null"
        :label="$t('agents.title.limits_reset')"
        size="sm"
        color="neutral"
        variant="outline"
        class="ms-auto"
        :disabled="stale || resetting"
        @click="confirmingReset = true"
      />
    </div>

    <DataTableSkeleton v-if="settings === null && readFailed === ''" :headers="tableHeaders(columns)" :rows="4" :label="$t('agents.status.limits_loading')" />

    <ReadFailure class="mx-4 my-4 w-auto" :failure="readFailure" :message="readFailed" :reading="reading" @retry="reload()" />

    <template v-if="settings !== null">
      <UAlert
        v-if="stale"
        class="mx-4 my-4 w-auto"
        color="warning"
        variant="subtle"
        :title="$t('agents.title.read_at', { time: clockText(readAt) })"
        :description="readFailed === '' ? $t('agents.status.refreshing_locked') : $t('agents.status.save_locked', { reason: readFailed })"
        :actions="[{ label: $t('common.action.refresh'), color: 'neutral', variant: 'outline', loading: reading, onClick: () => reload() }]"
      />

      <DataTable :data="rows" :columns="columns">
        <template #tier-cell="{ row }">
          <span :id="settingsAnchor(`pool-${row.original.tier}`)" :class="CELL_TEXT" class="ps-4 font-medium text-highlighted">
            <SearchText :text="row.original.tier" :query="props.query" />
          </span>
        </template>

        <template v-for="name in providerColumns" :key="name" #[`${name}-header`]>
          {{ labelOf(name) }}
        </template>

        <template v-for="name in providerColumns" :key="name" #[`${name}-cell`]="{ row }">
          <SettingsInputCell
            v-if="placeOf(row.original.tier, name) !== null"
            :value="placeOf(row.original.tier, name)!.capacity"
            :label="$t('agents.aria.limit_of', { mode: labelOf(name), tier: row.original.tier })"
            :min="0"
            :max="settings.pool.capacityLimit"
            :step="1"
            :disabled="stale"
            :check="(text: string) => poolCapacityFault($t, numberOf(text), settings!.pool.capacityLimit)"
            :save="(text: string) => ask({ action: 'set-pool-limit', provider: name, tier: row.original.tier, capacity: numberOf(text) })"
          >
            <span class="flex items-center gap-2">
              <UIcon
                :name="placeOf(row.original.tier, name)!.capacity === 0 ? 'i-material-symbols-block' : 'i-material-symbols-check-circle-outline'"
                class="size-4"
                :class="placeOf(row.original.tier, name)!.capacity === 0 ? 'text-warning' : 'text-success'"
              />
              <span class="font-medium tabular-nums text-highlighted">{{ placeOf(row.original.tier, name)!.capacity.toLocaleString("ko-KR") }}</span>
            </span>
            <span class="tabular-nums text-muted">{{ countsText(placeOf(row.original.tier, name)!) }}</span>

            <template #extra>
              <dl class="pair-list gap-x-4 gap-y-1 text-sm">
                <dt class="text-muted">{{ $t("common.label.model") }}</dt>
                <dd class="break-words text-toned">{{ tierModelText($t, placeOf(row.original.tier, name)!) }}</dd>
                <dt class="text-muted">{{ $t("agents.label.default_value") }}</dt>
                <dd class="tabular-nums text-toned">{{ defaultText($t, placeOf(row.original.tier, name)!) }}</dd>
                <dt class="text-muted">{{ $t("agents.label.role") }}</dt>
                <dd class="break-words text-toned">{{ placeOf(row.original.tier, name)!.roles.join(", ") || "—" }}</dd>
                <dt class="text-muted">{{ $t("agents.label.in_use") }}</dt>
                <dd class="break-words tabular-nums text-toned">{{ usageText($t, placeOf(row.original.tier, name)!) }}</dd>
                <template v-if="placeOf(row.original.tier, name)!.reservations.length > 0">
                  <dt class="text-muted">{{ $t("agents.label.reserved") }}</dt>
                  <dd class="flex min-w-0 flex-col gap-1 text-toned">
                    <span class="[overflow-wrap:anywhere]">{{ pingText($t, placeOf(row.original.tier, name)!.lastPing) }}</span>
                    <span
                      v-for="reservation in placeOf(row.original.tier, name)!.reservations"
                      :key="`${reservation.owner}:${reservation.role}:${reservation.createdAt}`"
                      class="[overflow-wrap:anywhere]"
                    >
                      {{ reservationText($t, reservation) }}
                    </span>
                  </dd>
                </template>
              </dl>
            </template>
          </SettingsInputCell>
          <span v-else :class="CELL_TEXT" class="text-muted">—</span>
        </template>
      </DataTable>

      <p v-if="resetFailed !== ''" role="alert" class="break-words px-4 py-2 text-sm text-error">{{ resetFailed }}</p>

      <SettingsDialog v-model:open="confirmingReset" :title="$t('agents.title.limits_reset')">
        <p class="text-sm text-toned">{{ defaultsText(settings.pool.defaults) }}</p>

        <template #footer>
          <UButton :label="$t('common.action.cancel')" color="neutral" variant="ghost" :disabled="resetting" @click="confirmingReset = false" />
          <UButton :label="$t('agents.action.reset')" color="error" :loading="resetting" :disabled="stale || resetting" @click="reset" />
        </template>
      </SettingsDialog>
    </template>
  </section>
</template>

<script setup lang="ts">
import { tableHeaders } from "bmo-ui/app/domain/column-widths";
import { poolCapacityFault } from "#shared/research";
import { CELL_TEXT, PROVIDER_COLUMNS, settingsAnchor } from "~/domain/agent-settings";
import { clockText, defaultsText, defaultText, isStale, pingText, reservationText, tierModelText, usageText } from "~/domain/research";
import { useReadFailure } from "~/domain/events";
import { usePolledResearchSettings } from "~/domain/research-settings";
import { modeLabel } from "~/domain/status";

import type { TierProvider } from "#shared/agency";
import type { CoworkerTier } from "#shared/agency";
import type { PoolTierView } from "#shared/research";
import type { TableColumn } from "@nuxt/ui";

const props = withDefaults(defineProps<{ query?: string }>(), { query: "" });

const { t } = useI18n();
const { settings, readFailed, reading, readAt, reload, ask, now } = usePolledResearchSettings(t);
const readFailure = useReadFailure({ failed: () => settings.value === null && readFailed.value !== "", reading: () => reading.value, reread: () => void reload() });

const stale = computed(() => isStale(readAt.value, readFailed.value !== "", now.value));
const confirmingReset = ref(false);
const resetting = ref(false);
const resetFailed = ref("");

const rows = computed(() => (settings.value?.pool.coworkerTiers ?? []).map((tier) => ({ tier })));

// The providers the pool shows, in the order the bot sends them; until the settings come, every
// provider holds a column of the skeleton.
const providerColumns = computed<TierProvider[]>(() => settings.value === null
  ? [...PROVIDER_COLUMNS]
  : [...new Set(settings.value.pool.tiers.map((place) => place.provider))]);

const columns = computed<TableColumn<{ tier: CoworkerTier }>[]>(() => [
  { accessorKey: "tier", header: t("agents.label.tier"), meta: { class: { th: "w-28" } } },
  ...providerColumns.value.map((name) => ({ id: name, header: labelOf(name) }))
]);

function placeOf(tier: CoworkerTier, provider: TierProvider): PoolTierView | null {
  return settings.value?.pool.tiers.find((place) => place.tier === tier && place.provider === provider) ?? null;
}

// A column is headed by the name its provider gives itself, once the settings have come.
function labelOf(provider: TierProvider): string {
  return settings.value?.pool.tiers.find((place) => place.provider === provider)?.label ?? modeLabel(t, provider);
}

// A number box hands back the text typed; an empty one is no number.
function numberOf(text: string): number {
  return text.trim() === "" ? Number.NaN : Number(text);
}

// What a cell says under its limit: running, held and waiting, as three short counts.
function countsText(place: PoolTierView): string {
  return t("agents.label.place_counts", {
    active: place.active.toLocaleString("ko-KR"),
    reserved: place.reserved.toLocaleString("ko-KR"),
    waiting: place.waiting.toLocaleString("ko-KR")
  });
}

async function reset(): Promise<void> {
  if (stale.value || resetting.value) return;

  resetting.value = true;
  resetFailed.value = "";

  const refused = await ask({ action: "reset-pool-limits" });

  resetting.value = false;

  if (refused === null) {
    confirmingReset.value = false;
  } else {
    resetFailed.value = refused;
  }
}
</script>
