// How the pictures of one send stand in their bubble (docs/web.md, the room screen's album section): a mosaic of rows that
// fill the bubble's width with no empty bars. Each row stands at the height that lets its pictures,
// side by side at their own shapes, fill the width exactly; each picture is cut to its cell. The rows
// are split by how many pictures there are, so no row is left half full.

export interface AlbumRow {
  // The pictures of the row, by their place in the send.
  pictures: number[];
  // The row's width over its height: the sum of its pictures' own, kept within what reads as a row.
  aspect: number;
}

// The widest and the tallest a row may stand, as its width over its height. A single tall picture
// would otherwise stand taller than the bubble is wide, and a long row of wide ones as a sliver.
const ROW_TALLEST = 1;
const ROW_WIDEST = 4;

// How many pictures each row holds: one row for two, one over the rest for three and four, and rows
// of three after that, with rows of two taking up what three would leave alone.
export function rowSizes(count: number): number[] {
  if (count <= 2) {
    return count === 0 ? [] : [count];
  }

  if (count === 3) {
    return [1, 2];
  }

  if (count === 4) {
    return [1, 3];
  }

  const threes = Math.floor(count / 3);
  const left = count % 3;

  if (left === 0) {
    return Array.from({ length: threes }, () => 3);
  }

  if (left === 2) {
    return [2, ...Array.from({ length: threes }, () => 3)];
  }

  // One left over: two rows of two take its place and the three before it.
  return [2, 2, ...Array.from({ length: threes - 1 }, () => 3)];
}

// The rows of an album, given each picture's width over its height; a picture whose shape is not
// known yet stands as a square.
export function albumRows(aspects: readonly (number | null)[]): AlbumRow[] {
  const rows: AlbumRow[] = [];
  let next = 0;

  for (const size of rowSizes(aspects.length)) {
    const pictures = Array.from({ length: size }, (_, at) => next + at);
    const sum = pictures.reduce((total, at) => total + (aspects[at] ?? 1), 0);

    rows.push({ pictures, aspect: Math.min(ROW_WIDEST, Math.max(ROW_TALLEST, sum)) });
    next += size;
  }

  return rows;
}

// Three or four pictures whose first is not wide stand as one large picture on the leading side and
// the rest stacked beside it, all at one height: the large one's width over that height is its own
// shape, the column's is what its pictures stacked at one width come to, and each in the column takes
// the height its shape asks of that width.
export interface AlbumSide {
  // The block's width over its height.
  aspect: number;
  // How much of the width the large picture and the column take, as shares of the two.
  main: number;
  column: number;
  // How much of the column's height each of the rest takes, by their place in the send.
  rest: { picture: number; share: number }[];
}

// The widest a first picture may be and still stand beside the rest rather than over them.
const SIDE_WIDEST_FIRST = 1.2;
const SIDE_TALLEST = 0.75;
const SIDE_WIDEST = 2;

export type AlbumLayout = { kind: "rows"; rows: AlbumRow[] } | ({ kind: "side" } & AlbumSide);

export function albumLayout(aspects: readonly (number | null)[]): AlbumLayout {
  const shapes = aspects.map((aspect) => aspect ?? 1);
  const first = shapes[0] ?? 1;

  if ((shapes.length !== 3 && shapes.length !== 4) || first > SIDE_WIDEST_FIRST) {
    return { kind: "rows", rows: albumRows(aspects) };
  }

  const rest = shapes.slice(1);
  const column = 1 / rest.reduce((total, aspect) => total + 1 / aspect, 0);

  return {
    kind: "side",
    aspect: Math.min(SIDE_WIDEST, Math.max(SIDE_TALLEST, first + column)),
    main: first,
    column,
    rest: rest.map((aspect, at) => ({ picture: at + 1, share: 1 / aspect })),
  };
}
