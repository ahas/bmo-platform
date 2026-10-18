<template>
  <!-- The registered models, every model of the deployment's own choosing, reached through the agency's gateway
       (the agency's docs/own-llm.md, 「Registered models」). A row says of each address's key only whether it is
       usable: no cell and no box here has a key to show, because none is ever sent to this screen.
       A key typed into the box goes to the bot once, with the save. -->
  <section :id="settingsAnchor('registry')" class="flex flex-col gap-3" aria-labelledby="registry-title">
    <h2 id="registry-title" class="text-lg font-semibold text-highlighted">{{ generalSectionTitle($t, "registry") }}</h2>

    <ReadFailure v-if="props.failed && props.registry === null" :failure="readFailure" :message="$t('agents.error.registry_read_failed')" :reading="props.reading" @retry="props.reread()" />

    <AgentsRegistrySkeleton v-else-if="props.registry === null" :rows="REGISTRY_WAITING" />

    <ul v-else class="flex flex-col divide-y divide-default overflow-hidden rounded-lg border border-default" :aria-label="$t('agents.title.registry')">
      <li v-for="one in props.registry.registrations" :key="one.name" :data-registration="one.name" class="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-x-2 py-2 ps-4 pe-2">
        <span class="flex min-w-0 flex-col gap-1">
          <span class="flex min-w-0 items-center gap-2">
            <SearchText class="truncate text-sm font-medium text-highlighted" :text="one.label" :query="props.query" />
            <SearchText class="truncate font-mono text-sm text-muted" :text="one.name" :query="props.query" />
          </span>
          <span data-overflow-cell class="block max-w-full overflow-x-auto whitespace-nowrap [scrollbar-width:thin]">
            <SearchText class="font-mono text-sm text-toned" :text="one.model" :query="props.query" />
          </span>
          <SearchText class="text-sm tabular-nums text-muted" :text="registrationFigures($t, one)" :query="props.query" />
          <span class="flex flex-wrap gap-1 pt-1">
            <UTooltip v-for="address in one.addresses" :key="address.node" :text="address.url">
              <UBadge color="neutral" variant="subtle" size="sm" class="gap-1">
                <UIcon :name="keyStateMark(address.key).icon" class="size-4 shrink-0" :class="keyStateMark(address.key).tone" />
                <SearchText class="font-mono" :text="address.node" :query="props.query" />
                <span class="text-muted">{{ keyStateLabel($t, address.key) }}</span>
              </UBadge>
            </UTooltip>
          </span>
        </span>

        <UDropdownMenu :items="menuOf(one)" :content="{ align: 'end' }">
          <UTooltip :text="$t('common.aria.menu_of', { name: one.label })">
            <UButton color="neutral" variant="ghost" size="sm" icon="i-material-symbols-more-vert" :aria-label="$t('common.aria.menu_of', { name: one.label })" />
          </UTooltip>
        </UDropdownMenu>
      </li>

      <li v-if="props.registry.registrations.length === 0" class="px-4 py-3 text-sm text-muted">{{ $t("agents.status.no_registrations") }}</li>
    </ul>

    <p v-if="removeFailed !== ''" role="alert" class="break-words text-sm text-error">{{ removeFailed }}</p>

    <p v-if="gatewayText !== ''" class="flex items-center gap-2 text-sm text-muted" role="status">
      <UIcon name="i-material-symbols-progress-activity" class="size-4 shrink-0 animate-spin" />
      <span>{{ gatewayText }}</span>
    </p>

    <SettingsDialog v-model:open="editing" :title="adding ? $t('agents.title.registry_add') : form.label || form.name" :description="adding ? undefined : form.name" :dirty="formDirty">
      <form id="registry-form" class="flex flex-col gap-4" @submit.prevent="save">
        <div class="grid gap-4 sm:grid-cols-2">
          <UFormField :label="$t('common.label.name')" name="name" :error="errorAt('name')">
            <UInput v-model="form.name" class="w-full font-mono" :disabled="!adding" :placeholder="$t('agents.placeholder.registry_name')" :aria-label="$t('common.label.name')" />
          </UFormField>
          <UFormField :label="$t('agents.label.worker_label')" name="label" :error="errorAt('label')">
            <UInput v-model="form.label" class="w-full" :placeholder="form.name" :aria-label="$t('agents.label.worker_label')" />
          </UFormField>
        </div>

        <UFormField :label="$t('common.label.model')" name="model" :error="errorAt('model')">
          <UInput v-model="form.model" class="w-full font-mono" :aria-label="$t('common.label.model')" />
        </UFormField>

        <div class="grid gap-4 sm:grid-cols-3">
          <UFormField :label="$t('agents.label.context_window')" name="contextTokens" :error="errorAt('contextTokens')">
            <NumberInput v-model="form.contextTokens" v-bind="WHOLE_NUMBER" :digits="9" :aria-label="$t('agents.label.context_window')" />
          </UFormField>
          <UFormField :label="$t('agents.label.registry_answer_size')" name="answerTokens" :error="errorAt('answerTokens')">
            <NumberInput v-model="form.answerTokens" v-bind="WHOLE_NUMBER" :digits="9" :aria-label="$t('agents.label.registry_answer_size')" />
          </UFormField>
          <UFormField :label="$t('agents.label.registry_wait_seconds')" name="waitSeconds" :error="errorAt('waitSeconds')">
            <NumberInput v-model="form.waitSeconds" v-bind="WHOLE_NUMBER" :digits="5" :aria-label="$t('agents.label.registry_wait_seconds')" />
          </UFormField>
        </div>

        <div class="flex flex-wrap gap-x-6 gap-y-2">
          <USwitch v-model="form.thinking" :label="$t('agents.label.registry_thinking_default')" />
          <USwitch v-model="form.walked" :label="$t('agents.label.registry_walk')" />
          <USwitch v-model="form.keepsResearch" :label="$t('agents.label.registry_research')" />
        </div>

        <!-- Every address the same model is served at, one node each. -->
        <div class="flex flex-col gap-2">
          <span class="flex items-center gap-2">
            <span class="text-sm font-medium text-highlighted">{{ $t("agents.label.registry_addresses") }}</span>
            <UButton class="ms-auto" size="sm" color="neutral" variant="outline" icon="i-material-symbols-add" :label="$t('agents.action.address_add')" @click="form.addresses.push(emptyAddress())" />
          </span>

          <div class="flex flex-col divide-y divide-default">
            <div v-for="(address, index) in form.addresses" :key="index" class="flex flex-col gap-2 py-3 first:pt-0" :data-address="index">
              <div class="grid grid-cols-[6rem_minmax(0,1fr)_auto] items-start gap-2">
                <UFormField :label="$t('agents.label.registry_node')" :name="`node-${index}`" :error="errorAt('node', index)">
                  <UInput v-model="address.node" class="w-full min-w-0 font-mono" :aria-label="$t('agents.label.registry_node')" />
                </UFormField>
                <UFormField :label="$t('agents.label.registry_address')" :name="`url-${index}`" :error="errorAt('url', index)">
                  <UInput v-model="address.url" class="w-full min-w-0 font-mono" placeholder="http://127.0.0.1:18021/v1/chat/completions" :aria-label="$t('agents.label.registry_address')" />
                </UFormField>
                <UTooltip :text="$t('agents.aria.address_remove_of', { node: address.node || String(index + 1) })">
                  <UButton
                    class="mt-6"
                    color="neutral"
                    variant="ghost"
                    icon="i-material-symbols-do-not-disturb-on-outline"
                    :aria-label="$t('agents.aria.address_remove_of', { node: address.node || String(index + 1) })"
                    :disabled="form.addresses.length === 1"
                    @click="form.addresses.splice(index, 1)"
                  />
                </UTooltip>
              </div>
              <!-- The no-key checkbox stands beside the field rather than in it, so it keeps a name of its own. -->
              <div class="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-3">
                <UFormField :label="$t('agents.label.key')" :name="`key-${index}`" :error="errorAt('key', index)">
                  <UInput
                    v-model="address.key"
                    type="password"
                    autocomplete="off"
                    class="w-full min-w-0"
                    :placeholder="address.held === 'usable' || address.held === 'unusable' ? $t('agents.label.key_keep') : ''"
                    :disabled="address.clearKey"
                    :aria-label="$t('agents.label.key')"
                  />
                </UFormField>
                <div v-if="address.held === 'usable' || address.held === 'unusable'" class="mt-6 flex h-8 items-center">
                  <UCheckbox v-model="address.clearKey" :label="$t('agents.label.key_clear')" />
                </div>
              </div>
            </div>
          </div>
        </div>

        <p v-if="errorAt('form') !== undefined" role="alert" class="break-words text-sm text-error">{{ errorAt("form") }}</p>
      </form>

      <template #footer="{ close }">
        <UButton :label="$t('common.action.cancel')" color="neutral" variant="ghost" :disabled="busy !== ''" @click="close()" />
        <UButton type="submit" form="registry-form" :label="adding ? $t('common.action.add') : $t('common.action.save')" :loading="busy === 'save'" :disabled="busy !== '' || !formDirty" />
      </template>
    </SettingsDialog>
  </section>
</template>

<script setup lang="ts">
import { settingsAnchor } from "~/domain/agent-settings";
import { GENERAL_SECTIONS, generalSectionTitle } from "~/domain/agents";
import { failureText } from "bmo-ui/app/domain/errors";
import { useReadFailure } from "~/domain/events";
import { useIsland } from "bmo-ui/app/domain/island";
import { emptyAddress, emptyRegistrationForm, gatewayLine, keyStateLabel, keyStateMark, refusalPlace, registrationFigures, registrationForm, registrySaves } from "~/domain/registry";

import type { RegistrationView, RegistryView } from "#shared/agency";
import type { DropdownMenuItem } from "@nuxt/ui";
import type { RegistrationForm } from "~/domain/registry";

interface Props {
  // The registry as the page last read it, or null before its first read has landed.
  registry: RegistryView | null;
  failed?: boolean;
  // Whether the page is reading the registry now.
  reading?: boolean;
  // Every change is asked of the bot and then read back by the page, which this waits for.
  reread: () => Promise<void>;
  query?: string;
}

const props = withDefaults(defineProps<Props>(), { failed: false, reading: false, query: "" });
const { t } = useI18n();
const readFailure = useReadFailure({ failed: () => props.failed && props.registry === null, reading: () => props.reading, reread: () => void props.reread() });
const island = useIsland(t);
// As many waiting rows as the page's own waiting shape stood with.
const REGISTRY_WAITING = GENERAL_SECTIONS.find((one) => one.id === "registry")?.waiting ?? 1;

// A whole number of tokens or seconds, written with its thousands separators.
const WHOLE_NUMBER = { step: 1, formatOptions: { useGrouping: true, maximumFractionDigits: 0 }, locale: "en-US" } as const;

const busy = ref("");
const editing = ref(false);
const adding = ref(false);
const form = ref<RegistrationForm>(emptyRegistrationForm());
// The box as it opened; its Save button is on only while it differs (docs/web.md, 「Forms」).
const formOpened = ref(JSON.stringify(form.value));
const formDirty = computed(() => JSON.stringify(form.value) !== formOpened.value);
const editedFrom = ref<RegistrationView | null>(null);
// The refusal the bot gave the last save, where in the box it stands, and the address it was about.
const removeFailed = ref("");
const refusal = ref<{ place: string; address: number | null; text: string } | null>(null);

const gatewayText = computed(() => gatewayLine(t, props.registry?.gateway ?? null));

function errorAt(place: string, address?: number): string | undefined {
  const held = refusal.value;

  if (held === null || held.place !== place) return undefined;
  if (address !== undefined && held.address !== address) return undefined;

  return held.text;
}

function menuOf(registration: RegistrationView): DropdownMenuItem[] {
  return [
    { label: t("common.action.edit"), icon: "i-material-symbols-edit-outline", onSelect: () => openEdit(registration) },
    {
      label: t("common.action.delete"),
      icon: "i-material-symbols-delete-outline",
      color: "error" as const,
      onSelect: () => void remove(registration)
    }
  ];
}

function openAdd(): void {
  adding.value = true;
  editedFrom.value = null;
  form.value = emptyRegistrationForm();
  formOpened.value = JSON.stringify(form.value);
  refusal.value = null;
  editing.value = true;
}

function openEdit(registration: RegistrationView): void {
  adding.value = false;
  editedFrom.value = registration;
  form.value = registrationForm(registration);
  formOpened.value = JSON.stringify(form.value);
  refusal.value = null;
  editing.value = true;
}

function refusalOf(error: unknown): { field: string | null; text: string } {
  const data = (error as { data?: { data?: { field?: unknown } } } | null)?.data?.data;

  return { field: typeof data?.field === "string" ? data.field : null, text: failureText(error, t("common.error.save_failed")) };
}

// The box's saves, one after another; the first refused stops the rest and stands under the field
// it names. What went before it is saved, and the box is read again as it now stands.
async function save(): Promise<void> {
  const saves = registrySaves(form.value, editedFrom.value);

  busy.value = "save";
  refusal.value = null;

  try {
    for (const [index, body] of saves.entries()) {
      try {
        await $fetch("/api/registry", { method: "POST", body });
      } catch (error) {
        const { field, text } = refusalOf(error);
        const address = body.action === "set-address" ? index - 1 : null;

        refusal.value = { place: refusalPlace(field), address, text };
        adding.value = adding.value && index === 0;
        await props.reread();

        return;
      }
    }

    await props.reread();
    editing.value = false;
  } finally {
    busy.value = "";
  }
}

// A removal is asked in the message pill; what the bot refuses it for — a tier, a pin or a member still
// naming the registration — stands under the list.
async function remove(registration: RegistrationView): Promise<void> {
  const taken = await island.ask({
    title: t("agents.message.registry_delete_confirm"),
    description: registration.label,
    confirm: t("common.action.delete"),
    tone: "warning"
  });

  if (!taken) return;

  removeFailed.value = "";

  try {
    await $fetch("/api/registry", { method: "POST", body: { action: "remove", name: registration.name } });
    await props.reread();
  } catch (error) {
    removeFailed.value = failureText(error, t("common.error.save_failed"));
  }
}

defineExpose({ openAdd });
</script>
