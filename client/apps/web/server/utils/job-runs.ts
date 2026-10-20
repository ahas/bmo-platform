import { STATUS_HISTORY_LIMIT, WORKER_AGENT } from "#shared/config";

import { agency, agencyCall } from "./agency";

import type { Job as AgencyJob } from "@agency/client";
import type { JobLine, JobPrompt, JobRecord } from "#shared/types";

// A worker's job and the jobs that have just run, as the agency's fixed API reads them (the agency's `docs/api.md`,
// 「assignments and jobs」), into the shapes the worker's screen and the live board draw. The agency's times are
// milliseconds and the console's seconds.

const seconds = (ms: number): number => Math.floor(ms / 1000);

/** The job a worker's screen keeps a line for, or null where the worker has none. */
export async function readWorkerJob(worker: string, run: string | null): Promise<JobLine | null> {
  const jobs = await agencyCall(agency().GET("/jobs", { params: { query: { workerName: worker, limit: 50 } } }));
  const named = run === null || run.trim() === "" ? undefined : jobs.items.find((job) => job.runName === run.trim() || String(job.runId ?? job.id) === run.trim());
  const live = jobs.items.find((job) => job.state === "running" || job.state === "starting");
  const job = named ?? live ?? [...jobs.items].sort((one, other) => other.startedAt - one.startedAt)[0];

  return job === undefined ? null : {
    run: job.runName ?? String(job.runId ?? job.id),
    todoId: job.itemId,
    title: null,
    purpose: null,
    startedAt: seconds(job.startedAt),
    finishedAt: job.endedAt === null ? null : seconds(job.endedAt),
    tools: 0,
    models: []
  };
}

/** The prompt of the job that line speaks of, or null where the worker has none. */
export async function readWorkerJobPrompt(worker: string, run: string | null): Promise<JobPrompt | null> {
  const job = await jobNamed(worker, run);

  if (job === null) return null;

  try {
    const prompt = await agencyCall(agency().GET("/jobs/{jobId}/prompt", { params: { path: { jobId: job.id } } }));

    return { text: prompt.text, english: prompt.english ?? null };
  } catch (error) {
    if ((error as { statusCode?: number }).statusCode === 404) return null;
    throw error;
  }
}

async function jobNamed(worker: string, run: string | null): Promise<AgencyJob | null> {
  const jobs = await agencyCall(agency().GET("/jobs", { params: { query: { workerName: worker, limit: 50 } } }));
  const named = run === null || run.trim() === "" ? undefined : jobs.items.find((job) => job.runName === run.trim() || String(job.runId ?? job.id) === run.trim());
  const live = jobs.items.find((job) => job.state === "running" || job.state === "starting");
  const job = named ?? live ?? [...jobs.items].sort((one, other) => other.startedAt - one.startedAt)[0];

  return job ?? null;
}

/** The jobs that have just run, newest first, as many as the live board shows of a kind. */
export async function readRecentJobs(): Promise<JobRecord[]> {
  const page = await agencyCall(agency().GET("/jobs", { params: { query: { state: "ended,stopped", limit: STATUS_HISTORY_LIMIT } } }));

  return page.items.map((job) => ({
    id: job.id,
    unit: "",
    chatId: 0,
    model: null,
    launchedAt: seconds(job.startedAt),
    finishedAt: job.endedAt === null ? null : seconds(job.endedAt),
    outcome: job.outcome,
    ok: job.outcome === "ok" ? true : job.outcome === null ? null : false,
    todoId: job.itemId,
    attempt: 1,
    purpose: job.report,
    worker: job.workerName
  }));
}
