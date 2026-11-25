import { registerChatAgent } from "#shared/agent-names";

// The chat agent's names, registered before any route answers (docs/web.md, 「Settings」).
export default defineNitroPlugin(() => {
  registerChatAgent(useRuntimeConfig().public.chatAgent);
});
