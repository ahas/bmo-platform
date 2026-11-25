// A non-model cleanup helper. Its pipe closes even when the supervisor receives SIGKILL.
// The supervised process group is never left running merely because its collector died.
const group = Number(process.argv[2]);
if (!Number.isSafeInteger(group) || group <= 1) throw new Error("invalid process group");
await Bun.stdin.text();
try { process.kill(-group, "SIGKILL"); } catch { /* The group already ended. */ }
