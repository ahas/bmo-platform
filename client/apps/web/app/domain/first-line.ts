// Where a conversation's new part begins, for a follower that reads it from its first line. Each row
// of the conversation carries `data-row`, and a row of the reader's own carries `data-mine` too.
//
// The new part begins at the first row after the reader's own last message — that message itself
// while nothing answers it — so a reply in several messages is read from its first; and at the first
// row not yet seen whole, where that one has been scrolled past or nothing new follows it. A row that
// stood when the reader last came to the end — opening the room, the way down, sending, their own
// scroll down — has been read, and so has a row standing before the reader's own last message: however
// either grows afterwards, a link card, a picture or a player sizing itself late, it never begins a new
// part. Nothing unseen is nothing new, and the follower keeps the end.
export interface FirstLineReader {
  readFrom: (seenBottom: number) => HTMLElement[];
  readThrough: () => void;
}

export function readsFromFirstLine(list: () => Element | null): FirstLineReader {
  // The row elements themselves: a row that goes and comes back, a running turn standing again for the
  // next turn, is a new element and new again.
  const read = new WeakSet<Element>();
  const rowsOf = (): HTMLElement[] => [...(list()?.querySelectorAll<HTMLElement>("[data-row]") ?? [])];

  return {
    readFrom(seenBottom) {
      const rows = rowsOf();
      const mine = rows.findLastIndex((row) => row.hasAttribute("data-mine"));
      const unseen = rows.findIndex((row, at) => at >= mine && !read.has(row) && row.getBoundingClientRect().bottom > seenBottom + 1);

      if (unseen === -1) {
        return [];
      }

      const reply = mine === -1 ? -1 : Math.min(mine + 1, rows.length - 1);

      return reply !== -1 && reply < unseen ? [rows[reply]!, rows[unseen]!] : [rows[unseen]!];
    },
    readThrough() {
      for (const row of rowsOf()) {
        read.add(row);
      }
    }
  };
}
