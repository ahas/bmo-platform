<template>
  <UPopover :content="{ align: 'end' }" :ui="{ content: 'w-96 max-w-[calc(100vw-1rem)] flex flex-col gap-3 p-3' }">
    <!-- The selected mode's two limit windows, or an honest absence for OpenRouter, on every
         title bar and nothing else. One axis: a percentage is what a window is, and a count of
         tokens set beside one is read as though it were another percentage.
         A phone's bar has no room even for the two — the room's own name was being cut to fit
         them — so below that width they fold into the one mark that opens them, and the mark
         carries the louder of them in its colour: an icon standing in for a share at red says
         red. Everything they say is a press away either way, since the press is the same press
         that was always there.

         The letters here are the interface's own face, not a typewriter's: only the digits want
         a fixed width, and they get it from the figures being tabular rather than from dragging
         the Korean along into a monospace stack that has no Korean in it. -->
    <!-- On a phone it is an icon alone, named on hover like every icon button and as square as the
         bar's icon button. The login it stands for is named there too, since the folded mark has
         nowhere else to say it. -->
    <UTooltip :text="accountName === null ? $t('usage.title.remaining') : $t('usage.title.remaining_of', { name: accountName })" :disabled="!narrow">
      <button
        type="button"
        class="flex shrink-0 items-center gap-2 rounded-md p-1.5 text-sm tabular-nums transition-colors hover:bg-elevated active:bg-accented sm:shrink sm:px-2 sm:py-0.5"
        :aria-label="$t('usage.title.remaining')"
      >
        <UIcon name="i-material-symbols-speed" class="size-5 shrink-0 sm:hidden" :class="toneClass(loudest)" />

        <SkeletonStatus v-if="usageInitial" class="hidden sm:block" :label="$t('usage.status.loading')"><SkeletonBar class="h-4 w-32" /></SkeletonStatus>
        <span v-else class="hidden sm:contents">
          <span class="text-sm text-muted">{{ modeLabel($t, focusedMode) }}</span>
          <!-- Which login the two figures are of, where the agency holds more than one to tell apart.
               Two logins report the same windows, each about its own limit, so a figure with no
               login named is a figure about nobody. -->
          <template v-if="accountName !== null">
            <span class="text-muted">·</span>
            <span class="max-w-32 truncate text-sm text-muted">{{ accountName }}</span>
          </template>
          <span class="text-muted">·</span>
          <template v-if="focusedMode === 'openrouter'">
            <span class="text-muted"><span class="text-sm text-muted">{{ $t("usage.title.remaining") }}</span> —</span>
          </template>
          <template v-else>
            <span :class="toneClass(shareTone(liveWindow(session, now)))">
              <span class="text-sm text-muted">{{ $t("usage.label.window_session") }}</span> {{ shareText(liveWindow(session, now)) }}
            </span>
            <span class="text-muted">·</span>
            <span :class="toneClass(shareTone(liveWindow(weekly, now)))">
              <span class="text-sm text-muted">{{ $t("usage.label.window_weekly") }}</span> {{ shareText(liveWindow(weekly, now)) }}
            </span>
          </template>
        </span>
      </button>
    </UTooltip>

    <template #content>
      <ReadFailure :failure="usageFailure" :message="usageFailureText" :reading="usageReading" @retry="rereadAll()" />
      <ContentSkeleton v-if="usageInitial" :rows="3" listClass="flex flex-col gap-3" :label="$t('usage.status.loading')">
        <div class="flex flex-col gap-1">
          <SkeletonLine width="w-24" />
          <SkeletonLine width="w-full" />
        </div>
      </ContentSkeleton>
      <div v-if="!usageInitial && (usageStatus === 'success' || usageLoaded)" class="flex flex-col gap-3">
        <div class="flex flex-col gap-2">
          <!-- Where these figures are settled: which logins the road walks, in what order and
               which of them are on. A mark standing on every screen leads to the page it is
               about, and this one is about the accounts. -->
          <div class="flex items-center justify-between gap-2">
            <p class="text-sm font-medium text-muted">{{ $t("usage.title.remaining") }}</p>
            <UButton
              to="/agents?tab=general"
              size="sm"
              color="neutral"
              variant="link"
              trailing-icon="i-material-symbols-chevron-right"
              :label="$t('usage.label.claude_accounts')"
            />
          </div>

          <!-- Every registered Claude account as a group of its own, in the order the road walks
               them, whichever of them is serving: each login keeps its own limit, so each keeps
               its own figures, and the one runs go out on now is marked. -->
          <div v-for="group in claudeGroups" :key="group.id" class="flex flex-col gap-1">
            <div class="flex min-w-0 items-center gap-2 text-sm">
              <span class="min-w-0 truncate font-medium text-muted">{{ modeLabel($t, "claude-code") }} · {{ group.name }}</span>
              <UBadge v-if="group.inUse" size="sm" color="primary" variant="subtle" :label="$t('usage.label.in_use')" class="shrink-0" />
            </div>

            <div v-for="window in group.windows" :key="window.name" class="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm">
              <span class="min-w-0 flex-1 basis-24 break-words text-dimmed">{{ windowLabel($t, window) }}</span>
              <span class="tabular-nums" :class="toneClass(shareTone(liveWindow(window, now)))">{{ shareText(liveWindow(window, now)) }}</span>
              <!-- How long is left of the window, not the hour it turns over at: the length is
                   what the figure beside it is read against. More than a day out, the day it
                   turns over on stands beside it, since that day is what tells one login's week
                   from another's. The clock face is kept on the row, for a reader who wants it. -->
              <span
                v-if="resetText($t, window, now) !== ''"
                class="ms-auto text-sm tabular-nums text-dimmed"
                :title="resetClock($t, window)"
              >
                {{ resetText($t, window, now) }}<template v-if="resetDay($t, window, now) !== ''"> · {{ resetDay($t, window, now) }}</template>
              </span>
            </div>

            <p v-if="group.windows.length === 0" class="text-sm text-dimmed">{{ $t("usage.status.no_info") }}</p>
          </div>

          <div v-for="window in otherWindows" :key="`${window.mode}:${window.name}`" class="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm">
            <span class="min-w-0 flex-1 basis-36 break-words text-dimmed">{{ modeLabel($t, window.mode) }} · {{ windowLabel($t, window) }}</span>
            <span class="tabular-nums" :class="toneClass(shareTone(liveWindow(window, now)))">{{ shareText(liveWindow(window, now)) }}</span>
            <span
              v-if="resetText($t, window, now) !== ''"
              class="ms-auto text-sm tabular-nums text-dimmed"
              :title="resetClock($t, window)"
            >
              {{ resetText($t, window, now) }}
            </span>
          </div>

          <p v-if="claudeGroups.length === 0 && otherWindows.length === 0" class="text-sm text-dimmed">{{ $t("usage.status.no_usage_yet") }}</p>
        </div>

        <!-- A row is its name with the whole of the traffic beside it, and under that the
             picture of what the traffic divided into: one track cut into the four kinds, with the
             parts named along the line below it. None of the four is hidden behind the total —
             the total alone would be the cached prefix read again on every resumed turn as much
             as anything else, and it answers how much went past rather than what was bought.
             Under the rows, where any of them was written before the kinds were kept apart,
             stands what that leaves unaccounted. -->
        <div v-if="gates.length > 0" class="flex flex-col gap-2">
          <p class="text-sm font-medium text-muted">{{ $t("usage.title.weekly_by_gate") }}</p>

          <div v-for="gate in gates" :key="gate.agent" class="flex flex-col">
            <div class="flex items-center gap-2 text-sm">
              <span class="min-w-0 flex-1 truncate text-dimmed">{{ agentLabel($t, gate.agent) }}</span>
              <span class="tabular-nums text-muted">{{ totalText($t, gate) }}</span>
            </div>

            <UsageSplit :tally="gate" class="mt-1" />
          </div>

          <p v-if="gatesUnsplit !== ''" class="text-sm text-dimmed">{{ gatesUnsplit }}</p>
        </div>

        <!-- What each user's work cost over the week, on the Claude accounts that are theirs; there are
             rows only once the multi-user model is in force. -->
        <div v-if="usage.users.length > 0" class="flex flex-col gap-2">
          <p class="text-sm font-medium text-muted">{{ $t("usage.title.weekly_by_user") }}</p>

          <div v-for="user in usage.users" :key="user.user" class="flex flex-col">
            <div class="flex items-center gap-2 text-sm">
              <span class="min-w-0 flex-1 truncate text-dimmed">{{ user.name }}</span>
              <span class="tabular-nums text-muted">{{ totalText($t, user) }}</span>
            </div>

            <UsageSplit :tally="user" class="mt-1" />
          </div>
        </div>

        <div class="flex flex-col gap-1">
          <p class="text-sm font-medium text-muted">{{ $t("usage.title.saved_sessions") }}</p>

          <div v-for="group in sessionGroups" :key="group.name" class="flex min-w-0 flex-col gap-1 py-1">
            <p class="break-words text-sm font-medium text-muted">{{ sessionLabel(group.sessions[0]!) }}</p>
            <div
              v-for="session in group.sessions"
              :key="`${session.mode}:${session.model ?? ''}`"
              class="flex min-w-0 items-start gap-2 text-sm"
            >
              <span class="min-w-0 flex-1 break-words text-dimmed">
                {{ modeLabel($t, session.mode) }}<template v-if="session.model !== null"> · {{ modelLabel(session.model) }}</template>
              </span>
              <span class="shrink-0 tabular-nums text-muted">
                {{ session.contextTokens === null ? $t("usage.label.unmeasured") : tokenText(session.contextTokens) }}
              </span>
            </div>
          </div>

          <p v-if="sessionGroups.length === 0" class="text-sm text-dimmed">{{ $t("usage.status.no_saved_sessions") }}</p>
        </div>
      </div>
    </template>
  </UPopover>
