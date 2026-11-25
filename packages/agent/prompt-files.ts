import { mkdirSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { agentsRoot } from "@agency/support/roots";

// A system prompt is handed to the CLI as a file under the agents' root, never on the command
// line: a chat's runs tens of kilobytes and a process list is no place for it. The file is
// written fresh for every launch, so it is always the current composition.
//
// Runs of one line start side by side — rooms answering at once, a search's branches opened
// together — and each rewrites the file another may be reading that moment. So the text is written
// beside it and moved into place whole: a reader sees the old file or the new one, never a file cut
// short.
export function systemPromptFile(name: string, text: string): string {
  const dir = join(agentsRoot(), "prompts");
  const path = join(dir, `${name}.md`);
  const staged = join(dir, `.${name}.md.${process.pid}.${Math.random().toString(36).slice(2)}`);

  mkdirSync(dir, { recursive: true });
  writeFileSync(staged, text.endsWith("\n") ? text : `${text}\n`);
  renameSync(staged, path);

  return path;
}
