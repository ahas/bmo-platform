<template>
  <!-- The Companions tab: one block for each companion, in the agent list's order (docs/web.md,
       「Agent management screen」), and the whole tab one form, saved from its foot (「Forms」). -->
  <div class="flex flex-1 flex-col">
    <div class="mx-auto flex w-full max-w-3xl flex-col gap-10 p-4 sm:p-6">
      <ReadFailure v-if="error && companions === undefined" :failure="readFailure" :message="$t('agents.error.companions_read_failed')" :reading="status === 'pending'" @retry="refresh()" />

      <!-- A block before it arrives: its header row and the shape of its first two groups. -->
      <ContentSkeleton v-else-if="companions === undefined" :rows="1" :label="$t('agents.status.companions_loading')">
        <div class="flex flex-col gap-6">
          <div class="flex min-h-8 items-center gap-2">
            <SkeletonBar class="size-5 shrink-0 rounded-sm" />
            <SkeletonBar class="h-5 w-24" />
          </div>
          <SkeletonBar class="h-8 w-full rounded-md" />
          <div class="flex flex-col divide-y divide-default rounded-lg border border-default">
            <div v-for="n in 5" :key="n" class="flex min-h-10 items-center gap-2 ps-3 pe-1">
              <SkeletonBar class="size-4 shrink-0 rounded-sm" />
              <SkeletonBar class="h-4 w-1/3" />
            </div>
          </div>
        </div>
      </ContentSkeleton>

      <p v-else-if="companions.length === 0" class="text-sm text-muted">{{ $t("agents.status.no_companions") }}</p>

      <template v-else>
        <CompanionSettingsBlock
          v-for="one in companions"
          :id="settingsAnchor(`companion-${one.agent}`)"
          :key="one.agent"
          :view="one"
          @saved="replaced"
        >
          <!-- A companion the console added is removed here; one the deployment seeds stands again
               at the next start, so it offers none. -->
          <template v-if="!one.builtin" #actions>
            <UTooltip :text="$t('common.title.delete_named', { name: one.label })">
              <UButton
                color="neutral"
                variant="ghost"
                size="sm"
                icon="i-material-symbols-delete-outline"
                :aria-label="$t('common.title.delete_named', { name: one.label })"
                :loading="removing === one.agent"
                :disabled="removing !== ''"
                @click="void remove(one)"
              />
            </UTooltip>
          </template>
        </CompanionSettingsBlock>
      </template>

      <p v-if="removeFailed !== ''" role="alert" class="break-words text-sm text-error">{{ removeFailed }}</p>
    </div>

    <!-- The form's one save, at the foot of the tab while the blocks scroll under it. The band
         reaches the section's edges and its button stands on the blocks' column. -->
    <div v-if="(companions?.length ?? 0) > 0" class="sticky bottom-0 mt-auto shrink-0 border-t border-default bg-default px-4 py-3 sm:px-6">
      <div class="mx-auto flex w-full max-w-3xl justify-end">
        <UButton :label="$t('common.action.save')" :loading="form.saving.value" :disabled="!form.dirty.value || form.saving.value" @click="void form.save()" />
      </div>
    </div>

    <!-- A companion added with a room of the console's own that it answers alone; the bot checks every
         field, and a refusal stands under the field it names. -->
    <SettingsDialog v-model:open="adding" :title="$t('agents.title.companion_add')" :dirty="addDirty">
      <form id="companion-add-form" class="flex flex-col gap-4" @submit.prevent="void add()">
        <div class="grid gap-4 sm:grid-cols-2">
          <UFormField :label="$t('common.label.name')" name="name" :error="addErrorAt('name')">
            <UInput v-model="draft.name" class="w-full font-mono" :aria-label="$t('common.label.name')" />
          </UFormField>
          <UFormField :label="$t('agents.label.companion_label')" name="label" :error="addErrorAt('label')">
            <UInput v-model="draft.label" class="w-full" :aria-label="$t('agents.label.companion_label')" />
          </UFormField>
        </div>

        <div class="grid gap-4 sm:grid-cols-2">
          <UFormField :label="$t('common.label.model')" name="model" :error="addErrorAt('model') ?? addErrorAt('provider')">
            <USelect v-model="draft.model" :items="modelItems" value-key="value" class="w-full" :aria-label="$t('common.label.model')" />
          </UFormField>
          <UFormField :label="$t('agents.label.companion_worker')" name="worker" :error="addErrorAt('worker')">
            <USelect v-model="draft.worker" :items="workerItems" value-key="value" class="w-full" :aria-label="$t('agents.label.companion_worker')" />
          </UFormField>
        </div>

        <UFormField :label="$t('agents.label.companion_room')" name="room" :error="addErrorAt('room')">
          <UInput v-model="draft.room" class="w-full" :aria-label="$t('agents.label.companion_room')" />
        </UFormField>

        <p v-if="addRefusal !== null && !ADD_FIELDS.has(addRefusal.field)" role="alert" class="break-words text-sm text-error">{{ addRefusal.text }}</p>
      </form>

      <template #footer="{ close }">
        <UButton :label="$t('common.action.cancel')" color="neutral" variant="ghost" :disabled="addBusy" @click="close()" />
        <UButton type="submit" form="companion-add-form" :label="$t('common.action.add')" :loading="addBusy" :disabled="addBusy || !addDirty" />
      </template>
    </SettingsDialog>
  </div>
