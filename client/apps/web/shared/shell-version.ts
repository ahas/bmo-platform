// An address of the installed app's shell carrying the shell version (client/modules/shell-version.ts),
// so the same file redrawn is a file at another address to every cache that kept the old one.
export function versioned(address: string, version: string): string {
  return `${address}${address.includes("?") ? "&" : "?"}v=${version}`;
}
