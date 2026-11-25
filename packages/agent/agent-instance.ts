import { mkdir } from "node:fs/promises";
import { dirname } from "node:path";

// The holder keeps the OS lock across preparation, local summarising and delivery, not only
// across the model process. Closing stdin releases it even when the owning runner dies. A caller
// either takes it at once or not at all, or waits for the holder to let it go however long that takes.
export async function withAgentLock<T>(
  path: string,
  nonBlocking: boolean,
  run: () => Promise<T>,
): Promise<T | null> {
  await mkdir(dirname(path), { recursive: true });
  const waiting = nonBlocking ? ["--nonblock", "--conflict-exit-code", "75"] : [];
  const holder = Bun.spawn(["flock", ...waiting, path,
    "sh", "-c", "printf 'locked\n'; cat >/dev/null"], { env: { ...process.env }, stdin: "pipe", stdout: "pipe", stderr: "pipe" });
  const errors = new Response(holder.stderr).text();
  const reader = holder.stdout.getReader();
  const first = await reader.read();
  reader.releaseLock();
  if (first.done || !new TextDecoder().decode(first.value).includes("locked")) {
    holder.stdin.end(); const code = await holder.exited; const error = await errors;
    if (code === 75) return null;
    throw new Error(`agent lock: ${error || code}`);
  }
  try { return await run(); }
  finally { holder.stdin.end(); await holder.exited; await errors; }
}
