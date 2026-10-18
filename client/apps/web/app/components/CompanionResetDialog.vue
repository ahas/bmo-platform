<template>
  <!-- A companion's reset from a room it answers in (docs/web.md, 「Room screen」): the
       two ranges as checkboxes saying in fixed words what each takes, nothing read before the reset button is
       pressed, and what it took shown once it has; with neither chosen this room's fresh session
       alone, the session it ends read first. -->
  <SettingsDialog v-model:open="open" :title="$t('room.title.companion_reset', { name: props.label })" :description="props.agent">
    <template v-if="done === null">
      <div class="flex flex-col gap-3" role="group" :aria-label="$t('room.title.companion_reset', { name: props.label })">
        <UCheckbox v-model="conversation" :label="$t('room.label.reset_conversation')" :description="$t('room.label.reset_conversation_note')" :disabled="busy" />
        <UCheckbox v-model="person" :label="$t('room.label.reset_person')" :description="$t('room.label.reset_person_note')" :disabled="busy" />
      </div>

      <!-- Neither range: the room's session that starts afresh, or that the room holds none. -->
      <div v-if="sessionOnly" class="flex flex-col gap-1 text-sm text-toned">
        <p>{{ $t("room.message.session_only") }}</p>
        <p v-if="sessionFailed !== ''" role="alert" class="break-words text-error">{{ sessionFailed }}</p>
        <ContentSkeleton v-else-if="session === undefined" :label="$t('agents.status.sessions_loading')">
          <div class="flex min-h-5 items-center"><SkeletonBar class="h-4 w-64 max-w-full" /></div>
        </ContentSkeleton>
        <p v-else>{{ session === null ? $t("room.message.no_session") : sessionClears($t, session) }}</p>
      </div>
    </template>

    <!-- What the reset took: one line to a kind, its figure first. The rooms are the conversation
         range's alone. -->
    <template v-else>
      <p class="text-sm text-toned">{{ $t("room.message.reset_done") }}</p>
      <dl class="pair-list gap-x-4 gap-y-1 text-sm">
        <template v-if="done.ranges.includes('conversation')">
          <dt class="text-muted">{{ $t("room.label.reset_rooms_alone") }}</dt>
          <dd class="break-words text-toned">{{ roomsText(done.alone) }}</dd>
          <dt class="text-muted">{{ $t("room.label.reset_rooms_shared") }}</dt>
          <dd class="break-words text-toned">{{ roomsText(done.shared) }}</dd>
        </template>
        <dt class="text-muted">{{ $t("room.label.reset_rows") }}</dt>
        <dd class="text-toned">
          <span class="tabular-nums">{{ countText(companionResetTotal(done)) }}</span>
          <span v-if="tables !== ''" class="block break-words font-mono text-muted">{{ tables }}</span>
        </dd>
        <dt class="text-muted">{{ $t("room.label.reset_files") }}</dt>
        <dd class="text-toned">
          <span class="tabular-nums">{{ countText(done.files.length) }}</span>
          <span v-for="path in done.files" :key="path" class="block whitespace-nowrap font-mono text-muted">{{ path }}</span>
        </dd>
        <dt class="text-muted">{{ $t("room.label.reset_copies") }}</dt>
        <dd class="text-toned">
          <span class="tabular-nums">{{ countText(done.copies.length) }}</span>
          <span v-for="copy in done.copies" :key="copy.path" class="block whitespace-nowrap font-mono text-muted">
            <span class="tabular-nums">{{ countText(copy.rows ?? 0) }}</span> · {{ copy.path }}<template v-if="copy.failure !== null"> · {{ copy.failure }}</template>
          </span>
        </dd>
        <template v-if="done.writerRuns.length > 0">
          <dt class="text-muted">{{ $t("room.label.reset_writer") }}</dt>
          <dd class="break-words font-mono text-toned">{{ done.writerRuns.join(", ") }}</dd>
        </template>
        <template v-if="(done.failures ?? []).length > 0">
          <dt class="text-muted">{{ $t("room.label.reset_failures") }}</dt>
          <dd class="text-error">
            <span v-for="one in done.failures" :key="one" class="block whitespace-nowrap font-mono">{{ one }}</span>
          </dd>
        </template>
      </dl>
    </template>

    <p v-if="failed !== ''" role="alert" class="break-words text-sm text-error">{{ failed }}</p>

    <template #footer="{ close }">
      <UButton v-if="done !== null" :label="$t('common.action.close')" color="neutral" variant="ghost" @click="close()" />
      <template v-else>
        <UButton :label="$t('common.action.cancel')" color="neutral" variant="ghost" :disabled="busy" @click="close()" />
        <UButton
          v-if="sessionOnly"
          color="warning"
          :label="$t('room.action.session_only')"
          :loading="busy"
          :disabled="busy || !session"
          @click="void startSession()"
        />
        <UButton v-else color="error" :label="$t('room.action.reset_run')" :loading="busy" :disabled="busy" @click="void reset()" />
      </template>
    </template>
  </SettingsDialog>
