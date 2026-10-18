<template>
  <!-- The registered models before they have arrived, in the list's own row: the label and name, the
       model, the figures and the address badges, at the row's own padding and gaps. The page's own
       waiting shape for the section is this one too, so the list never turns from one skeleton into
       another. -->
  <SkeletonStatus class="flex flex-col divide-y divide-default overflow-hidden rounded-lg border border-default" :label="$t('agents.status.registry_loading')">
    <div v-for="one in props.rows" :key="one" class="flex flex-col gap-1 py-2 ps-4 pe-2">
      <SkeletonBar class="h-5 w-48 max-w-full" />
      <SkeletonBar class="h-6 w-64 max-w-full" />
      <SkeletonBar class="h-5 w-80 max-w-full" />
      <span class="flex gap-1 pt-1">
        <SkeletonBar class="h-5 w-28" />
        <SkeletonBar class="h-5 w-28" />
      </span>
    </div>
  </SkeletonStatus>
</template>

<script setup lang="ts">
const props = defineProps<{ rows: number }>();
</script>
