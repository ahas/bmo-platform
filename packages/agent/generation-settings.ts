// The generation settings a supervisor's turns are asked with (docs/supervisors.md, 「Its generation
// settings」): each by the name the engine's chat completions path reads it under, a number with its
// bounds or a switch. One left empty is the engine's own default. `max_tokens` is the run's answer
// ceiling; every other one is the run's sampling, which the gateway writes into the request the engine
// reads (docs/own-llm.md, 「The gateway」).

import declaredGeneration from "@agency/policy/generation.toml";
import controlWords from "@agency/policy/control-words.toml";
import { fillMarks } from "@agency/support/marks";

import type { HarnessRun, Sampling } from "./harness";

interface SettingBase {
  name: string;
  // Whether the chat template reads it, so the request carries it under `chat_template_kwargs`.
  template?: true;
}

export interface NumberSetting extends SettingBase {
  switch?: undefined;
  min: number;
  // The highest value, or null where the registration's window sets it.
  max: number | null;
  // Whether the lowest value itself is refused, as `top_p` 0 is.
  aboveMin?: boolean;
  // Whether it takes whole numbers only.
  whole?: boolean;
}

// A setting that takes on or off, with no bounds.
export interface SwitchSetting extends SettingBase {
  switch: true;
}

export type GenerationSetting = NumberSetting | SwitchSetting;

// The settings and their bounds are the shared policy's (`generation.toml`); a flag the policy leaves
// out is left out here too.
interface DeclaredSetting {
  name: string;
  min?: number;
  max?: number;
  above_min?: boolean;
  whole?: boolean;
  switch?: boolean;
  template?: boolean;
}

function settingOf(one: DeclaredSetting): GenerationSetting {
  const template = one.template === true ? { template: true as const } : {};

  if (one.switch === true) return { name: one.name, switch: true, ...template };

  return {
    name: one.name,
    min: one.min ?? 0,
    max: one.max ?? null,
    ...(one.above_min === true ? { aboveMin: true } : {}),
    ...(one.whole === true ? { whole: true } : {}),
    ...template,
  };
}

export const generationSettings: readonly GenerationSetting[] = (declaredGeneration.settings as DeclaredSetting[]).map(settingOf);

// The setting that is the run's answer ceiling rather than its sampling.
const answerCeiling = "max_tokens";

// Where a request carries the settings its chat template reads.
const templateField = "chat_template_kwargs";

export type GenerationValue = number | boolean;
export type GenerationValues = Readonly<Record<string, GenerationValue>>;

export function generationSetting(name: string): GenerationSetting | null {
  return generationSettings.find((one) => one.name === name) ?? null;
}

/**
 * Why a value is not one the setting takes, as one sentence in the policy's words, or null where it takes it. The
 * window is the registration's, which `max_tokens` stays under; null where none is known.
 */
export function generationRefusal(setting: GenerationSetting, value: unknown, window: number | null): string | null {
  if (setting.switch === true) return typeof value === "boolean" ? null : fillMarks(controlWords.generation.switch, { name: setting.name });

  const max = setting.max ?? (window === null ? null : window - 1);
  const least = setting.aboveMin === true ? fillMarks(controlWords.generation.above, { min: String(setting.min) }) : fillMarks(controlWords.generation.least, { min: setting.min.toLocaleString("en-US") });
  const bounds = max === null ? least : fillMarks(controlWords.generation.most, { least, max: max.toLocaleString("en-US") });

  if (typeof value !== "number" || !Number.isFinite(value)) return fillMarks(controlWords.generation.number, { name: setting.name });
  if (setting.whole === true && !Number.isInteger(value)) return fillMarks(controlWords.generation.whole, { name: setting.name });
  if (value < setting.min || (setting.aboveMin === true && value === setting.min) || (max !== null && value > max)) {
    return fillMarks(controlWords.generation.range, { name: setting.name, span: bounds });
  }

  return null;
}

// What a run is handed of the settings: the answer ceiling where they name one, and the rest as its
// sampling where they name any, a setting the chat template reads under the template's own field.
export function generationRun(values: GenerationValues): Pick<HarnessRun, "answerTokens" | "sampling"> {
  const { [answerCeiling]: ceiling, ...rest } = values;
  const named = Object.entries(rest);
  const template = Object.fromEntries(named.filter(([name]) => generationSetting(name)?.template === true));
  const sampling: Sampling = {
    ...Object.fromEntries(named.filter((entry): entry is [string, number] => generationSetting(entry[0])?.template !== true && typeof entry[1] === "number")),
    ...(Object.keys(template).length === 0 ? {} : { [templateField]: template }),
  };

  return {
    ...(typeof ceiling === "number" ? { answerTokens: ceiling } : {}),
    ...(Object.keys(sampling).length === 0 ? {} : { sampling }),
  };
}

// What a run is handed of a worker's own sampling: the settings it names, the answer ceiling left out.
export function samplingRun(values: GenerationValues): Pick<HarnessRun, "sampling"> {
  const { sampling } = generationRun(Object.fromEntries(Object.entries(values).filter(([name]) => name !== answerCeiling)));

  return sampling === undefined ? {} : { sampling };
}
