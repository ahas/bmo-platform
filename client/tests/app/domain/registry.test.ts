import { describe, expect, test } from "bun:test";

import { wording } from "../../../apps/web/shared/wording";
import { mappingText } from "../../../apps/web/app/domain/agent-settings";
import { emptyRegistrationForm, gatewayLine, keyStateLabel, pinValue, refusalPlace, registrationFigures, registrationForm, registryPinChoices, registrySaves } from "../../../apps/web/app/domain/registry";

import type { RegistrationView } from "../../../apps/web/shared/agency";
import KO from "../../fixtures/app/domain/registry.test.ko.json";

// The registered models as the agents screen reads and changes them (the agency's docs/own-llm.md, 「Registered
// models」). A key is one word on the screen, typed once into the box and never read back.

const t = wording;

const registration: RegistrationView = {
  name: "model-a-0-1",
  label: "Model A 0.1",
  model: "RVN-model",
  contextTokens: 1_048_576,
  answerTokens: 4_096,
  waitSeconds: 2_400,
  thinking: false,
  walked: false,
  keepsResearch: true,
  addresses: [
    { node: "alpha", url: "http://127.0.0.1:18021/v1/chat/completions", key: "usable" },
    { node: "bravo", url: "http://127.0.0.1:18023/v1/chat/completions", key: "none" },
  ],
  referrers: [KO.line_28],
};

describe("what a row says", () => {
  test("a key is one word, and the figures carry their separators and units", () => {
    expect(["usable", "none", "unusable"].map((state) => keyStateLabel(t, state as "usable"))).toEqual([wording("agents.label.key_usable"), KO.line_33, wording("agents.label.key_unusable")]);
    expect(registrationFigures(t, registration)).toBe(KO.line_34);
    expect(registrationFigures(t, { ...registration, thinking: true, walked: true, keepsResearch: false })).toBe(KO.line_35);
  });

  test("the gateway is spoken of only while it starts a new table", () => {
    expect(gatewayLine(t, null)).toBe("");
    expect(gatewayLine(t, { serving: "a", starting: null, draining: 1, at: 1 })).toBe("");
    expect(gatewayLine(t, { serving: "a", starting: "b", draining: 0, at: 1 })).toBe(wording("agents.status.gateway_starting"));
  });

  test("a tier cell of the registry leaves thinking to the registration where it names none", () => {
    const mapping = { provider: "registered", model: "model-a-0-1", effort: null, answerTokens: null };

    expect(mappingText(t, mapping, true, true)?.detail).toBe(`${wording("common.label.default")}${KO.line_47}`);
    expect(mappingText(t, { ...mapping, effort: "no-thinking" }, true, true)?.detail).toContain(KO.line_48);
    expect(mappingText(t, mapping, true)?.detail).toContain(KO.line_49);
  });
});

describe("what a save of the box asks", () => {
  test("the registration first, every address with a key only where one was typed, and each address let go removed", () => {
    const form = registrationForm(registration);

    form.label = "";
    form.addresses[0]!.key = "  new-key  ";
    form.addresses[1]!.url = "http://127.0.0.1:18024/v1/chat/completions";
    form.addresses.push({ node: "charlie", url: "http://127.0.0.1:18024/v1/chat/completions", key: "", clearKey: false, held: null });

    expect(registrySaves(form, registration)).toEqual([
      { action: "save", name: "model-a-0-1", label: "model-a-0-1", model: "RVN-model", contextTokens: 1_048_576, answerTokens: 4_096, waitSeconds: 2_400, thinking: false, walked: false, keepsResearch: true },
      { action: "set-address", name: "model-a-0-1", node: "alpha", url: "http://127.0.0.1:18021/v1/chat/completions", key: "new-key" },
      { action: "set-address", name: "model-a-0-1", node: "bravo", url: "http://127.0.0.1:18024/v1/chat/completions" },
      { action: "set-address", name: "model-a-0-1", node: "charlie", url: "http://127.0.0.1:18024/v1/chat/completions" },
    ]);

    const dropped = registrationForm(registration);

    dropped.addresses = [dropped.addresses[0]!];
    dropped.addresses[0]!.clearKey = true;

    expect(registrySaves(dropped, registration).slice(1)).toEqual([
      { action: "set-address", name: "model-a-0-1", node: "alpha", url: "http://127.0.0.1:18021/v1/chat/completions", key: null },
      { action: "remove-address", name: "model-a-0-1", node: "bravo" },
    ]);
  });

  test("a new box holds one empty address, and a refusal stands under the field the bot named", () => {
    expect(emptyRegistrationForm().addresses).toHaveLength(1);
    expect(refusalPlace("url")).toBe("url");
    expect(refusalPlace("action")).toBe("form");
    expect(refusalPlace(null)).toBe("form");
  });
});

describe("a worker's pin", () => {
  test("offers every registration with an address as a pin of its own, and reads one back", () => {
    expect(registryPinChoices([registration, { ...registration, name: "bare", label: "Bare", addresses: [] }])).toEqual([{ label: "Model A 0.1", value: "registered::model-a-0-1" }]);
    expect(pinValue("registered", "model-a-0-1", "auto")).toBe("registered::model-a-0-1");
    expect(pinValue("registered", null, "auto")).toBe("registered");
    expect(pinValue(null, null, "auto")).toBe("auto");
  });
});
