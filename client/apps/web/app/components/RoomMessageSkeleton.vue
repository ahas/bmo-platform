<!-- The place one message of a conversation keeps while the messages are being read: whose it is,
     and the bubble their words stand in, with the photo beside its foot as a run's last bubble
     stands it. -->
<template>
  <div class="flex items-end gap-3">
    <SkeletonBar class="size-8 shrink-0 rounded-full" />
    <div class="flex min-w-0 flex-1 flex-col gap-1">
      <SkeletonLine width="w-24" size="sm" />
      <div class="w-2/3 rounded-2xl rounded-bl-none bg-elevated/40 px-3 py-2">
        <SkeletonText :lines="2" lastWidth="w-2/3" />
      </div>
    </div>
  </div>
</template>
