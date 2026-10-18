import { STATUS_HISTORY_LIMIT, WORKER_AGENT } from "#shared/config";
import { jobRead } from "./job";

import type { JobLine, JobPrompt, JobRecord } from "#shared/types";

// A worker's job and the jobs that have just run, as bmo-job reads them (bmo-job's `docs/api.md`, 「Readings」;
// its `docs/reading.md`, "A worker's job line" and "The jobs that have just run"), into the shapes the worker's
// screen and the live board draw. bmo-job's times are milliseconds and the console's seconds.

interface ModelSpent {
  model: string;
  input_tokens: number | null;
  output_tokens: number | null;
  cache_read_tokens: number | null;
  cache_write_tokens: number | null;
}

interface JobLineReading {
  run: string;
  item: number | null;
  title: string | null;
  purpose: string | null;
  launched_at: number;
  finished_at: number | null;
  tools: number;
  models: ModelSpent[];
}

type JobLabel = { kind: "item"; id: number; attempt?: number | null } | { kind: "itemless" };

interface RecentJob {
  id: number;
  unit: string;
  chat: number;
  model: string | null;
  launched_at: number;
  finished_at: number | null;
  outcome: string | null;
  ok: boolean | null;
  label: JobLabel;
  purpose: string | null;
  worker: string | null;
}

const seconds = (ms: number): number => Math.floor(ms / 1000);

// The run the screen was opened at, where it names one, as the query bmo-job reads it by.
function runQuery(run: string | null): string {
  return run === null || run.trim() === "" ? "" : `?run=${encodeURIComponent(run.trim())}`;
}

/** The job a worker's screen keeps a line for, or null where the worker has none. */
export async function readWorkerJob(worker: string, run: string | null): Promise<JobLine | null> {
  const { job } = await jobRead<{ job: JobLineReading | null }>(`/workers/${encodeURIComponent(worker)}/job${runQuery(run)}`);

  return job === null
    ? null
    : {
        run: job.run,
        todoId: job.item,
        title: job.title,
        purpose: job.purpose,
        startedAt: seconds(job.launched_at),
        finishedAt: job.finished_at === null ? null : seconds(job.finished_at),
        tools: job.tools,
        models: job.models.map((spent) => ({
          model: spent.model,
          inputTokens: spent.input_tokens,
          outputTokens: spent.output_tokens,
          cacheReadTokens: spent.cache_read_tokens,
          cacheWriteTokens: spent.cache_write_tokens
        }))
      };
}

/** The prompt of the job that line speaks of, or null where the worker has none. */
export async function readWorkerJobPrompt(worker: string, run: string | null): Promise<JobPrompt | null> {
  const { prompt } = await jobRead<{ prompt: { text: string; english: string | null } | null }>(
    `/workers/${encodeURIComponent(worker)}/job/prompt${runQuery(run)}`
  );

  return prompt === null ? null : { text: prompt.text, english: prompt.english ?? null };
}

/** The jobs that have just run, newest first, as many as the live board shows of a kind. */
export async function readRecentJobs(): Promise<JobRecord[]> {
  const jobs = await jobRead<RecentJob[]>(`/jobs/recent?limit=${STATUS_HISTORY_LIMIT}`);

  return jobs.map((job) => ({
    id: job.id,
    unit: job.unit,
    chatId: job.chat,
    model: job.model,
    launchedAt: seconds(job.launched_at),
    finishedAt: job.finished_at === null ? null : seconds(job.finished_at),
    outcome: job.outcome,
    ok: job.ok,
    // A label names the run of its item past the first; the first names none.
    todoId: job.label.kind === "item" ? job.label.id : null,
    attempt: job.label.kind === "item" ? (job.label.attempt ?? 1) : 0,
    purpose: job.purpose,
    // A job that names no worker is the built-in worker's.
    worker: job.worker ?? WORKER_AGENT
  }));
}
