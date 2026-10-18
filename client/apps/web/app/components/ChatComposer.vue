<template>
  <!-- The box a message or a job is written in, wherever one is sent from: the words, kept as a
       draft of this place, the long pastes that stand beside them as marks, and the files, which
       reach it by the clip button, a paste or a drop. What is attached stands in the box's head
       until it is sent, each with its own way out; what the place adds — a reply it answers, a
       choice it launches with — stands before the attachments and in the foot. -->
  <div class="flex min-h-0 flex-col" @dragover.prevent.stop="onDragOver" @dragleave="dragging = false" @drop.prevent.stop="onDrop">
    <ChatInput
      ref="box"
      v-model="typed"
      :split="props.split"
      :inline="props.inline"
      :label="props.label"
      :placeholder="dragging ? $t('room.placeholder.drop_here') : props.placeholder"
      :disabled="props.disabled"
      :busy="props.busy"
      :holds="attached.length > 0"
      @keydown.esc="emit('escape', $event)"
      @paste="onPaste"
      @submit="submit"
    >
      <template v-if="$slots.lead || attached.length > 0 || shownPastes.length > 0" #header>
        <div class="flex min-w-0 flex-col gap-2">
          <slot name="lead" />

          <ul v-if="attached.length > 0 || shownPastes.length > 0" class="flex flex-wrap gap-2">
            <li
              v-for="entry in shownPastes"
              :key="entry.key"
              class="flex min-w-0 items-center gap-2 rounded-sm border border-default px-2 py-1"
            >
              <UIcon name="i-material-symbols-content-paste" class="size-5 shrink-0 text-dimmed" />
              <span class="shrink-0 text-sm font-medium text-muted">{{ pastedLabel($t, entry).name }}</span>
              <span class="max-w-60 truncate text-sm text-muted">{{ pastedLabel($t, entry).head }}</span>
              <span class="shrink-0 text-sm text-dimmed tabular-nums">{{ pastedLabel($t, entry).size }}</span>
              <UTooltip :text="$t('room.action.remove_paste')">
                <UButton
                  icon="i-material-symbols-close"
                  color="neutral"
                  variant="ghost"
                  size="xs"
                  :aria-label="$t('room.action.remove_paste')"
                  @click="unpaste(entry)"
                />
              </UTooltip>
            </li>
            <li
              v-for="picked in attached"
              :key="picked.key"
              data-picked
              class="flex min-w-0 items-center gap-2 rounded-sm border border-default px-2 py-1"
            >
              <img v-if="picked.previewUrl !== null" :src="picked.previewUrl" alt="" class="size-8 shrink-0 object-cover" />
              <UIcon v-else name="i-material-symbols-draft-outline" class="size-5 shrink-0 text-dimmed" />
              <span class="max-w-60 truncate text-sm text-muted">{{ attachmentLabel(picked.file) }}</span>
              <UTooltip :text="$t('room.aria.remove_named', { name: picked.file.name })">
                <UButton
                  icon="i-material-symbols-close"
                  color="neutral"
                  variant="ghost"
                  size="xs"
                  :aria-label="$t('room.aria.remove_named', { name: picked.file.name })"
                  @click="remove(picked.key)"
                />
              </UTooltip>
            </li>
          </ul>
        </div>
      </template>

      <template #tools>
        <!-- A phone's file picker takes one file a tap, so a touch screen offers its photo picker
             beside it, where every tap adds one to the choice. -->
        <UDropdownMenu v-if="props.attachable && touch" :items="pickers" :content="{ side: 'top', align: 'start' }">
          <UTooltip :text="$t('room.action.attach_file')">
            <UButton
              icon="i-material-symbols-attach-file"
              color="neutral"
              variant="ghost"
              :aria-label="$t('room.action.attach_file')"
              :disabled="props.disabled"
            />
          </UTooltip>
        </UDropdownMenu>
        <UTooltip v-else-if="props.attachable" :text="$t('room.action.attach_file')">
          <UButton
            icon="i-material-symbols-attach-file"
            color="neutral"
            variant="ghost"
            :aria-label="$t('room.action.attach_file')"
            :disabled="props.disabled"
            @click="picker?.click()"
          />
        </UTooltip>
        <slot name="tools" />
      </template>
    </ChatInput>

    <input v-if="props.attachable" ref="picker" type="file" multiple class="hidden" @change="onPick" />
    <input v-if="props.attachable" ref="mediaPicker" type="file" :accept="MEDIA_ACCEPT" multiple class="hidden" @change="onPick" />
  </div>
</template>

<script setup lang="ts">
import { attachmentLabel, isPreviewable, pickFiles } from "~/domain/attachments";
import { useDraft, useDraftPastes } from "~/domain/drafts";
import { useIsland } from "bmo-ui/app/domain/island";
import { composedText, isPasteEntry, pastedLabel, pastedOf, standingPastes, withoutMark } from "~/domain/pasted";
import { TOUCH_QUERY } from "bmo-ui/app/domain/prompt";
import { useNarrow } from "bmo-ui/app/domain/screen";

import type { ComposedMessage, PickedFile } from "~/domain/attachments";
import type { DraftKind } from "~/domain/drafts";
import type { PastedText } from "~/domain/pasted";
import type { DropdownMenuItem } from "@nuxt/ui";

