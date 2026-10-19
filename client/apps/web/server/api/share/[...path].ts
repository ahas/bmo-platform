import { wording } from "#shared/wording";

import { artifactAsk, artifactPath, forwardToArtifact } from "../../utils/artifact";

// The share page's roads, handed on whole to bmo-artifact with no sign-in: the token in the road is the whole of the
// permission, and every road under it only reads.
export default defineEventHandler(async (event) => {
  const method = getMethod(event).toUpperCase();

  if (method !== "GET" && method !== "HEAD") {
    throw fail(405, wording("common.error.read_only_road"));
  }

  return sendWebResponse(event, await forwardToArtifact(await artifactAsk(event, artifactPath("/api/share", getRouterParam(event, "path")), null)));
});
