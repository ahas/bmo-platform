// Which screen an address is, as the router is told it (docs/web.md, 「A list is found as it was left」).
// A screen is built anew whenever this changes, which takes down everything it holds — rows read,
// a scroll, a box being typed in. So it is the screen's path and only the parts of the query the
// screen itself reads. The rest of the query belongs to what stands around the screen — an item
// opened on the TODO panel writes `?todo=` into whatever address is open — and changing it leaves
// the screen standing as it was.

export interface KeyedRoute {
  path: string;
  query: Record<string, unknown>;
}

// The screen's path with the query parts it names, in the order it names them.
export function screenKey(route: KeyedRoute, reads: readonly string[]): string {
  const kept = reads.flatMap((part) => {
    const value = route.query[part];

    return value === undefined || value === null ? [] : [`${part}=${String(value)}`];
  });

  return kept.length === 0 ? route.path : `${route.path}?${kept.join("&")}`;
}
