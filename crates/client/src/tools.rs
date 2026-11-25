//! The harness plugin's tools: each one route, called through this client.

/// One tool of the agency's MCP server.
pub struct Tool {
  /// Its name, `<unit>_<what>` in snake_case.
  pub name: &'static str,
  /// What it does, for the harness's own reading.
  pub description: &'static str,
  /// The schema of its input.
  pub input: &'static str,
  /// The schema of its output.
  pub output: &'static str,
  /// The route it calls.
  pub route: &'static str,
}

/// What `file_item` does, as the supervisor's own server and the tool host's entry both say it.
pub const FILE_ITEM: &str = "File an item in the job service: the room it was asked in and whether the owner asked for it are the turn's, which the agency reads; an item the owner asked for himself opens at once, any other is a draft until an approver approves it. Name the repository the work is in, so only that repository's worker takes it, and none for host work; one change that reaches two or more repositories is one item, its repository the one it runs in, the umbrella repository where it has no home of its own, and its repositories label naming every repository it reaches, comma-separated.";

/// What `stop_job` does, as its server and the document both say it.
pub const STOP_JOB: &str = "Stop a running job on the owner's word, by its job id or by its item, with a short reason that becomes the job's report: only a turn whose every message is the owner's stops one. The job's turn is asked to end first, and the answer is the job once its turn has ended, stopped, however long that takes.";

/// What `review_job` does, as its server and the document both say it.
pub const REVIEW_JOB: &str = "Examine a job's report under review, the one a job-review instruction names, on the check due now: ask puts your question to the worker in the job's own session and its answer comes back to you; reject sends the work back with why and what is to be done, to the stage under way or, naming stage, to an earlier stage whose work the report shows falls short, whose step is handed over again with the stages after it following; pass passes the check; escalate puts the work to the owner, refused until the work was sent back as often as the review's ceiling. Any reason to send the work back sends it back.";

/// The tools the agency's MCP server lists.
pub const TOOLS: [Tool; 6] = [
  Tool {
    name: "file_item",
    description: "File an item in the job service: the room it was asked in and whether the owner asked for it are the turn's, which the agency reads; an item the owner asked for himself opens at once, any other waits as a draft for his approval.",
    input: "FileItem",
    output: "ItemFiled",
    route: "POST /item-filings",
  },
  Tool {
    name: "stop_job",
    description: STOP_JOB,
    input: "StopJob",
    output: "Job",
    route: "POST /job-stops",
  },
  Tool {
    name: "review_job",
    description: REVIEW_JOB,
    input: "ReviewJob",
    output: "JobReviewed",
    route: "POST /job-reviews",
  },
  Tool {
    name: "assign_worker",
    description: "Assign a worker to an item for the harness holding it: the agency picks the worker from the item's content and the assignee it names, and holds it until its job starts and ends, or the harness releases it.",
    input: "AssignWorker",
    output: "Assignment",
    route: "POST /assignments",
  },
  Tool {
    name: "release_worker",
    description: "Release an assignment the harness will not run, by its id: the worker it held is free for the next item. A hold also ends when its job ends or when the session that took it ends.",
    input: "ReleaseWorker",
    output: "Assignment",
    route: "PATCH /assignments/{assignmentId}",
  },
  Tool {
    name: "run_job",
    description: "Run an item on its assigned worker: the harness hands over the item's content and the lease it holds, and the agency launches the worker's session on it. The agency calls nothing of the job service.",
    input: "RunJob",
    output: "Job",
    route: "POST /jobs",
  },
];
