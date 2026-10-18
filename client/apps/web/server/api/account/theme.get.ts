// The theme the logged-in account reads the console in.
export default defineEventHandler(async (event): Promise<{ theme: string }> => {
  const signed = await signedInUser(event);

  return { theme: await readTheme(signed.name) };
});
