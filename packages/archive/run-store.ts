// The archive's `runs` and `agent_log` tables, read and written here and nowhere else in the
// archive (docs/store.md).

import { deploymentNames } from "@agency/support/names";
import type { AgentStore } from "./agent-store";
import type { RunIdRow } from "./query-rows";
import type { RunCall } from "./run-rows";
import type { ArchiveSql } from "./archive-sql";

// What a launcher opens a run's row with. Nothing of the query or the task is in it: this is a
// record of a run, not of what was asked.
export interface NewRun {
  agent: string;
  chatId: number;
  unit: string;
  depth?: string | null;
  capMs?: number | null;
  // Who set it off: the console, or the chat whose session ran the launcher. A run launched
  // with nobody named belongs to the chat it will report to, which is that chat's own supervisor.
  startedBy?: string | null;
  // The conversational agent whose turn set it off, which its result is queued for; none where no
  // agent's turn did, and then the result goes to the room's voice.
  askedBy?: string | null;
  // The process that is the run, named only by a run opening its own row: a launcher opening a row
  // for a unit it is about to start does not know that process yet.
  process?: RunProcess | null;
  // Which step of a series its opener planned this run is, for a run that is one.
  step?: RunStep | null;
}

// A process as this machine's kernel knows it: the pid with its start time, so a pid handed to a
// later process is not taken for this one, and the machine whose kernel that is.
export interface RunProcess {
  host: string;
  pid: number;
  start: string;
}

export interface RunStep {
  step: number;
  steps: number;
}

export interface OpenRun {
  id: number;
  agent: string;
  chatId: number;
  unit: string;
  depth: string | null;
  capMs: number | null;
  startedAt: number;
  startedBy: string | null;
  process: RunProcess | null;
  step: RunStep | null;
}

// One line of an agent's own log as the head pane of that agent's herdr workspace reads it back:
// the little of it a person watching needs — when, which run, what sort of line and what it said.
export interface AgentLogTail {
  run: string;
  at: number;
  kind: string;
  name: string | null;
  text: string;
}

// One line of an agent's own log, as the writer hands it over. Nobody reads these lines back as
// lines — the web reads the table straight off the file — and the one thing the bot asks the log
// is how many times a run has been started, which is a count rather than a line.
export interface NewAgentLogLine {
  agent: string;
  run: string;
  chatId: number | null;
  kind: string;
  name: string | null;
  parent: string | null;
  ref: string | null;
  truncated: boolean;
  text: string;
  // A worker job keeps its detailed stream for as long as the job row itself. Other agent logs
  // remain the small rolling diagnostic log they have always been.
  jobId?: number | null;
  // The durable order inside that one job. `id` orders the whole table; this one keeps the job's
  // order explicit even when it is read without unrelated agents' rows.
  sequence?: number | null;
  // Where the evidence came from. `bmo` is a line the supervisor wrote beside provider events;
  // `legacy` is a row linked to a job after it had already been reduced by the old logger.
  source?: string | null;
  sourceEvent?: number | null;
  sourceBlock?: number | null;
  sourceId?: string | null;
  state?: "requested" | "running" | "succeeded" | "failed" | "unverified" | "unavailable" | "attached" | "held" | null;
  // This says the provider itself declared loss. `truncated` above only says the list preview is
  // shorter than the full detail agency retained.
  upstreamTruncated?: boolean;
  detailComplete?: boolean;
  // Long text and structured provider evidence are left out of list reads and fetched one bounded
  // piece at a time. A short line needs no duplicate detail: readers fall back to `text`.
  detail?: string | null;
  metadata?: string | null;
}

// SQLite's text length and substr count Unicode code points. Store the same count when a line is
// written so a polling page need not scan a megabyte result merely to say how much detail exists.
function sqliteTextLength(text: string): number {
  let length = 0;

  for (const _character of text) length += 1;
  return length;
}

interface OpenRunRow {
  id: number;
  agent: string;
  chat_id: number;
  unit: string;
  depth: string | null;
  cap_ms: number | null;
  started_at: number;
  started_by: string | null;
  process_host: string | null;
  process_pid: number | null;
  process_start: string | null;
  step: number | null;
  steps: number | null;
}

export class RunStore {
  constructor(
    private readonly sql: ArchiveSql,
    private readonly agents: AgentStore,
  ) {}

  // The newest line one job wrote to the agent log — when, in seconds, and which line — or null where
  // it has written none. Every line a job's runner writes carries the job, and lines are written as
  // they arrive, so the newest one is the job's last activity of any kind: a token, a tool call, a
  // result. The line is handed back with the time because two lines can share a second, and a new
  // line is what says the job moved; its kind and name say what the job was last doing — a tool
  // call with no result after it is a job waiting on that tool.
  async jobLastActivity(jobId: number): Promise<{ at: number; line: number; kind: string; name: string | null } | null> {
    const row = await this.sql.get("SELECT at, id, kind, name FROM agent_log WHERE job_id = ? ORDER BY id DESC LIMIT 1", jobId) as { at: number; id: number; kind: string; name: string | null } | null;

    return row === null ? null : { at: row.at, line: row.id, kind: row.kind, name: row.name };
  }

