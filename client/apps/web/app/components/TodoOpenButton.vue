<template>
  <UTooltip :text="$t('todo.action.open_todo')">
    <UButton
      class="shrink-0 xl:hidden"
      icon="i-material-symbols-checklist-rtl"
      color="neutral"
      variant="ghost"
      :aria-label="$t('todo.action.open_todo')"
      @click="todoOpen = true"
    />
  </UTooltip>
</template>

<script setup lang="ts">
// The layout keeps the drawer alive; each page only provides its entrance.
const todoOpen = useState<boolean>("todo-open", () => false);
</script>