</template>

<script setup lang="ts">
import { companionResetTotal } from "#shared/agency";
import { failureText } from "bmo-ui/app/domain/errors";
import { useIsland } from "bmo-ui/app/domain/island";
import { countText } from "~/domain/numbers";
import { sessionClears } from "~/domain/sessions";

import type { CompanionResetAnswer, CompanionResetRoom } from "#shared/agency";
import type { SessionView } from "#shared/agency";

interface Props {
  agent: string;
  label: string;
  // The room it was opened from, whose session alone starts afresh where no range is chosen.
  chatId: number;
}

const props = defineProps<Props>();
const open = defineModel<boolean>("open", { required: true });

const { t } = useI18n();
const island = useIsland(t);

const conversation = ref(false);
const person = ref(false);
// What a reset took, once it has gone through.
const done = ref<CompanionResetAnswer | null>(null);
// The room's session of the companion: undefined until read, null where the room holds none.
const session = ref<SessionView | null | undefined>(undefined);
const sessionFailed = ref("");
const busy = ref(false);
const failed = ref("");

const sessionOnly = computed(() => !conversation.value && !person.value);

const tables = computed(() => (done.value?.rows ?? []).filter((one) => one.rows > 0).map((one) => `${one.table} ${countText(one.rows)}`).join(" · "));

function roomsText(rooms: readonly CompanionResetRoom[]): string {
  return rooms.length === 0 ? "—" : rooms.map((room) => room.title ?? String(room.chatId)).join(", ");
}

// The room's session, read once as the dialog opens: the one thing it reads before a press.
async function readSession(): Promise<void> {
  sessionFailed.value = "";

  try {
    const sessions = await $fetch<SessionView[]>("/api/sessions");

    session.value = sessions.find((one) => one.agent === props.agent && one.chatId === props.chatId) ?? null;
  } catch (error) {
    sessionFailed.value = failureText(error, t("agents.error.sessions_read_failed"));
  }
}

watch([conversation, person], () => {
  failed.value = "";
});

// Opened afresh each time, with neither range chosen and the room's session read.
watch(open, (now) => {
  if (!now) return;

  conversation.value = false;
  person.value = false;
  done.value = null;
  failed.value = "";
  session.value = undefined;
  void readSession();
}, { immediate: true });

async function reset(): Promise<void> {
  busy.value = true;
  failed.value = "";

  try {
    done.value = await $fetch<CompanionResetAnswer>("/api/companions/reset", {
      method: "POST",
      body: { agent: props.agent, conversation: conversation.value, person: person.value },
    });
  } catch (error) {
    failed.value = failureText(error, t("common.error.save_failed"));
  } finally {
    busy.value = false;
  }
}

async function startSession(): Promise<void> {
  busy.value = true;
  failed.value = "";

  try {
    await $fetch("/api/session-reset", { method: "POST", body: { agent: props.agent, chatId: props.chatId } });
    island.say({ tone: "success", title: t("room.message.session_started") });
    open.value = false;
  } catch (error) {
    failed.value = failureText(error, t("common.error.save_failed"));
  } finally {
    busy.value = false;
  }
}
</script>
