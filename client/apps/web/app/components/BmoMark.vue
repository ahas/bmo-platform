<template>
  <!-- The mascot's own mark, for a screen. It is the tab's icon: the mascot's controls, the yellow cross pad and the
       three buttons, each in its own colour and a darker shade of it as its outline, on a rounded
       square of the mascot's mint, so the picture on a tab, on a home screen and at the top of the sidebar is
       read as one. Its colours are the mascot's own rather than the theme's, so it stands the same in either
       theme. The mark is 20 CSS pixels square, one vector drawing the browser smooths at whatever
       density the screen has. -->
  <svg
    :viewBox="`0 0 ${SIDE} ${SIDE}`"
    :width="SIDE"
    :height="SIDE"
    role="presentation"
    aria-hidden="true"
    focusable="false"
  >
    <rect :width="SIDE" :height="SIDE" :rx="frame.corner" :fill="BMO_MINT" />
    <g :transform="frame.transform" :stroke-width="CONTROL_OUTLINE" stroke-linejoin="round">
      <template v-for="control in ICON_CONTROLS" :key="control.name">
        <polygon
          v-if="control.kind === 'polygon'"
          :points="svgPoints(control.points)"
          :fill="CONTROL_PAINT[control.name].fill"
          :stroke="CONTROL_PAINT[control.name].outline"
        />
        <circle
          v-else
          :cx="control.cx"
          :cy="control.cy"
          :r="control.r"
          :fill="CONTROL_PAINT[control.name].fill"
          :stroke="CONTROL_PAINT[control.name].outline"
        />
      </template>
    </g>
  </svg>
</template>

<script setup lang="ts">
import { BMO_MINT, CONTROL_OUTLINE, CONTROL_PAINT, ICON_CONTROLS, roundedIcon, SMALL_ICON_FILL, svgPoints } from "#shared/bmo-controls";

// The mark's side in CSS pixels, which the sidebar's rows are laid out around.
const SIDE = 20;

const frame = roundedIcon(SIDE, SMALL_ICON_FILL);
</script>