</template>

<script setup lang="ts">
import { emptyFallbackView } from "~/domain/agent-settings";
import { useInitialRead, useLastRead } from "bmo-ui/app/domain/loading";
import { failureText } from "bmo-ui/app/domain/errors";
import { useReadFailure } from "~/domain/events";
import { useNarrow } from "bmo-ui/app/domain/screen";
import { agentLabel, chatIdOf, roomTitle } from "~/domain/agents";
import { usePolling } from "~/domain/polling";
import { modeLabel, modelLabel } from "~/domain/status";
import {
  accountGroups,
  emptyUsage,
  USAGE_REFRESH_MS,
  liveWindow,
  loudestTone,
  providerWindows,
  resetClock,
  resetDay,
  resetText,
  selectedUsageMode,
  shareText,
  shareTone,
  titleAccountLabel,
  titleWindows,
  tokenText,
  totalText,
  unsplitText,
  windowLabel
} from "~/domain/usage";

import type { AgentTokenUsage, FallbackView, RoomSummary, SessionContext, UsageView } from "#shared/types";
import type { UsageTone } from "~/domain/usage";

const { t } = useI18n();
const narrow = useNarrow();

// The figures are the bot's, written down as it runs, so the same numbers stand on every screen
// and every screen reads them from one place.
const { data, refresh, status: usageStatus, error: usageFetchError } = useFetch<UsageView>("/api/usage", {
  key: "usage",
  lazy: true,
  server: false,
  default: emptyUsage
});

