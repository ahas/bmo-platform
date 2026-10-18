<!-- The place one run of the live board keeps: what is running it, the two lines it is read by, and
     the mark that says the row leads somewhere. -->
<template>
  <div class="flex items-center gap-3 rounded-lg border border-default px-4 py-3">
    <SkeletonBar class="size-5 shrink-0 rounded-sm" />
    <div class="min-w-0 grow">
      <SkeletonLine width="w-1/2" />
      <SkeletonLine width="w-3/4" />
    </div>
    <SkeletonBar class="size-4 shrink-0 rounded-sm" />
  </div>
</template>
