import { unitFetch } from "./units";

// The owner's private chat, as the receiver names it (the receiver's `docs/boundary-api.md`, `GET /owner`), or
// null where the receiver is not reached.
export async function ownerChat(): Promise<number | null> {
  try {
    return (await unitFetch<{ chat: number | null }>("RECEIVER", "/owner")).chat ?? null;
  } catch {
    return null;
  }
}
