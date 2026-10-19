<template>
  <!-- One memory's active flag. Putting a row back is one press. Taking one down asks why first, in
       a dropdown on the switch itself, since a row that comes down keeps the reason for good. -->
  <UPopover v-if="memory.active" v-model:open="asking" :content="{ align: 'end', side: 'bottom', sideOffset: 0 }">
    <USwitch
      :model-value="true"
      :aria-label="$t('memories.aria.retire_named', { name: memory.slug })"
      :loading="busy === memory.slug"
      :disabled="busy !== ''"
      @update:model-value="asking = true"
    />

    <template #content>
      <form class="flex w-80 max-w-[calc(100vw-2rem)] flex-col gap-3 p-4" @submit.prevent="takeDown">
        <UFormField :label="$t('memories.label.retire_reason')">
          <UInput v-model="why" class="w-full" size="sm" autofocus :aria-label="$t('memories.aria.retire_reason_named', { name: memory.slug })" />
        </UFormField>

        <div class="flex justify-end gap-2">
          <UButton size="sm" color="neutral" variant="ghost" :label="$t('common.action.cancel')" @click="asking = false" />
          <UButton size="sm" color="error" type="submit" :label="$t('memories.action.retire')" :loading="busy === memory.slug" :disabled="why.trim() === ''" />
        </div>
      </form>
    </template>
  </UPopover>

  <USwitch
    v-else
    :model-value="false"
    :aria-label="$t('memories.aria.restore_named', { name: memory.slug })"
    :loading="busy === memory.slug"
    :disabled="busy !== ''"
    @update:model-value="restore()"
  />
</template>

<script setup lang="ts">
import type { MemoryRowView } from "#shared/types";

const props = defineProps<{
  memory: MemoryRowView;
  // The slug of the row whose write is out, or empty while none is: every switch waits on it.
  busy: string;
  retire: (reason: string) => Promise<void>;
  restore: () => Promise<void>;
}>();

const asking = ref(false);
const why = ref("");

// Nothing goes out until a reason is written, and the dropdown stays open until the write is back.
async function takeDown(): Promise<void> {
  const reason = why.value.trim();

  if (reason === "") {
    return;
  }

  await props.retire(reason);
  asking.value = false;
  why.value = "";
}
</script>