interface Props {
  // Whose draft this is: the kind of place and which one of them.
  kind: DraftKind;
  draftId: string | number;
  placeholder: string;
  label?: string;
  disabled?: boolean;
  busy?: boolean;
  split?: boolean;
  // Whether what is sent from here may carry files. A place that cannot take them never offers
  // the clip, and a file pasted or dropped on it is left to the browser.
  attachable?: boolean;
  // Whether the box is one line, its buttons on the words' last line (see ChatInput).
  inline?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  label: undefined,
  disabled: false,
  busy: false,
  split: false,
  attachable: false,
  inline: false
});

const emit = defineEmits<{ submit: [message: ComposedMessage]; escape: [event: KeyboardEvent] }>();

const { t } = useI18n();
const island = useIsland(t);

const typed = useDraft(props.kind, props.draftId);
// The long pastes waiting beside the box, kept with the draft; the ones sent are those whose mark
// still stands in it.
const pasted = useDraftPastes(props.kind, props.draftId);
const shownPastes = computed<PastedText[]>(() => standingPastes(typed.value, pasted.value));
const attached = ref<PickedFile[]>([]);
const dragging = ref(false);
const picker = useTemplateRef<HTMLInputElement>("picker");
const mediaPicker = useTemplateRef<HTMLInputElement>("mediaPicker");
const touch = useNarrow(TOUCH_QUERY);

// A picker asked for pictures and videos alone opens a phone's own photo picker.
const MEDIA_ACCEPT = "image/*,video/*";

const pickers = computed<DropdownMenuItem[]>(() => [
  { label: t("room.action.attach_media"), icon: "i-material-symbols-photo-library-outline", onSelect: () => mediaPicker.value?.click() },
  { label: t("room.action.attach_document"), icon: "i-material-symbols-draft-outline", onSelect: () => picker.value?.click() }
]);
const box = useTemplateRef<{ focus: () => void }>("box");

function submit(): void {
  if (props.disabled || props.busy) {
    return;
  }

  const text = composedText(typed.value, shownPastes.value);

  if (text.trim() === "" && attached.value.length === 0) {
    return;
  }

  emit("submit", { text, files: [...attached.value] });
}

// A browser draws an image it already holds; everything else stands as its name and size.
function previewOf(file: File): string | null {
  return isPreviewable(file.type) ? URL.createObjectURL(file) : null;
}

// Files reach here three ways — the clip button, a drop, a paste into the box — and all three end
// in the same list, with anything refused said out loud rather than dropped.
function attach(files: readonly File[]): void {
  if (files.length === 0 || !props.attachable) {
    return;
  }

  const picked = pickFiles(t, attached.value, files, previewOf);

  attached.value = picked.files;

  for (const refusal of picked.refusals) {
    island.say({ title: refusal, tone: "warning" });
  }
}

function remove(key: string): void {
  const going = attached.value.find((picked) => picked.key === key);

  if (going?.previewUrl != null) {
    URL.revokeObjectURL(going.previewUrl);
  }

  attached.value = attached.value.filter((picked) => picked.key !== key);
}

// The box emptied after a send. A place that keeps showing the files — a room, whose feed stands
// them as sending — takes their previews over and lets them go itself.
function clear(options: { keepPreviews?: boolean } = {}): void {
  if (options.keepPreviews !== true) {
    for (const picked of attached.value) {
      if (picked.previewUrl !== null) {
        URL.revokeObjectURL(picked.previewUrl);
      }
    }
  }

  typed.value = "";
  pasted.value = [];
  attached.value = [];
}

// The previews still in the tray go back to the browser with the box; the words stay as the draft.
onScopeDispose(() => {
  for (const picked of attached.value) {
    if (picked.previewUrl !== null) {
      URL.revokeObjectURL(picked.previewUrl);
    }
  }
});

function onPick(event: Event): void {
  const input = event.target as HTMLInputElement;

  attach([...(input.files ?? [])]);
  input.value = "";
}

function onDragOver(event: DragEvent): void {
  dragging.value = props.attachable && (event.dataTransfer?.types.includes("Files") ?? false);
}

function onDrop(event: DragEvent): void {
  dragging.value = false;
  attach([...(event.dataTransfer?.files ?? [])]);
}

// A picture in the clipboard arrives as a file with no name of its own; the box keeps the text it
// was carrying, so pasted words still land in it — unless there are so many of them that they are
// text to hand over rather than to edit, and then a mark stands for them where the caret is, and
// the words go back in that place when the message is sent.
function onPaste(event: ClipboardEvent): void {
  const files = [...(event.clipboardData?.files ?? [])];

  if (files.length > 0 && props.attachable) {
    event.preventDefault();
    attach(files);

    return;
  }

  const text = event.clipboardData?.getData("text/plain") ?? "";

  if (!isPasteEntry(text)) {
    return;
  }

  event.preventDefault();

  const entry = pastedOf(text, pasted.value);

  pasted.value = [...pasted.value, entry];
  insertAtCaret(event.target, entry.mark);
}

// Text put where the caret is, as typing would put it: through the editing command, so undo takes
// it back, and by hand where the browser no longer has the command.
function insertAtCaret(target: EventTarget | null, text: string): void {
  const area = target as HTMLTextAreaElement | null;

  if (area === null || typeof area.setRangeText !== "function") {
    typed.value += text;

    return;
  }

  area.focus();

  if (document.execCommand("insertText", false, text)) {
    return;
  }

  area.setRangeText(text, area.selectionStart, area.selectionEnd, "end");
  area.dispatchEvent(new Event("input", { bubbles: true }));
}

function unpaste(entry: PastedText): void {
  typed.value = withoutMark(typed.value, entry);
}

defineExpose({ focus: () => box.value?.focus(), attach, clear });
</script>
