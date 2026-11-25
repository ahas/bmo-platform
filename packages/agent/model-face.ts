// The name a model reads under where a person reads it, worked out from the identifier itself.
//
// It is a rule and not a list. A list would have to be written to every time a model is named, and
// which models agency may run is no longer written down anywhere — it is read from the catalogue the
// CLI keeps, at the moment it is asked (`docs/model-tiers.md`, 「The Claude catalogue」). A name
// held in code would then be the one place left where a new model still waited for a build, which
// is the whole of what that reading was for. So what is written down here is the shape of a Claude
// identifier rather than any of the identifiers: the family, the generation, and — on a model
// pinned to one build — the date that build was cut, which is not part of the name.
//
// An identifier this cannot read stands under the name it came with. Nothing is ever hidden behind
// a guess, and a provider whose models carry names no rule could derive — a product name rather
// than a family and a number — names them where its own screens do.

// `claude-opus-5`, `claude-opus-5-5`, `claude-haiku-4-5-20251001`.
const CLAUDE_MODEL = /^claude-([a-z]+)((?:-\d+)+)$/;

// The eight digits a pinned build carries. They say when that build was cut rather than which
// generation it is, so they are left off.
const BUILD_DATE = /^\d{8}$/;

export function modelFace(model: string): string {
  const named = CLAUDE_MODEL.exec(model);

  if (named === null) {
    return model;
  }

  const [, family = "", generation = ""] = named;
  const parts = generation.slice(1).split("-").filter((part) => !BUILD_DATE.test(part));

  return parts.length === 0 ? model : `${family.charAt(0).toUpperCase()}${family.slice(1)} ${parts.join(".")}`;
}