// The same keyed fallback view powers the route chooser. Changing the chat agent's mode there therefore
// changes this title immediately instead of waiting for a model turn to happen.
const { data: fallbackRead, refresh: refreshFallback, status: fallbackStatus, error: fallbackError } = useFetch<FallbackView>("/api/fallback", {
  key: "fallback",
  lazy: true,
  server: false,
  // The one empty view every reader of this key shares; a default of its own would be refused as a
  // second, incompatible set of options for the same key.
  default: emptyFallbackView
});

const { data: roomList, status: roomStatus, error: roomError, refresh: refreshRooms } = useFetch<RoomSummary[]>("/api/rooms", {
  key: "rooms",
  lazy: true,
  server: false,
  default: () => []
});

const usagePending = useInitialRead(usageStatus);
const fallbackPending = useInitialRead(fallbackStatus);
const roomsPending = useInitialRead(roomStatus);
const usageError = computed(() => usageFetchError.value ?? fallbackError.value ?? roomError.value);
const usageInitial = computed(() => !usageError.value && (usagePending.value || fallbackPending.value || roomsPending.value));
const usageLoaded = ref(usageStatus.value === "success");
watch(usageStatus, value => { if (value === "success") usageLoaded.value = true; });
const usageReading = computed(() => [usageStatus, fallbackStatus, roomStatus].some(status => status.value === "pending"));
const usageFailureText = computed(() => failureText(usageError.value, t("usage.error.read_failed")));

