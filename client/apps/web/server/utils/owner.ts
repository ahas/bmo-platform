import { receiver, receiverCall } from "./receiver";

// The owner's private chat, as the receiver names it (the receiver's `docs/api.md`, 「The owner」), or null where
// the receiver is not reached.
export async function ownerChat(): Promise<number | null> {
  try {
    return (await receiverCall(receiver().GET("/owner"))).room ?? null;
  } catch {
    return null;
  }
}
