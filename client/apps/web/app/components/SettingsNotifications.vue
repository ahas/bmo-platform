<template>
  <!-- The notifications section (docs/web.md, 「Settings」, 「Notifications」): whether this device hears the
       console, which kinds it hears, each a row with its switch taking effect as it is switched, and
       the account's other devices. -->
  <div class="flex flex-col gap-6">
    <ContentSkeleton v-if="push.initial.value" :rows="3" listClass="flex flex-col divide-y divide-default" :label="$t('account.status.notifications_loading')">
      <div class="flex items-center gap-4 py-3">
        <div class="flex min-w-0 flex-1 flex-col gap-1">
          <SkeletonLine width="w-1/3" />
          <SkeletonLine width="w-1/2" />
        </div>
        <SkeletonBar class="h-5 w-9 shrink-0 rounded-full" />
      </div>
    </ContentSkeleton>

    <!-- Nothing was read, so there is nothing to show and the one thing to do is read it again. A
         change that did not go through is the other case and stands below. -->
    <div v-else-if="push.readFailure.value" class="flex flex-col items-end gap-3">
      <UAlert class="w-full" color="error" variant="subtle" :description="push.readFailure.value" />
      <UButton
        color="neutral"
        variant="subtle"
        icon="i-material-symbols-refresh"
        :label="$t('common.action.refresh')"
        :loading="push.initial.value"
        @click="push.reload"
      />
    </div>

    <UAlert v-else-if="push.notice.value" color="warning" variant="subtle" :description="push.notice.value" />

    <template v-else>
      <SettingsGroup v-if="push.here.value === null">
        <SettingsRow :label="$t('account.status.not_subscribed')">
          <UButton icon="i-material-symbols-notifications-outline" :label="$t('account.action.subscribe')" :loading="push.busy.value" @click="push.subscribe" />
        </SettingsRow>
      </SettingsGroup>

      <template v-else>
        <SettingsGroup>
          <SettingsRow v-for="kind in PUSH_KIND_LIST" :key="kind.id" :label="$t(kind.label)" :description="$t(kind.note)">
            <USwitch
              :model-value="push.here.value.kinds.includes(kind.id)"
              :aria-label="$t(kind.label)"
              :disabled="push.busy.value"
              @update:model-value="(on: boolean) => toggleKind(kind.id, on)"
            />
          </SettingsRow>
        </SettingsGroup>

        <div class="flex flex-col gap-2">
          <div class="flex flex-wrap items-center justify-end gap-2">
            <UButton
              color="neutral"
              variant="subtle"
              icon="i-material-symbols-send-outline"
              :label="$t('account.action.send_test')"
              :loading="push.busy.value"
              @click="push.ringTest"
            />
            <UButton
              color="neutral"
              variant="ghost"
              icon="i-material-symbols-notifications-off-outline"
              :label="$t('account.action.unsubscribe')"
              :loading="push.busy.value"
              @click="push.unsubscribe"
            />
          </div>

          <p v-if="push.tested.value" class="text-sm text-muted">{{ push.tested.value }}</p>
        </div>
      </template>

      <SettingsGroup v-if="push.others.value.length > 0" :title="$t('account.label.other_devices', { n: countText(push.others.value.length) })">
        <SettingsRow
          v-for="device in push.others.value"
          :key="device.endpoint"
          :label="$t('account.label.device_since', { time: formatStamp(device.createdAt), kinds: kindsLine(device.kinds) })"
        >
          <UButton
            color="neutral"
            variant="ghost"
            size="xs"
            :label="$t('account.action.unsubscribe')"
            :disabled="push.busy.value"
            @click="push.forget(device.endpoint)"
          />
        </SettingsRow>
      </SettingsGroup>
    </template>

    <UAlert v-if="push.failure.value" color="error" variant="subtle" :description="push.failure.value" />
  </div>
</template>

<script setup lang="ts">
import { PUSH_KIND_LIST } from "#shared/config";

import { countText } from "~/domain/numbers";
import { usePush } from "~/domain/push";
import { formatStamp } from "bmo-ui/app/domain/time";

import type { PushKind } from "#shared/config";

const { t } = useI18n();
const push = usePush(t, { immediate: true });

// A switch says what this device wants now, and the whole list is what is sent: the server keeps
// the kinds left out as the ones this device refused.
async function toggleKind(kind: PushKind, on: boolean): Promise<void> {
  const held = push.here.value?.kinds ?? [];

  await push.setKinds(on ? [...new Set([...held, kind])] : held.filter((one) => one !== kind));
}

function kindsLine(kinds: PushKind[]): string {
  return kinds.length === 0 ? t("account.label.nothing_subscribed") : PUSH_KIND_LIST.filter((kind) => kinds.includes(kind.id)).map((kind) => t(kind.label)).join(", ");
}
</script>