// A failed read puts each fetch back to its empty default; what was read last stays on the screen
// through it, so a figure is never taken down for a read that did not happen.
const usageRead = useLastRead(data, usageStatus);
const fallback = useLastRead(fallbackRead, fallbackStatus);
const roomsRead = useLastRead(roomList, roomStatus);
const usage = computed<UsageView>(() => usageRead.value ?? emptyUsage());
const rooms = computed<RoomSummary[]>(() => roomsRead.value ?? []);

function rereadAll(): void {
  void Promise.all([refresh(), refreshFallback(), refreshRooms()]);
}

// Held where the title stands rather than in the popover, so the read the bot's return brings is
// made whether the popover is open or not.
const usageFailure = useReadFailure({
  failed: () => usageError.value !== undefined && usageError.value !== null,
  reading: () => usageReading.value,
  reread: rereadAll
});
const sessionGroups = computed(() => {
  const groups = new Map<string, SessionContext[]>();
  for (const session of usage.value.sessions) {
    const rows = groups.get(session.name) ?? [];
    rows.push(session);
    groups.set(session.name, rows);
  }
  return [...groups].map(([name, sessions]) => ({ name, sessions }));
});

// The Claude accounts as groups of their own, and the windows of every other road beside them.
const claudeGroups = computed(() => accountGroups(usage.value, fallback.value));
const otherWindows = computed(() => providerWindows(usage.value));

// Whose figures these are is the bot's answer: the provider the road answers on and the login it
// walks to now, not the head of the account order. A login the road has passed over is one nothing
// goes out on, and its ceiling is no news about what the account has left.
const focusedMode = computed(() => selectedUsageMode(fallback.value));
const accountName = computed(() => titleAccountLabel(fallback.value));
const focusedWindows = computed(() => titleWindows(fallback.value));
const session = computed(() => focusedWindows.value[0]);
const weekly = computed(() => focusedWindows.value[1]);

// What the folded mark reads as: the loudest of the two windows it stands for. Those two are the
// whole of the bar, so the mark stands for the whole of it.
const loudest = computed<UsageTone>(() => loudestTone([liveWindow(session.value, now.value), liveWindow(weekly.value, now.value)]));

// The second the lengths are counted from. It moves once a minute because that is the smallest
// unit those lengths are said in: a clock of its own ticking every second would redraw the rows
// sixty times for every word that changed.
const now = ref(Math.floor(Date.now() / 1000));

usePolling(() => {
  now.value = Math.floor(Date.now() / 1000);
  void Promise.all([refresh({ dedupe: "defer" }), refreshFallback({ dedupe: "defer" })]);
}, USAGE_REFRESH_MS);

// The gates alone, out of every agent's weekly tally: the work keys the agency's roads name as gates.
const gateKeys = computed<ReadonlySet<string>>(() => new Set((fallbackRead.value?.profiles ?? []).filter((one) => one.kind === "gate").map((one) => one.key)));
const gates = computed<AgentTokenUsage[]>(() => usage.value.agents.filter((agent) => gateKeys.value.has(agent.agent)));

// What the list of rows leaves unaccounted, worked out once for the whole list rather than per
// row: the rows written before the kinds were kept apart are the same omission whichever row they
// sat on, and a note on every row would say it a dozen times.
const gatesUnsplit = computed<string>(() => unsplitText(t, gates.value));


function sessionLabel(session: SessionContext): string {
  const chatId = chatIdOf(session.name);

  return chatId === null ? agentLabel(t, session.name) : roomTitle(t, rooms.value, chatId);
}

function toneClass(tone: UsageTone): string {
  if (tone === "error") {
    return "text-error";
  }

  return tone === "warning" ? "text-warning" : "text-muted";
}
</script>
