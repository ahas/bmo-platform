// The palette the logged-in account's bubbles are drawn in.
export default defineEventHandler(async (event): Promise<{ palette: string }> => {
  const signed = await signedInUser(event);

  return { palette: await readPalette(signed.name) };
});
