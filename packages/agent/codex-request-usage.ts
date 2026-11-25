import { homeDir } from "@agency/support/roots";
import { closeSync, fstatSync, openSync, readSync } from "node:fs";
import { join } from "node:path";
import type { AgentReply } from "./agent-reply";

type RequestUsage = NonNullable<AgentReply["lastRequestUsage"]>;

const chunkBytes = 1 << 20;
// How far back a turn's start is looked for. A turn longer than this is reported by its last
// request alone rather than by a partial run of requests that would read as a turn of its own.
const turnScanBytes = 64 << 20;

/**
 * Every request of the thread's latest turn, oldest first, as the rollout recorded them: each
 * `token_count` after the turn's `task_started`. The CLI compacts its own thread in the middle of a
 * turn and records a zero-token count at that point, which is no request and is left out. Null where
 * the thread's rollout cannot be read or carries a count that is not a count.
 */
export function codexTurnRequestUsages(sessionId: string | null, root = join(homeDir(), ".codex", "sessions")): RequestUsage[] | null {
  if (!sessionId || !/^[a-zA-Z0-9-]+$/.test(sessionId)) return null;
  try {
    const paths = [...new Bun.Glob(`**/*${sessionId}.jsonl`).scanSync({ cwd: root, absolute: true })];
    if (paths.length !== 1) return null;
    const fd = openSync(paths[0]!, "r");
    const newestFirst: RequestUsage[] = [];
    try {
      const end = fstatSync(fd).size;
      let position = end;
      let carry = Buffer.alloc(0);
      while (position > 0) {
        const length = Math.min(chunkBytes, position);
        position -= length;
        const chunk = Buffer.alloc(length);
        readSync(fd, chunk, 0, length, position);
        let data = Buffer.concat([chunk, carry]);
        // A line is read only once it is whole. A newline byte never sits inside a multibyte
        // character, so the cut is made on bytes and a character split by the chunk stays intact.
        carry = Buffer.alloc(0);
        if (position > 0) {
          const cut = data.indexOf(10);
          carry = cut < 0 ? data : data.subarray(0, cut);
          data = cut < 0 ? Buffer.alloc(0) : data.subarray(cut + 1);
        }
        for (const line of data.toString("utf8").split("\n").reverse()) {
          let event;
          try { event = JSON.parse(line); } catch { continue; }
          if (event.type !== "event_msg") continue;
          if (event.payload?.type === "task_started") return newestFirst.reverse();
          if (event.payload?.type !== "token_count") continue;
          const u = event.payload.info?.last_token_usage;
          if (!u || ![u.input_tokens, u.cached_input_tokens, u.output_tokens].every(v => typeof v === "number" && Number.isFinite(v) && v >= 0)) return null;
          if (u.input_tokens === 0) continue;
          const write = typeof u.cache_write_input_tokens === "number" ? u.cache_write_input_tokens : 0;
          newestFirst.push({ inputTokens: Math.max(0, u.input_tokens - u.cached_input_tokens - write), cacheReadTokens: u.cached_input_tokens, cacheWriteTokens: write, outputTokens: u.output_tokens });
        }
        if (end - position >= turnScanBytes) return newestFirst.slice(0, 1);
      }
    } finally { closeSync(fd); }
    return newestFirst.reverse();
  } catch { /* Missing local journal is unknown, never cumulative usage. */ }
  return null;
}

