// The icon each of the deployment's own tools declares, by the tool's own name. The tool host hands the console no
// list of them (docs/boundary.md, 「Gaps the console meets」), so every call takes the generic icon.
export default defineEventHandler(async (event): Promise<Record<string, string>> => {
  await requireUserSession(event);

  return {};
});
