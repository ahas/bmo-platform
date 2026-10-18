import { countText } from "./numbers";

import type { LayaAgreement, LayaGateView, LayaMode, LayaModeRefusal, LayaView } from "#shared/types";
import type { Translate } from "#shared/wording";
import type { Ref } from "vue";

// Laya on the agent management screen and a gate's own screen (docs/web.md, 「Agent management screen」): the server's last
// health check and each gate's agreement with its LLM. Both are the gate unit's, which is not split yet and is
// reached at no address, so the view stands as Laya not reached and no call is made (docs/boundary.md, 「Screens
// of units not yet split」); this decides only how they read.

export function emptyLayaView(t: Translate): LayaView {
  return {
    health: {
      state: "off", stateWord: t("agents.status.laya_unavailable"), tone: "unknown", node: null, model: null, device: null, vramBytes: null,
      loadMs: null, versions: {}, address: null, checkMs: null, checkedAt: null, healthyAt: null,
    },
    gates: [],
    modes: [],
    taus: [],
  };
}

// The view as it stands while the gate unit is reached at no address: Laya not reached, and no gate of its.
export function useLaya(t: Translate): { laya: Ref<LayaView>; status: Ref<string>; refresh: () => Promise<void> } {
  return { laya: ref(emptyLayaView(t)), status: ref("success"), refresh: async () => {} };
}

export function percentText(share: number | null): string {
  return share === null ? "—" : `${(share * 100).toFixed(1)}%`;
}

export function tauText(t: Translate, tau: number | null): string {
  return tau === null ? t("common.label.none") : tau.toFixed(2);
}

export function layaModeLabel(view: LayaView, mode: LayaMode): string {
  return view.modes.find((one) => one.value === mode)?.label ?? mode;
}

// A gate's own line of the view: the mode and τ it runs on and its agreement, or null for a gate that
// has no Laya probe.
export function layaGateOf(view: LayaView, stage: string): LayaGateView | null {
  return view.gates.find((gate) => gate.stage === stage) ?? null;
}

export function agreementOf(view: LayaView, stage: string): LayaAgreement | null {
  return layaGateOf(view, stage)?.agreement ?? null;
}

// A gate's agreement where a cell shows it: the share over every pair, or a dash where none is recorded.
export function agreementText(agreement: LayaAgreement | null): string {
  return agreement === null || agreement.pairs === 0 ? "—" : percentText(agreement.agreement);
}

// What stands under it: how many pairs the share rests on — questions, for a gate Laya settles one
// question at a time, with the share of them it settled — and, at the gate's τ, the share Laya
// would settle and how often it agrees there.
export function agreementNote(t: Translate, agreement: LayaAgreement | null, tau: number | null): string {
  if (agreement === null) return t("common.label.no_record");

  const counted = agreement.unit === "question" ? "agents.label.laya_questions" : "agents.label.laya_pairs";
  const parts = [t(counted, { n: countText(agreement.pairs) })];

  if (agreement.failures > 0) parts.push(t("agents.label.laya_failures", { n: countText(agreement.failures) }));

  if (agreement.settled !== null) parts.push(t("agents.label.laya_settled", { share: percentText(agreement.settled) }));

  const at = tau === null ? undefined : agreement.byTau.find((one) => one.tau === tau);

  if (at !== undefined && at.coverage !== null) parts.push(
      t("agents.label.laya_at_tau", { tau: tauText(t, tau), coverage: percentText(at.coverage), agreement: percentText(at.agreement) })
    );

  return parts.join(" · ");
}

// Why the bot would not put a gate in a mode, as the mode's list says it.
export function layaRefusalText(t: Translate, why: LayaModeRefusal): string {
  const keys: Record<LayaModeRefusal, string> = {
    "no-probe": "agents.label.laya_refused_no_probe",
    "no-safe-verdict": "agents.label.laya_refused_no_safe_verdict",
    "no-checkpoint": "agents.label.laya_refused_no_checkpoint",
  };

  return t(keys[why]);
}

export function vramText(bytes: number | null): string {
  return bytes === null ? "" : `${(bytes / 1024 ** 3).toLocaleString("ko-KR", { maximumFractionDigits: 1, minimumFractionDigits: 1 })} GB`;
}

// The model's last name, which is what a line has room for.
export function modelShortName(model: string | null): string {
  if (model === null) return "";

  const parts = model.split("/").filter(Boolean);

  return parts.at(-1) ?? model;
}
