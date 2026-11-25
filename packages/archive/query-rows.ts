// The rows of the plain questions several stores of the archive ask their tables alike — a count, a
// list of rooms, of ids, of columns — and the one way a list kept in a column is read back.

export interface CountRow {
  count: number;
}

export interface ChatIdRow {
  chat_id: number;
}

export interface JobIdRow {
  id: number;
}

export interface RunIdRow {
  id: number;
}

// A list column as the row keeps it, read back tolerantly: a column that holds nothing readable is
// an empty list rather than a failed read of the whole library.
export function listColumn<T>(text: string, keep: (value: unknown) => value is T): T[] {
  try {
    const value: unknown = JSON.parse(text);

    return Array.isArray(value) ? value.filter(keep) : [];
  } catch {
    return [];
  }
}
