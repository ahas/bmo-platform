// The cells a settings table is built of (docs/web.md, 「Settings」): a cell that is pressed to change what it
// shows, and one that only shows it. Every screen whose settings stand as a table draws its cells with these.

// A cell that is pressed, as one box from edge to edge of its column, so the whole cell answers a hover and a
// press.
export const CELL_BUTTON =
  "relative flex min-h-12 w-full min-w-0 flex-col items-start justify-center px-2 py-1 text-start transition-colors hover:bg-elevated active:bg-accented focus-inset disabled:cursor-default disabled:hover:bg-transparent disabled:active:bg-transparent";

// A cell that only shows what a unit worked out.
export const CELL_TEXT = "flex min-h-12 min-w-0 flex-col justify-center px-2 py-1";

// One choice a choice cell offers.
export interface CellChoice {
  value: string | number;
  label: string;
  description?: string;
  disabled?: boolean;
}
