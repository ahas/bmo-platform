<!-- The place one line of an agent's log keeps: its clock, what kind of line it is and what it
     names, with what it did on the line under them. -->
<template>
  <div class="flex flex-col gap-1">
    <div class="flex h-5 flex-wrap items-center gap-2">
      <SkeletonBar class="h-4 w-14" />
      <SkeletonBar class="h-5 w-12 rounded-sm" />
      <SkeletonBar class="h-4 w-24" />
    </div>
    <SkeletonLine width="w-3/4" />
  </div>
</template>
