// Where another unit is reached (docs/boundary.md, 「Settings」): each by the base address and the token its
// settings name, `<NAME>_ADDRESS` and `<NAME>_TOKEN` in the console's environment. A caller names the unit it
// calls; a unit whose address is not set is none, and the screen that needs it stands as for a unit not yet
// split, making no call.

/** A unit's base address, without a trailing slash, and the token it is called with where it takes one. */
export interface UnitAddress {
  address: string;
  token: string | null;
}

/** The unit the environment names under `name`, such as `AGENCY` or `CACHE_SIREN`, or null where none is set. */
export function unitAt(name: string, environment: Record<string, string | undefined> = process.env): UnitAddress | null {
  const address = environment[`${name}_ADDRESS`]?.trim() ?? "";

  if (address === "") return null;

  const token = environment[`${name}_TOKEN`]?.trim() ?? "";

  return { address: address.replace(/\/+$/, ""), token: token === "" ? null : token };
}
