// The prefix a worker session is opened on, and what a change to it asks for.
//
// What goes into it: the CLI's own instructions, the CLAUDE.md chain from the working directory
// up to the home directory and whatever those files import, less what the settings exclude, the
// rules appended to the CLI's own prompt, the settings handed over with the launch line, the
// skills the line's plugin declares, the co-worker definitions the line carries whole, and the MCP
// servers it names. What arrives in
// the turn is no part of it — the item's own text and the report trailer are the job's prompt,
// not its prefix — and neither is anything a run reads for itself once it is going.
//
// A prompt cache matches a byte-identical prefix, so one character anywhere in that assembly
// ends the cache behind every worker session at once. There is no small change and no large one:
// what a change decides is only how much the session was still holding. Folding is what keeps
// it — the CLI's own summariser, run on that same session — because the fold pays the one
// re-read the change had already cost and leaves a small session warm behind it, where dropping
// the record leaves the next job to open a cold one and keeping it as it stands leaves every job
// after it to read the whole thing again.
//
// Where the line is between a session worth folding and one better opened fresh: the summariser's
// own answer. A session with too little in it to fold says so and nothing happens, and the run
// that asked cost as little as the session it was asking about — the smallness that makes the
// fold pointless is the same smallness that makes asking cheap. So nothing here measures a
// session before folding it.
//
// Two things say the prefix moved. The paths below are the repository's own share of it, which a
// commit can name, and a job that committed one of them folds its own session as it ends. The
// digest further down is read off the machine instead, and catches what no commit here can — a
// deployment somebody else ran, an edit by hand to a document outside every checkout, a change to
// the launch line itself.

// ---------------------------------------------------------------- the prefix as it stands now

// A rule alone leaks, and so does a commit. Neither can see a deployment somebody else ran, an
// edit made by hand to a document that lives outside every checkout, or a change to the launch
// line that opens the session. So the prefix is also read straight off the machine and reduced to
// one short word, and that word goes on the session's own row when the session answers. A session
// whose word no longer matches the prefix as it stands now was opened on something that is gone.
//
// What is read is what the CLI is actually given: the launch line itself, which carries the
// co-worker definitions whole; the skills the plugin declares, by the front matter that reaches
// the listing and not by the rest of a document nothing reads until a run asks for it; the
// settings handed over with the line; the rules appended to the CLI's own prompt; the MCP servers
// it names; and every CLAUDE.md from the working directory up to the home directory that the
// settings do not exclude, together with the documents those import. Nothing here
// reproduces the CLI's own instructions, which no file on this machine holds, so a new CLI
// version is the one move this cannot see.
//
// Absolute paths come out of the launch line before it is read. The bot records the word from the
// deployed tree and a shell asks it from the checkout; those two differ by their directories
// alone, and leaving them in would make every comparison say the prefix had moved.

// The part of a document the CLI reads before anything asks for the rest.
export function frontMatter(text: string): string {
  const body = text.replaceAll("\r\n", "\n");
  const match = /^---\n([\s\S]*?)\n---\n/.exec(body);

  return match === null ? body : (match[1] as string);
}

