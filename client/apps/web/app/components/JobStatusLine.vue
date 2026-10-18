<template>
  <!-- A worker's job, the way the CLI keeps it at the foot of its own screen: one line that says
       which item, how long, how many tool calls and what the job wrote, and under it, pressed, what
       every model it used spent and the way to the item itself. The figures tick while the job runs,
       so they are the fixed-width ones and the line keeps its width under the reader's pointer. -->
  <SkeletonStatus v-if="!loaded" as="div" :label="$t('common.status.loading')" class="flex h-7 items-center">
    <SkeletonBar class="h-4 w-full max-w-md" />
  </SkeletonStatus>

  <UCollapsible v-else-if="view !== null" v-model:open="open" class="flex flex-col gap-2">
    <button
      type="button"
      class="flex w-full min-w-0 flex-wrap items-center gap-x-2 gap-y-1 rounded-md px-2 py-1 text-start text-sm transition-colors hover:bg-elevated active:bg-accented"
      :aria-label="`${$t('agents.aria.job_line')} ${open ? $t('common.action.collapse') : $t('common.action.expand')}`"
    >
      <span class="flex min-w-0 flex-1 basis-60 items-center gap-2">
        <UIcon
          :name="view.running ? 'i-material-symbols-progress-activity' : 'i-material-symbols-check-circle-outline'"
          class="size-4 shrink-0"
          :class="view.running ? 'animate-spin text-primary motion-reduce:animate-none' : 'text-muted'"
        />
        <ItemTip v-if="view.todoId !== null" :id="view.todoId" press class="flex min-w-0 items-center gap-2">
          <span class="shrink-0 tabular-nums text-highlighted">#{{ view.todoId }}</span>
          <span class="min-w-0 truncate text-default">{{ view.title }}</span>
        </ItemTip>
        <span v-else class="min-w-0 truncate text-default">{{ view.title }}</span>
      </span>

      <span class="flex shrink-0 items-center gap-2 text-muted tabular-nums sm:ms-auto">
        <span>{{ view.time }}</span>
        <span aria-hidden="true">·</span>
        <span>{{ view.tools }}</span>
        <span aria-hidden="true">·</span>
        <UTooltip :text="$t(CHIP_LABELS.tokens)">
          <span class="flex items-center gap-1">
            <UIcon :name="CHIP_ICONS.tokens" class="size-4 shrink-0" />
            {{ view.written }}
          </span>
        </UTooltip>
        <UIcon name="i-material-symbols-expand-more" class="size-5 shrink-0 transition-transform" :class="open ? 'rotate-180' : ''" />
      </span>
    </button>

    <template #content>
      <div class="flex flex-col gap-2 px-2">
        <!-- The prompt the job was handed, in the Korean a person reads it in, with the English its
             worker read under it where that says something else. -->
        <section v-if="!promptLoaded || promptFailed || prompt !== null" class="flex flex-col gap-2">
          <h3 class="text-sm text-muted">{{ $t('agents.label.job_prompt') }}</h3>

          <SkeletonStatus v-if="!promptLoaded" as="div" :label="$t('common.status.loading')" class="flex flex-col gap-1">
            <SkeletonBar class="h-4 w-full" />
            <SkeletonBar class="h-4 w-full" />
            <SkeletonBar class="h-4 w-2/3" />
          </SkeletonStatus>
          <p v-else-if="promptFailed" class="text-sm text-error">{{ $t('agents.error.job_prompt_failed') }}</p>
          <template v-else-if="prompt !== null">
            <p class="max-h-60 overflow-y-auto text-sm break-words whitespace-pre-wrap text-default">{{ prompt.text }}</p>

            <UCollapsible v-if="prompt.english !== null" v-model:open="englishOpen" class="flex flex-col gap-2">
              <UButton
                :label="$t('agents.label.job_prompt_english')"
                trailing-icon="i-material-symbols-expand-more"
                size="sm"
                color="neutral"
                variant="ghost"
                class="self-start"
                :ui="{ trailingIcon: englishOpen ? 'rotate-180 transition-transform' : 'transition-transform' }"
              />

              <template #content>
                <p class="max-h-60 overflow-y-auto font-mono text-sm break-words whitespace-pre-wrap text-default">{{ prompt.english }}</p>
              </template>
            </UCollapsible>
          </template>
        </section>

        <ul v-if="view.models.length > 0" class="flex flex-col gap-2 text-sm">
          <li v-for="model in view.models" :key="model.model" class="flex flex-col gap-1">
            <span class="flex min-w-0 items-baseline gap-2">
              <span class="min-w-0 truncate font-mono text-highlighted">{{ model.model }}</span>
              <span class="ms-auto whitespace-nowrap text-muted tabular-nums">{{ model.total }}</span>
            </span>

            <UsageSplit :tally="model.tally" />
          </li>
        </ul>

        <div v-if="view.todoId !== null">
          <UButton
            :label="$t('agents.action.open_item')"
            icon="i-material-symbols-checklist-rtl"
            size="sm"
            color="neutral"
            variant="outline"
            @click="openItem(view.todoId)"
          />
        </div>
      </div>
    </template>
  </UCollapsible>
</template>

<script setup lang="ts">
import { CHIP_ICONS, CHIP_LABELS } from "~/domain/chips";
import { jobLineView, useJobLine, useJobPrompt } from "~/domain/job-line";

import type { JobLineView } from "~/domain/job-line";

interface Props {
  agent: string;
  // The run the screen was opened at, which the line speaks of where it is one of this worker's jobs.
  run: string | null;
  // The screen's own clock, in seconds, which the running job's time is counted against.
  now: number;
}

const props = defineProps<Props>();

const { t } = useI18n();
const router = useRouter();
const route = useRoute();

const { job, loaded } = useJobLine(props.agent, toRef(props, "run"));
const view = computed<JobLineView | null>(() => (job.value === null ? null : jobLineView(t, job.value, props.now)));

// Folded or spread stays as the reader left it while the screen is open, across jobs.
const open = ref(false);
const englishOpen = ref(false);

const { prompt, loaded: promptLoaded, failed: promptFailed } = useJobPrompt(props.agent, computed(() => job.value?.run ?? null), open);

// The item is opened the way a notification opens one: by its number in the address, on the TODO
// panel, which on a narrow screen is a drawer that has to be brought up too.
const todoOpen = useState<boolean>("todo-open", () => false);

async function openItem(todoId: number): Promise<void> {
  todoOpen.value = true;
  await router.push({ query: { ...route.query, todo: String(todoId) } });
}
</script>
