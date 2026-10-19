import { artifactAsk, artifactPath, forwardToArtifact } from "../../utils/artifact";

// Every road under the archive — a document, its file, its attachments, its sources' previews and pictures, and its
// share link — handed on whole to bmo-artifact behind the console's sign-in; a link made here is made by the
// signed-in user.
export default defineEventHandler(async (event) => {
  const session = await requireUserSession(event);
  const path = artifactPath("/api/archive", getRouterParam(event, "path"));

  return sendWebResponse(event, await forwardToArtifact(await artifactAsk(event, path, session.user.name)));
});
