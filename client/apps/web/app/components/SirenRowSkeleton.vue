<!-- The place one request of the watch keeps. The row stands on one line wherever the list has the
     width for it, which is the form it is read in, so the placeholder stands that one line. -->
<template>
  <div class="flex items-center gap-3 rounded-lg border border-default px-4 py-3">
    <SkeletonBar class="size-5 shrink-0 rounded-sm" />
    <div class="flex h-5 min-w-0 grow items-center gap-3">
      <SkeletonBar class="h-4 w-16 shrink-0" />
      <SkeletonBar class="h-4 w-20 shrink-0" />
      <SkeletonBar class="h-4 min-w-0 flex-1" />
      <SkeletonBar class="h-4 w-16 shrink-0" />
    </div>
    <SkeletonBar class="size-4 shrink-0 rounded-sm" />
  </div>
</template>
