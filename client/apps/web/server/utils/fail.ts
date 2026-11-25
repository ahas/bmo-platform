// h3 strips non-ASCII from a status message, so the Korean reason travels in the body's message.
export function fail(statusCode: number, reason: string): Error {
  return createError({ statusCode, message: reason });
}
