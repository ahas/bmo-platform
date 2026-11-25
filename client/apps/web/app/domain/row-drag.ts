import { onBeforeUnmount, ref, type Ref } from "vue";

// Carrying a row of an ordered list by its handle, by the mouse, the pen and the finger alike —
// the TODO panel's own pointer rules, in the one place every list that reorders itself reads them
// from. A press is held until it has travelled far enough to be a drag, so a press that lets go
// where it went down leaves the list as it was, and the list is saved whole on the drop.
//
// A row is found by its `data-order-index`, which the list puts on each row, because a captured
// pointer reports the handle it was caught on whatever it is over: which row it is level with is
// worked out from where the rows stand rather than from what the event says.

// How far a press has to travel before it is carrying anything. A tap on a handle that never moves is a tap —
// it saves nothing — and a finger resting on one wanders a pixel or two by itself, so the same slack serves the
// mouse, the pen and the finger.
export const DRAG_START_PX = 6;

// Where a pointer is, in the coordinates a row's own box is measured in.
export interface DragPoint {
  x: number;
  y: number;
}

// Where one row of the list stands on the screen at this moment.
export interface RowBox {
  id: number;
  top: number;
  bottom: number;
}

// Whether a press on a handle is one that could carry a row: the main button, and the pointer that is leading.
// A right or middle button, and a second finger put down while one is already dragging, start nothing.
export function startsRowDrag(press: { button: number; isPrimary: boolean }): boolean {
  return press.button === 0 && press.isPrimary;
}

// Whether a press that is being held has become a drag. Until it has, no row is carried and nothing is saved,
// so a press that lets go where it went down leaves the list as it was.
export function movedEnoughToDrag(from: DragPoint, to: DragPoint): boolean {
  return Math.hypot(to.x - from.x, to.y - from.y) >= DRAG_START_PX;
}

// Which row a point falls on, or nothing where it falls past the ends of the list or in the gap between two
// rows. Only the height decides: the list runs down the panel, so a pointer that has wandered off to the side is
// still over the row it is level with.
export function rowAtPoint(boxes: RowBox[], y: number): number | null {
  return boxes.find((box) => y >= box.top && y < box.bottom)?.id ?? null;
}

export interface RowDragOptions {
  // The element the rows stand in.
  list: Ref<HTMLElement | null>;
  // Whether a press may begin a drag at all — nothing is carried while a save is in flight.
  ready: () => boolean;
  // Where the row was dropped, once it has been.
  drop: (from: number, to: number) => void;
}

export interface RowDrag {
  held: Ref<number | null>;
  over: Ref<number | null>;
  onHandleDown: (index: number, event: PointerEvent) => void;
  // What a row looks like while one is being carried: the one held, and the one it is over.
  mark: (index: number) => string;
  end: () => void;
}

export function useRowDrag(options: RowDragOptions): RowDrag {
  const held = ref<number | null>(null);
  const over = ref<number | null>(null);
  let press: { index: number; pointer: number; from: DragPoint; at: DragPoint; handle: HTMLElement } | null = null;

  function boxes(): RowBox[] {
    const root = options.list.value;

    if (root === null) {
      return [];
    }

    return [...root.querySelectorAll<HTMLElement>("[data-order-index]")].map((row) => {
      const box = row.getBoundingClientRect();

      return { id: Number(row.dataset.orderIndex), top: box.top, bottom: box.bottom };
    });
  }

  function onHandleDown(index: number, event: PointerEvent): void {
    if (!startsRowDrag(event) || !options.ready()) {
      return;
    }

    event.preventDefault();

    const handle = event.currentTarget as HTMLElement;
    const from = { x: event.clientX, y: event.clientY };

    press = { index, pointer: event.pointerId, from, at: from, handle };

    try {
      handle.setPointerCapture(event.pointerId);
    } catch {
      // A pointer the browser will not let us hold is still one we can follow while it is down.
    }

    handle.addEventListener("pointermove", onHandleMove);
    handle.addEventListener("pointerup", onHandleUp);
    handle.addEventListener("pointercancel", end);
  }

  function onHandleMove(event: PointerEvent): void {
    if (press === null || event.pointerId !== press.pointer) {
      return;
    }

    press.at = { x: event.clientX, y: event.clientY };

    if (held.value === null && !movedEnoughToDrag(press.from, press.at)) {
      return;
    }

    held.value = press.index;
    over.value = rowAtPoint(boxes(), press.at.y);
  }

  function onHandleUp(event: PointerEvent): void {
    const dragged = held.value;
    const onto = dragged === null ? null : rowAtPoint(boxes(), event.clientY);

    end();

    if (dragged !== null && onto !== null && onto !== dragged) {
      options.drop(dragged, onto);
    }
  }

  function end(): void {
    const current = press;

    press = null;
    held.value = null;
    over.value = null;

    if (current === null) {
      return;
    }

    current.handle.removeEventListener("pointermove", onHandleMove);
    current.handle.removeEventListener("pointerup", onHandleUp);
    current.handle.removeEventListener("pointercancel", end);

    if (current.handle.hasPointerCapture(current.pointer)) {
      current.handle.releasePointerCapture(current.pointer);
    }
  }

  function mark(index: number): string {
    if (held.value === index) {
      return "opacity-50 outline-1 outline-dashed outline-primary";
    }

    return over.value === index && held.value !== null ? "bg-primary/10" : "";
  }

  onBeforeUnmount(end);

  return { held, over, onHandleDown, mark, end };
}
