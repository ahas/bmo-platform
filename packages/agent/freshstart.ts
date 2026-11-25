// A session handed over to a new one (docs/sessions.md, 「Session replacement」) is agency's server's; what
// stays here is where the CLI keeps a session's record.

// Where the CLI keeps a session's records is the agent base's reading of the CLI (`cli-transcripts.ts`).
export { cliProjectsRoot, projectDir, transcriptPath } from "./cli-transcripts";

