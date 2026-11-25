<template>
  <!-- A document's headings, indented by depth. The heading being read is marked by weight and a
       rule down its left side as well as by colour, and pressing one goes to it. The list scrolls
       inside itself and keeps the marked heading in view. -->
  <nav :aria-label="props.label ?? $t('common.label.toc')" ref="box" class="relative min-h-0 overflow-y-auto overscroll-contain">
    <p v-if="props.headings.length === 0" class="px-2 text-sm text-muted">{{ $t("documents.status.no_toc") }}</p>

    <ol v-else ref="list" class="flex flex-col">
      <li v-for="heading in props.headings" :key="heading.slug">
        <a
          :href="`#${heading.slug}`"
          :data-slug="heading.slug"
          :aria-current="heading.slug === props.active ? 'location' : undefined"
          class="block min-h-8 rounded-sm border-s-2 py-2 pe-2 text-sm leading-snug break-words focus-inset"
          :class="
            heading.slug === props.active
              ? 'border-primary font-semibold text-highlighted'
              : 'border-transparent text-muted hover:bg-elevated hover:text-default active:bg-accented'
          "
          :style="{ paddingInlineStart: `${0.5 + (heading.depth - minDepth) * 0.75}rem` }"
          @click.prevent="emit('select', heading.slug)"
        >{{ heading.text }}</a>
      </li>
    </ol>
  </nav>
</template>

<script setup lang="ts">
// The table of contents of one document: the document viewer's, and the Archive page's
// for its findings. The headings come from whoever read the text (see markdown-structure); the one
// being read is the caller's to say.

interface TocHeading {
  slug: string;
  text: string;
  depth: number;
}

interface Props {
  headings: readonly TocHeading[];
  active: string | null;
  label?: string;
}

const props = defineProps<Props>();
const emit = defineEmits<{ select: [slug: string] }>();
const list = useTemplateRef<HTMLElement>("list");
const box = useTemplateRef<HTMLElement>("box");

// The shallowest heading stands flush, so a document that starts at `##` is not indented for nothing.
const minDepth = computed(() => Math.min(...props.headings.map((heading) => heading.depth), 6));

// The marked heading is brought into the list's own view, never the page's.
watch(
  () => props.active,
  async (slug) => {
    await nextTick();

    const item = slug === null ? null : list.value?.querySelector<HTMLElement>(`[data-slug="${CSS.escape(slug)}"]`);
    const frame = box.value;

    if (item === null || item === undefined || frame === null) {
      return;
    }

    const top = item.offsetTop;

    if (top < frame.scrollTop || top + item.offsetHeight > frame.scrollTop + frame.clientHeight) {
      frame.scrollTop = top - frame.clientHeight / 3;
    }
  }
);
</script>