</template>

<script setup lang="ts">
import { settingsAnchor } from "~/domain/agent-settings";
import { anyRunning } from "~/domain/catalogue-models";
import { failureText } from "bmo-ui/app/domain/errors";
import { useReadFailure } from "~/domain/events";
import { useFormSave, useLeaveGuard } from "bmo-ui/app/domain/form-save";
import { useIsland } from "bmo-ui/app/domain/island";
import { answeringModels, REGISTRY_PROVIDER, registrationName } from "~/domain/registry";

import type { CompanionAdded, CompanionView, ModelPage } from "#shared/agency";

interface Props {
  // The workers a companion's items may go to, as the page lists them.
  workers?: { name: string; label: string | null }[];
}

const props = withDefaults(defineProps<Props>(), { workers: () => [] });
const { t } = useI18n();
const island = useIsland(t);
const { data: companions, error, refresh, status } = useFetch<CompanionView[]>("/api/companions", { key: "companions", lazy: true, server: false });
const readFailure = useReadFailure({ failed: () => !!error.value && companions.value === undefined, reading: () => status.value === "pending", reread: () => void refresh() });

const form = useFormSave(t);

useLeaveGuard(form.dirty, t);

// While a model choice for an engine runs, the blocks are read again every few seconds, so its step
// and its end show where it was started.
const POLL_MS = 3_000;
let poll: ReturnType<typeof setInterval> | null = null;

watch(() => anyRunning((companions.value ?? []).flatMap((one) => (one.modelUse === null ? [] : [one.modelUse]))), (running) => {
  if (running && poll === null) poll = setInterval(() => void refresh(), POLL_MS);
  if (!running && poll !== null) {
    clearInterval(poll);
    poll = null;
  }
}, { immediate: true });

onBeforeUnmount(() => {
  if (poll !== null) clearInterval(poll);
});

// A block stands as the bot answered its save; the list is replaced whole, since the fetched value
// is not watched deeply.
function replaced(saved: CompanionView): void {
  companions.value = (companions.value ?? []).map((one) => (one.agent === saved.agent ? saved : one));
}

// The fields the dialog shows a refusal under; any other stands at its end.
const ADD_FIELDS = new Set(["name", "label", "model", "provider", "worker", "room"]);

function emptyDraft(): { name: string; label: string; model: string; worker: string; room: string } {
  return { name: "", label: "", model: "", worker: "", room: "" };
}

const adding = ref(false);
const addBusy = ref(false);
const draft = ref(emptyDraft());
const addDirty = computed(() => JSON.stringify(draft.value) !== JSON.stringify(emptyDraft()));
const addRefusal = ref<{ field: string; text: string } | null>(null);

// The registrations whose addresses answer, which a companion may be pinned to; the page reads the
// same list, so this shares its reading of it.
const { data: models } = useFetch<ModelPage>("/api/models", { key: "models", lazy: true, server: false });
const modelItems = computed(() => answeringModels(models.value?.items ?? []).map((one) => {
  const name = registrationName(one.key);

  return { label: name, value: name };
}));
const workerItems = computed(() => props.workers.map((one) => ({ label: one.label ?? one.name, value: one.name })));

function addErrorAt(field: string): string | undefined {
  return addRefusal.value?.field === field ? addRefusal.value.text : undefined;
}

function openAdd(): void {
  draft.value = emptyDraft();
  addRefusal.value = null;
  adding.value = true;
}

function refusalOf(error: unknown): { field: string; text: string } {
  const field = (error as { data?: { data?: { field?: unknown } } } | null)?.data?.data?.field;

  return { field: typeof field === "string" ? field : "", text: failureText(error, t("common.error.save_failed")) };
}

// An add whose name a removed companion's console room still stood under takes that room back with its record, and
// the notice says so and leads to it.
async function add(): Promise<void> {
  addBusy.value = true;
  addRefusal.value = null;

  try {
    const added = await $fetch<CompanionAdded>("/api/companions/add", { method: "POST", body: { ...draft.value, provider: REGISTRY_PROVIDER } });

    companions.value = added.companions;

    if (added.room.takenBack) {
      island.say({ title: t("agents.status.companion_room_taken_back", { name: draft.value.label, title: draft.value.room }), to: `/rooms/${added.room.chatId}` });
    }

    draft.value = emptyDraft();
    adding.value = false;
  } catch (error) {
    addRefusal.value = refusalOf(error);
  } finally {
    addBusy.value = false;
  }
}

const removing = ref("");
const removeFailed = ref("");

// A removal is asked first, naming what goes with the companion — its settings, its sessions and what it
// remembers — and that the rooms it answered stay with their messages and documents. What the bot refuses it
// for stands under the blocks.
async function remove(companion: CompanionView): Promise<void> {
  const taken = await island.ask({
    title: t("agents.message.companion_delete_confirm", { name: companion.label }),
    description: t("agents.message.companion_delete_takes"),
    confirm: t("common.action.delete"),
    tone: "warning"
  });

  if (!taken) return;

  removing.value = companion.agent;
  removeFailed.value = "";

  try {
    companions.value = await $fetch<CompanionView[]>("/api/companions/remove", { method: "POST", body: { agent: companion.agent } });
  } catch (error) {
    removeFailed.value = failureText(error, t("common.error.save_failed"));
  } finally {
    removing.value = "";
  }
}

defineExpose({ refresh, openAdd });
</script>
