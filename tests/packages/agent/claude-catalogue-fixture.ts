import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { scratchDir } from "@agency/test-support/scratch";

// A Claude Code catalogue of a test's own. What a Claude model accepts is read from the catalogue
// the CLI keeps for the login a run goes out on, so a test asking that question is otherwise
// answered by whatever this machine's own login last fetched — an answer that changes under the
// test as Anthropic changes what it offers. A test that cares names the models itself here.

/** One model as a test wants the catalogue to name it. */
export interface StoodCatalogueModel {
  id: string;
  // Whether the CLI serves this generation or has superseded it. Served unless the test says not.
  inService?: boolean;
  // The efforts its entry names. Left out, the entry names none at all, which is what a model that
  // does not think looks like.
  efforts?: readonly string[];
}

export interface StoodCatalogue {
  // The configuration directory the catalogue stands in, for a test that must point a login at it.
  configDir: string;
  close: () => void;
}

/**
 * A catalogue naming these models, stood in a configuration directory of its own and pointed at by
 * the variable the CLI itself reads. It answers every question put to the machine's own login,
 * which is what a test asks where it installs no store of logins; a test that installs one points
 * its account at `configDir`. `close` gives the directory and the variable back; the directory is
 * the one door's, so a test that never closes still leaves nothing behind.
 */
export function standClaudeCatalogue(
  models: readonly StoodCatalogueModel[],
  moments: { fetchedAt?: number; staleAt?: number } = {},
): StoodCatalogue {
  const configDir = scratchDir("bmo-claude-catalogue-");
  const dir = join(configDir, "cache", "model-catalog");
  const fetchedAt = moments.fetchedAt ?? Date.now();
  const was = process.env.CLAUDE_CONFIG_DIR;

  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "stood-cc.json"), JSON.stringify({
    version: 2,
    fetchedAt,
    staleAt: moments.staleAt ?? fetchedAt + 3_600_000,
    catalog: {
      surface: "cc",
      config: {
        id: "cc",
        models: models.map((model) => ({
          id: model.id,
          section: model.inService === false ? "overflow" : "main",
          ...(model.efforts === undefined ? {} : { thinking: { type: "effort", effort_options: model.efforts.map((id) => ({ id })) } }),
        })),
      },
    },
  }));

  process.env.CLAUDE_CONFIG_DIR = configDir;

  return {
    configDir,
    close: () => {
      if (was === undefined) delete process.env.CLAUDE_CONFIG_DIR;
      else process.env.CLAUDE_CONFIG_DIR = was;

      rmSync(configDir, { recursive: true, force: true });
    },
  };
}
