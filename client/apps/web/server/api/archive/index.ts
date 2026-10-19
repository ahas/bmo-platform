import { artifactAsk, artifactPath, forwardToArtifact } from "../../utils/artifact";

// The archive's list, bmo-artifact's `GET /api/archive`, handed on whole behind the console's sign-in.
export default defineEventHandler(async (event) => {
  const session = await requireUserSession(event);

  return sendWebResponse(event, await forwardToArtifact(await artifactAsk(event, artifactPath("/api/archive", undefined), session.user.name)));
});
