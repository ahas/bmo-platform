import { countText } from "./numbers";

import type { GatewayView, KeyState, RegistrationView } from "#shared/agency";
import type { Translate } from "#shared/wording";

// The registered models as the screen reads and changes them (the agency's docs/own-llm.md, 「Registered
// models」). What a row says of a key is one word — usable, none, or named but not usable — since
// nothing the bot answers with carries a key; a key goes from the box to the bot once and is never
// shown again.

// The provider every registration is chosen under, where a worker is pinned to one.
export const REGISTRY_PROVIDER = "registered";

export function keyStateLabel(t: Translate, state: KeyState): string {
  return t(state === "usable" ? "agents.label.key_usable" : state === "none" ? "agents.label.key_none" : "agents.label.key_unusable");
}

export function keyStateMark(state: KeyState): { icon: string; tone: string } {
  return state === "usable"
    ? { icon: "i-material-symbols-key-outline", tone: "text-success" }
    : state === "none"
      ? { icon: "i-material-symbols-key-off-outline", tone: "text-muted" }
      : { icon: "i-material-symbols-warning-outline", tone: "text-warning" };
}

// What a row says of a registration beside its name: its window, its answer and its wait with their
// units, whether it thinks by default, whether the automatic walk asks it, and whether the research
// its runs set off runs on it.
export function registrationFigures(t: Translate, registration: Pick<RegistrationView, "contextTokens" | "answerTokens" | "waitSeconds" | "thinking" | "walked" | "keepsResearch">): string {
  return [
    t("agents.label.registry_context", { tokens: countText(registration.contextTokens) }),
    t("agents.label.registry_answer", { tokens: countText(registration.answerTokens) }),
    t("agents.label.registry_wait", { seconds: countText(registration.waitSeconds) }),
    t(registration.thinking ? "agents.label.registry_thinking_on" : "agents.label.registry_thinking_off"),
    ...(registration.walked ? [t("agents.label.registry_walked")] : []),
    ...(registration.keepsResearch ? [t("agents.label.registry_keeps_research")] : []),
  ].join(" · ");
}

// The line under the list while the gateway starts the table a save gave it; nothing once it serves.
export function gatewayLine(t: Translate, gateway: GatewayView | null): string {
  return gateway?.starting == null ? "" : t("agents.status.gateway_starting");
}

// One address as the box edits it. A key is typed only to give one; left empty, the address keeps
// the key it has, and `clearKey` takes it away.
export interface AddressForm {
  node: string;
  url: string;
  key: string;
  clearKey: boolean;
  // The key the address holds now, as the bot says it; null for an address not saved yet.
  held: KeyState | null;
}

export interface RegistrationForm {
  name: string;
  label: string;
  model: string;
  contextTokens: number | null;
  answerTokens: number | null;
  waitSeconds: number | null;
  thinking: boolean;
  walked: boolean;
  keepsResearch: boolean;
  addresses: AddressForm[];
}

export function emptyAddress(): AddressForm {
  return { node: "", url: "", key: "", clearKey: false, held: null };
}

export function emptyRegistrationForm(): RegistrationForm {
  return { name: "", label: "", model: "", contextTokens: null, answerTokens: 4_096, waitSeconds: 90, thinking: false, walked: false, keepsResearch: false, addresses: [emptyAddress()] };
}

export function registrationForm(registration: RegistrationView): RegistrationForm {
  return {
    name: registration.name,
    label: registration.label,
    model: registration.model,
    contextTokens: registration.contextTokens,
    answerTokens: registration.answerTokens,
    waitSeconds: registration.waitSeconds,
    thinking: registration.thinking,
    walked: registration.walked,
    keepsResearch: registration.keepsResearch,
    addresses: registration.addresses.map((one) => ({ node: one.node, url: one.url, key: "", clearKey: false, held: one.key })),
  };
}

// What a save of the box asks of the bot, in order: the registration itself, every address it
// holds — each carrying a key only where one was typed, or null where it was taken away — and the
// removal of every address the registration held that the box no longer does.
export function registrySaves(form: RegistrationForm, before: RegistrationView | null): Record<string, unknown>[] {
  const name = form.name.trim();
  const kept = new Set(form.addresses.map((one) => one.node.trim()));

  return [
    {
      action: "save",
      name,
      label: form.label.trim() === "" ? name : form.label.trim(),
      model: form.model.trim(),
      contextTokens: form.contextTokens,
      answerTokens: form.answerTokens,
      waitSeconds: form.waitSeconds,
      thinking: form.thinking,
      walked: form.walked,
      keepsResearch: form.keepsResearch,
    },
    ...form.addresses.map((one) => ({
      action: "set-address",
      name,
      node: one.node.trim(),
      url: one.url.trim(),
      ...(one.key.trim() !== "" ? { key: one.key.trim() } : one.clearKey ? { key: null } : {}),
    })),
    ...(before?.addresses ?? []).filter((one) => !kept.has(one.node)).map((one) => ({ action: "remove-address", name, node: one.node })),
  ];
}

// Where in the box a refusal stands: the field the bot named, or the box as a whole.
export function refusalPlace(field: string | null | undefined): "name" | "label" | "model" | "contextTokens" | "answerTokens" | "waitSeconds" | "node" | "url" | "key" | "form" {
  const places = ["name", "label", "model", "contextTokens", "answerTokens", "waitSeconds", "node", "url", "key"] as const;

  return places.find((one) => one === field) ?? "form";
}

// The registrations a worker may be pinned to, each chosen as `registered::<name>`: those with an
// address, which the bot would run.
export function registryPinChoices(registrations: readonly RegistrationView[]): { label: string; value: string }[] {
  return registrations.filter((one) => one.addresses.length > 0).map((one) => ({ label: one.label, value: `${REGISTRY_PROVIDER}::${one.name}` }));
}

// A worker's pin as the dialog holds it: the provider, or the provider and the registration it names.
export function pinValue(provider: string | null | undefined, model: string | null | undefined, auto: string): string {
  if (provider === null || provider === undefined) return auto;

  return model === null || model === undefined ? provider : `${provider}::${model}`;
}
