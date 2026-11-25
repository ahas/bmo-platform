// The array walks whose step answers a promise, taken one item at a time in the array's order. A
// step that reads or writes the archive waits for the one before it, as it did when the archive
// answered on the spot, so writes land in the order they were asked for and a filter or a search
// stops reading where its answer is found.

type Step<T, R> = (item: T, index: number, items: readonly T[]) => R | Promise<R>;

export async function mapInOrder<T, R>(items: readonly T[], step: Step<T, R>): Promise<R[]> {
  const answers: R[] = [];

  for (const [index, item] of items.entries()) answers.push(await step(item, index, items));

  return answers;
}

export async function flatMapInOrder<T, R>(items: readonly T[], step: Step<T, R | readonly R[]>): Promise<R[]> {
  const answers: R[] = [];

  for (const [index, item] of items.entries()) {
    const answer = await step(item, index, items);

    if (Array.isArray(answer)) answers.push(...(answer as readonly R[]));
    else answers.push(answer as R);
  }

  return answers;
}

export async function eachInOrder<T>(items: readonly T[], step: Step<T, unknown>): Promise<void> {
  for (const [index, item] of items.entries()) await step(item, index, items);
}

export async function filterInOrder<T>(items: readonly T[], keep: Step<T, unknown>): Promise<T[]> {
  const kept: T[] = [];

  for (const [index, item] of items.entries()) if (await keep(item, index, items)) kept.push(item);

  return kept;
}

export async function findInOrder<T>(items: readonly T[], match: Step<T, unknown>): Promise<T | undefined> {
  for (const [index, item] of items.entries()) if (await match(item, index, items)) return item;

  return undefined;
}

export async function findIndexInOrder<T>(items: readonly T[], match: Step<T, unknown>): Promise<number> {
  for (const [index, item] of items.entries()) if (await match(item, index, items)) return index;

  return -1;
}

export async function someInOrder<T>(items: readonly T[], match: Step<T, unknown>): Promise<boolean> {
  return (await findIndexInOrder(items, match)) >= 0;
}

export async function everyInOrder<T>(items: readonly T[], match: Step<T, unknown>): Promise<boolean> {
  for (const [index, item] of items.entries()) if (!(await match(item, index, items))) return false;

  return true;
}
