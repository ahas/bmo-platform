import { formatElapsed, modelLabel, modeLabel } from "./status";

import type { PoolBoardBucket, PoolBoardTile, PoolBoardWork } from "#shared/agency";
import type { StopTarget } from "#shared/types";
import type { Translate } from "#shared/wording";

// How the live status screen draws the co-worker pool: a tile to a place, told apart by what holds
// it (docs/web.md, 「Live status screen」).

export const TILE_STATE_LABELS: Record<PoolBoardTile["state"], string> = {
  working: "live.label.tile_working",
  held: "live.label.tile_held",
  idle: "live.label.tile_idle"
};

export const TILE_STATE_ICONS: Record<PoolBoardTile["state"], string> = {
  working: "i-material-symbols-progress-activity",
  held: "i-material-symbols-bookmark-outline",
  idle: "i-material-symbols-bedtime-outline"
};

const KIND_LABELS: Record<PoolBoardWork["kind"], string> = {
  cowork: "live.label.work_kind_cowork",
  search: "live.label.work_kind_search",
  browse: "live.label.work_kind_browse",
  native: "live.label.work_kind_native",
  turn: "live.label.work_kind_turn",
  other: "live.label.work_kind_other"
};

// The head of a tier's row: where it is, how many of its places are taken and how many wait.
export function bucketHead(t: Translate, bucket: PoolBoardBucket): string {
  const used = bucket.tiles.filter((tile) => tile.state !== "idle").length;
  const figures = {
    mode: modeLabel(t, bucket.provider),
    tier: bucket.tier,
    used: used.toLocaleString("ko-KR"),
    capacity: bucket.capacity.toLocaleString("ko-KR")
  };
  const head = t("live.label.bucket_head", figures);

  return bucket.waiting.length > 0 ? t("live.label.bucket_waiting", { head, n: bucket.waiting.length.toLocaleString("ko-KR") }) : head;
}

// The lines a tile's tooltip carries: its state, what it serves, the role and where it runs.
// What a stop on a tile is aimed at. A co-worker has no reach of its own, so what the tile names is
// the run it works inside — its parent's own target, put there by the bot (`the agency's docs/agent-control.md`
// 「Stopping」). A place held for work that has not started names nothing.
export function tileStop(tile: PoolBoardTile): StopTarget | null {
  return tile.work?.stop ?? null;
}

export function tileLines(t: Translate, tile: PoolBoardTile, now: number): string[] {
  const work = tile.work;

  if (work === null) return [t(TILE_STATE_LABELS[tile.state])];

  const model = work.model === "" ? null : [modelLabel(work.model), work.effort].filter(Boolean).join(" ");

  return [
    `${t(TILE_STATE_LABELS[tile.state])} · ${t(KIND_LABELS[work.kind])}`,
    work.title,
    ...(work.detail === null ? [] : [work.detail]),
    `${work.role} · ${work.tier}`,
    [modeLabel(t, work.provider), model].filter(Boolean).join(" · "),
    ...(work.since === null ? [] : [t("live.label.elapsed", { time: formatElapsed(t, (now - work.since) / 1000) })])
  ];
}

// The short line a busy tile wears under its role.
export function tileCaption(work: PoolBoardWork): string {
  return work.detail === null ? work.title : `${work.title} · ${work.detail}`;
}
