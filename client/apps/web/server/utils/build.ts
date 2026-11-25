// Whether this console is served from a build rather than from source. It is the one difference
// that is already true between the deployed console and a development console standing beside it,
// needing nothing said on either side, and what each console must do differently from the other is
// decided by it: which one rings (docs/web.md, Notifications) and which one may read the real
// mailboxes without being pointed at them (the mail unit's docs/mail.md). Only a plain yes to "is this a development
// server" reads as source; anything else, an answer the runtime does not give included, is a build.
export function builtConsole(dev: unknown): boolean {
  return dev !== true;
}