  // When one run last wrote a line to the agent log. The run's start bounds the search, because the
  // log is indexed by time and by agent and a run's lines are never older than the run.
  async lastAgentLogAt(run: string, since: number): Promise<number | null> {
    const row = await this.sql.get("SELECT max(at) AS at FROM agent_log WHERE at >= ? AND run = ?", since, run) as { at: number | null };

    return row.at;
  }

  // The agent log, one line at a time as the run produces it: a line is in the table the moment
  // it exists, so a run killed from outside leaves everything it did up to the kill. The new
  // line's id is handed back, for a writer that fills in its metadata once the provider has
  // recorded it.
  async appendAgentLog(line: NewAgentLogLine): Promise<number> {
    // The line's place in its job is read and then written, which is the one shape the busy wait
    // does not cover, and this is the archive's busiest writer by far — a job at work writes one of
    // these a second while the bot writes its own. So the two statements are one transaction, and a
    // transaction the lock refused is run again rather than lost.
    return await this.sql.wrote(async () => this.appendedAgentLog(line));
  }

  private async appendedAgentLog(line: NewAgentLogLine): Promise<number> {
    return await this.sql.wrote(async () => {
      const jobId = line.jobId ?? null;
      const sequence = line.sequence ?? (jobId === null
        ? null
        : ((await this.sql.get("SELECT COALESCE(MAX(sequence), 0) + 1 AS sequence FROM agent_log WHERE job_id = ?", jobId) as { sequence: number }).sequence));
      const detailLength = sqliteTextLength(line.detail ?? line.text);
      const metadataLength = line.metadata === null || line.metadata === undefined ? 0 : sqliteTextLength(line.metadata);

      return (await this.sql.get(`
          INSERT INTO agent_log
            (agent, run, chat_id, at, kind, name, parent, ref, truncated, text,
             job_id, sequence, source, source_event, source_block, source_id, state,
             upstream_truncated, detail_complete, detail_length, metadata_length, detail, metadata)
          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
          RETURNING id
        `, line.agent, line.run, line.chatId, Date.now(), line.kind, line.name, line.parent, line.ref, line.truncated ? 1 : 0, line.text, jobId, sequence, line.source ?? (jobId === null ? null : deploymentNames().agentKey), line.sourceEvent ?? null, line.sourceBlock ?? null, line.sourceId ?? null, line.state ?? null, line.upstreamTruncated === true ? 1 : 0, (line.detailComplete ?? !line.truncated) ? 1 : 0, detailLength, metadataLength, line.detail ?? null, line.metadata ?? null) as { id: number }).id;
    });
  }

  // A job line's metadata written after the line itself: what the provider recorded about the
  // change only once the line was already in.
  async setAgentLogMetadata(id: number, metadata: string): Promise<void> {
    await this.sql.run("UPDATE agent_log SET metadata = ?, metadata_length = ? WHERE id = ?", metadata, sqliteTextLength(metadata), id);
  }

  async lastJobActivitySequence(jobId: number): Promise<number> {
    const row = await this.sql.get("SELECT COALESCE(MAX(sequence), 0) AS sequence FROM agent_log WHERE job_id = ?", jobId) as { sequence: number };

    return row.sequence;
  }

  // The newest lines of one agent's own log, oldest first. It is what the head pane of that agent's
  // herdr workspace draws as the agent's session: the run going now writes into it, and once that
  // run has ended the same lines stand as the last thing the agent did.
  async recentAgentLog(agent: string, limit: number): Promise<AgentLogTail[]> {
    const rows = await this.sql.all("SELECT run, at, kind, name, text FROM agent_log WHERE agent = ? ORDER BY id DESC LIMIT ?", await this.agents.canonicalAgent(agent), Math.max(0, limit)) as { run: string; at: number; kind: string; name: string | null; text: string }[];

    return rows.reverse();
  }

  // Every call the named runs of one agent made, oldest first, each beside the answer it got back.
  // It is how a turn is asked what it wrote down: the calls are the record, and nothing the turn
  // said about them is.
  async runCalls(agent: string, runs: readonly string[]): Promise<RunCall[]> {
    if (runs.length === 0) {
      return [];
    }

    const placeholders = runs.map(() => "?").join(", ");

    return await this.sql.all(`SELECT call.name AS name, call.text AS text, answer.text AS result, answer.state AS state
         FROM agent_log call
         LEFT JOIN agent_log answer
           ON answer.agent = call.agent AND answer.run = call.run AND answer.kind = 'tool-result' AND answer.ref = call.ref
         WHERE call.agent = ? AND call.run IN (${placeholders}) AND call.kind = 'tool'
         ORDER BY call.id`, await this.agents.canonicalAgent(agent), ...runs) as RunCall[];
  }

