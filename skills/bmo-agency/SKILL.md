---
name: bmo-agency
description: Run an item on one of the agency's workers with the bmo-agency MCP server. Use when a harness holds an item and should have a worker run it: ask for a worker with assign_worker, hand the item and the lease over with run_job, and read the job's progress and outcome from its fields and events.
---

# bmo-agency

The agency supplies, delivers and controls agents. For a harness holding an item, it assigns a worker to
that item and runs it there. The harness keeps the item: it takes the lease, renews it while the run lasts,
and reports and closes the item in the job service. A note added to the item while the job runs reaches the
job's turn by agency itself.

## Starting the server

Register it with the harness as a stdio MCP server:

```
bmo-agency mcp --url http://127.0.0.1:3220
```

The address is the agency's base address. `AGENCY_URL` may give it instead; with neither, the server reads
`AGENCY_CONTROL_PORT` and calls the loopback on that port. The token goes in `AGENCY_CONTROL_TOKEN`, in the
file `AGENCY_CONTROL_TOKEN_FILE` names, or in `--token`.

## Running an item

1. `assign_worker` with `itemId`, `holderName` and `item` (the item as you read it: `title`, `brief`,
   `workOrder`, `notes`, `labels`). `assigneeName` asks for one worker. The answer is an `Assignment` with its
   `workerName`; the worker stays held until its job starts and ends, or until you release the assignment, and
   no clock lets it go, so release one you will not run. When no worker is free the call fails with a 422
   `no-worker-free`; ask again later or name another assignee.
2. `run_job` with the assignment's `assignmentId`, the item's `itemId`, the `leaseId` you hold, your
   `holderName` and the same `item`. The answer is a `Job` in state `starting`; the worker's session is
   launched on it.
3. Read the job with `GET /jobs/{jobId}` or from the agency's events (`job.created`, `job.changed`,
   `job.ended`). Its `state` moves `starting` to `running` to `ended` or `stopped`; `outcome` and `report`
   carry what the run came to. `tokenCount` is what the model spent.
4. End or stop the job with `PATCH /jobs/{jobId}`; its assignment is released. A job that ends keeps the lease
   you handed over standing, the agency renewing it, so the item stays `leased` until you close it: report and
   close the item in the job service yourself from `outcome` and `report`, which ends the lease, or give that
   lease back where the item is not to close. A stopped job gives its lease back, and the item is open again.
5. A note added to the item while the job runs is read by agency from the job service's events and handed to
   the job's turn, or kept for its next turn; there is nothing to pass on. Other words for the job go to
   `POST /jobs/{jobId}/words`, or to its `sessionId` at `POST /sessions/{sessionId}/words`, while it is
   `starting`, `running` or `reviewing`. The worker's question is put at `GET /sessions/{sessionId}` under
   `question`, and answered with `POST /sessions/{sessionId}/answers`.

Every failure is the common error shape: a status and a body of `code` and `error`.
