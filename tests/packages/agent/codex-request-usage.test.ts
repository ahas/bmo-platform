import "@agency/test-support/setup";

import { expect, test } from "bun:test";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { scratchDir } from "@agency/test-support/scratch";
import { codexTurnRequestUsages } from "@agency/agent/codex-request-usage";
const root = scratchDir("bmo-codex-request-");
const count = (input: number, cached: number, output: number) => JSON.stringify({ type:"event_msg", payload:{ type:"token_count", info:{ total_token_usage:{input_tokens:32051126,cached_input_tokens:31054976}, last_token_usage:{input_tokens:input,cached_input_tokens:cached,output_tokens:output} } } });
const started = JSON.stringify({ type:"event_msg", payload:{ type:"task_started", turn_id:"t" } });
test("siren uses individual requests instead of a whole job's aggregate", () => {
  writeFileSync(join(root,"rollout-thread-id.jsonl"), count(128340,125952,740)+"\n");
  expect(codexTurnRequestUsages("thread-id",root)).toEqual([{inputTokens:2388,cacheReadTokens:125952,cacheWriteTokens:0,outputTokens:740}]);
  expect(codexTurnRequestUsages("missing",root)).toBeNull();
  expect(codexTurnRequestUsages("../thread-id",root)).toBeNull();
});
// The CLI compacts its thread in the middle of a turn: a zero count marks the point and the next
// request re-reads the summary. That request is what the siren has to be shown.
test("a turn's requests are read from its start, across a compaction, oldest first", () => {
  const compacted = JSON.stringify({ type:"compacted", payload:{ message:"", replacement_history:[] } });
  writeFileSync(join(root,"rollout-compacting.jsonl"), [started, count(1000,0,1), started, count(216111,202880,5), compacted, count(0,0,0), count(80847,0,7), count(85480,80640,9)].join("\n")+"\n");
  expect(codexTurnRequestUsages("compacting",root)).toEqual([
    {inputTokens:13231,cacheReadTokens:202880,cacheWriteTokens:0,outputTokens:5},
    {inputTokens:80847,cacheReadTokens:0,cacheWriteTokens:0,outputTokens:7},
    {inputTokens:4840,cacheReadTokens:80640,cacheWriteTokens:0,outputTokens:9},
  ]);
});
// A rollout is read backwards a mebibyte at a time; a line cut by that boundary is read whole.
test("a line split across the backward read is read whole", () => {
  const padding = JSON.stringify({ type:"response_item", payload:{ type:"message", content:[{ text:"€".repeat(700_000) }] } });
  writeFileSync(join(root,"rollout-long.jsonl"), [started, count(500,100,1), padding, count(900,800,2)].join("\n")+"\n");
  expect(codexTurnRequestUsages("long",root)).toEqual([
    {inputTokens:400,cacheReadTokens:100,cacheWriteTokens:0,outputTokens:1},
    {inputTokens:100,cacheReadTokens:800,cacheWriteTokens:0,outputTokens:2},
  ]);
});
