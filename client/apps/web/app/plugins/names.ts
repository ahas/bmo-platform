import { registerChatAgent } from "#shared/agent-names";

// The chat agent's names, registered before any screen draws (docs/web.md, 「Settings」).
export default defineNuxtPlugin(() => {
  registerChatAgent(useRuntimeConfig().public.chatAgent);
});
