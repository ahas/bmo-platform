import { unitAt, unitHeaders } from "./units";

import type { AvatarKind, AvatarRef } from "#shared/types";

// A room's and a person's profile picture, which the chat service that keeps it serves by its kind and id
// (docs/web.md, 「Profile photos」; the Telegram service's `docs/api.md`, `GET /rooms/{roomId}/avatar` and
// `GET /people/{personId}/avatar`). The console serves it under its own path behind the sign-in, so the
// browser never names the service.

const KINDS: readonly string[] = ["room", "person"];

export function isAvatarKind(value: unknown): value is AvatarKind {
  return typeof value === "string" && KINDS.includes(value);
}

// Every picture the console can show. No unit serves the list of the pictures it keeps, with the moment
// each was taken, so the list stands empty and every face stands as its letter until one does
// (docs/boundary.md, 「Gaps the console meets」).
export async function avatarRefs(): Promise<AvatarRef[]> {
  return [];
}

// One subject's picture as the chat service serves it, or null where it keeps none or is not reached.
export async function avatarPicture(kind: AvatarKind, subjectId: number): Promise<Uint8Array | null> {
  const unit = unitAt("TELEGRAM");

  if (unit === null) return null;

  const path = kind === "room" ? `/rooms/${subjectId}/avatar` : `/people/${subjectId}/avatar`;

  try {
    const response = await fetch(`${unit.address}${path}`, { headers: unitHeaders(unit), signal: AbortSignal.timeout(5_000) });

    return response.ok ? new Uint8Array(await response.arrayBuffer()) : null;
  } catch {
    return null;
  }
}
