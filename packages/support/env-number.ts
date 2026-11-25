// A positive number the environment gave for a setting, or the fallback where it gave none that
// reads as one. The waits a test shortens through the environment each read theirs through this,
// so an empty, zero, negative or mistyped value always means the module's own constant.
export function positiveFromEnv(given: string | undefined, fallback: number): number {
  const value = Number.parseInt(given ?? "", 10);

  return Number.isFinite(value) && value > 0 ? value : fallback;
}