  // How many times one run has been started. A process that keeps re-invoking its own turn writes
  // a start for each, so this count is how far a run has drifted from the one start an ordinary
  // one has — the whole of what tells a spin apart from work in progress.
  //
  // The agent comes first in the question because it is what the log is indexed by: asked with it
  // the answer takes a fraction of a millisecond, and asked by the run alone it is a scan of the
  // whole table. Asking for both costs the caller nothing: whoever launched a run holds the name
  // it was launched under and the unit it runs as, which are the two.
  async agentStartCount(agent: string, run: string): Promise<number> {
    const row = await this.sql.get("SELECT count(*) AS starts FROM agent_log WHERE agent = ? AND run = ? AND kind = 'start'", agent, run) as { starts: number };

    return row.starts;
  }

  // What the log may not keep: an agent's rows beyond the newest `keep` by id, and every row
  // anywhere older than `before`. A named agent sweeps its own rows off its own index, which is
  // what a writer does as it goes; null sweeps every agent at once, which is what the bot does
  // at start, for agents whose lines are there and whose writer is long gone.
  //
  // Past the count, an agent's rows also go oldest first once the text they hold together is over
  // `maxBytes`. None of the three ever takes an agent's newest system prompt line: it is the
  // version that agent's session runs on, and it is written again only when that version changes.
  //
  // Neither the count nor the bytes reaches a run that has not written its end. An agent's budget
  // is shared by every run it makes — every room's turns are the one agent the supervisor answers as — so a
  // run still going would otherwise lose its own opening lines to the rooms writing beside it, and
  // a reader asking for that run from its start would be handed a run that begins in the middle or
  // nothing at all. Such a run is neither counted nor taken, and what bounds it instead is the age:
  // a run still open a week later is not one anybody is waiting on.
  async pruneAgentLog(options: { agent: string | null; keep: number; maxBytes: number; before: number }): Promise<void> {
    return await this.sql.wrote(async () => {
      const scope = options.agent === null ? "" : "AND agent = ?";
      const bound = options.agent === null ? [] : [options.agent];
      const kept = `
        id NOT IN (
          SELECT MAX(id) FROM agent_log
          WHERE job_id IS NULL AND kind = 'prompt' AND name = 'system' ${scope}
          GROUP BY agent
        )
      `;

      // The runs still going, gathered once off the run index rather than asked again for every row
      // the sweep weighs: a run is going until it has written its end. A share of text with no bound
      // is weighed as the largest integer, which a sum compares with on either database.
      await this.sql.run(`
          WITH open AS (
            SELECT agent, run FROM agent_log
            WHERE job_id IS NULL ${scope}
            GROUP BY agent, run HAVING SUM(CASE WHEN kind = 'end' THEN 1 ELSE 0 END) = 0
          )
          DELETE FROM agent_log WHERE ${kept} AND id IN (
            SELECT id FROM (
              SELECT
                id,
                ROW_NUMBER() OVER (PARTITION BY agent ORDER BY id DESC) AS place,
                SUM(length(text) + COALESCE(length(detail), 0)) OVER (
                  PARTITION BY agent ORDER BY id DESC ROWS UNBOUNDED PRECEDING
                ) AS held
              FROM agent_log AS line
              WHERE job_id IS NULL ${scope} AND NOT EXISTS (
                SELECT 1 FROM open WHERE open.agent = line.agent AND open.run = line.run
              )
            ) AS weighed WHERE place > ? OR held > ?
          )
        `, ...bound, ...bound, ...bound, options.keep, Math.min(options.maxBytes, Number.MAX_SAFE_INTEGER));

      await this.sql.run(`DELETE FROM agent_log WHERE at < ? AND job_id IS NULL AND ${kept}`, options.before, ...bound);
    });
  }

  // The last tool one run of an agent was seen calling. A run that died wrote its lines as they
  // happened, so the newest of them is what it was doing when it stopped — which is the whole of
  // what anybody can honestly say it was up to.
  async lastAgentTool(agent: string, run: string): Promise<{ name: string | null; text: string } | null> {
    const row = await this.sql.get("SELECT name, text FROM agent_log WHERE agent = ? AND run = ? AND kind = 'tool' ORDER BY id DESC LIMIT 1", await this.agents.canonicalAgent(agent), run) as { name: string | null; text: string } | null;

    return row ?? null;
  }

  // When an agent's log last took a line for one room, in epoch milliseconds, or null where it took none: where a
  // screen opens that agent's session in the room.
  async latestAgentLogAt(agent: string, chatId: number): Promise<number | null> {
    const row = await this.sql.get("SELECT MAX(at) AS at FROM agent_log WHERE agent = ? AND chat_id = ?", agent, chatId) as { at: number | null } | null;

    return row?.at ?? null;
  }

  // The ref of the newest prompt line an agent wrote under one name, which is where a line that
  // stands for a version keeps that version. Asked at every launch, and answered by the index of
  // prompt lines alone rather than by every line the agent ever wrote.
  async latestPromptRef(agent: string, name: string): Promise<string | null> {
    const row = await this.sql.get("SELECT ref FROM agent_log WHERE agent = ? AND kind = 'prompt' AND name = ? ORDER BY id DESC LIMIT 1", agent, name) as { ref: string | null } | null;

    return row?.ref ?? null;
  }
}
