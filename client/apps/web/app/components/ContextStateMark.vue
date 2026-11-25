<template>
  <!-- Whether a reference document was read on this turn or was already in the session: an icon
       alone, named by its tooltip. A pointer names it on hover, a keyboard on focus and a finger
       by resting on it, as every tooltip of the console does where a tap would act. The finger is
       held here and never reaches the row: a finger resting on a message chooses it, and this
       finger is asking what the mark means. -->
  <HintTip :text="props.look.label" press class="inline-flex shrink-0" @touchstart.stop>
    <UBadge
      :icon="props.look.icon"
      :color="props.look.color"
      variant="subtle"
      size="sm"
      square
      role="img"
      tabindex="0"
      :aria-label="props.look.label"
      class="shrink-0"
      @contextmenu.prevent.stop
      @click.stop
    />
  </HintTip>
</template>

<script setup lang="ts">
import type { ContextStateLook } from "~/domain/log";

interface Props {
  look: ContextStateLook;
}

const props = defineProps<Props>();
</script>
