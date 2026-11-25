// How a message the owner sends rises out of the input bar into its place (docs/web.md, 「What
// arrives comes in at the end」): a bubble moving to its place, and for an answer settling a box changing
// size as it moves, so it takes the shared length both take and the easing an arrival takes. Both are
// the page's own motion tokens, read off the page as it runs rather than written here, so the flight
// moves with the rest of the console.

export interface FlightTiming {
  duration: number;
  ease: [number, number, number, number];
}

const LENGTH_TOKEN = "--bmo-motion-unfold";
const EASING_TOKEN = "--bmo-ease-enter";

// A length as the stylesheet writes it, in seconds, or null where it is not one.
export function seconds(value: string): number | null {
  const found = /^\s*([\d.]+)(ms|s)\s*$/.exec(value);

  if (found === null) {
    return null;
  }

  const amount = Number(found[1]);

  return found[2] === "ms" ? amount / 1000 : amount;
}

// An easing the stylesheet writes as a cubic Bézier, as its four numbers, or null where it is not one.
export function bezier(value: string): [number, number, number, number] | null {
  const found = /^\s*cubic-bezier\(([^)]*)\)\s*$/.exec(value);
  const points = found?.[1]?.split(",").map((point) => Number(point.trim()));

  return points?.length === 4 && points.every(Number.isFinite) ? (points as [number, number, number, number]) : null;
}

// The flight's timing off the page, or none where the page does not carry the tokens.
export function flightTiming(root: Element): FlightTiming | null {
  const style = getComputedStyle(root);
  const duration = seconds(style.getPropertyValue(LENGTH_TOKEN));
  const ease = bezier(style.getPropertyValue(EASING_TOKEN));

  return duration === null || ease === null ? null : { duration, ease };
}

// The easing a stylesheet's cubic Bézier draws, as a function of the movement's share of its length:
// the curve's x is solved for the share, and its y is how far the thing has come.
export function easing(points: readonly [number, number, number, number]): (share: number) => number {
  const [x1, y1, x2, y2] = points;
  const at = (a: number, b: number, t: number): number => 3 * a * t * (1 - t) ** 2 + 3 * b * t ** 2 * (1 - t) + t ** 3;
  const slope = (a: number, b: number, t: number): number => 3 * a * (1 - t) ** 2 + 6 * (b - a) * t * (1 - t) + 3 * (1 - b) * t ** 2;

  return (share) => {
    if (share <= 0 || share >= 1) {
      return Math.min(1, Math.max(0, share));
    }

    let t = share;

    for (let step = 0; step < 8; step += 1) {
      const off = at(x1, x2, t) - share;
      const steep = slope(x1, x2, t);

      if (Math.abs(off) < 1e-5 || steep === 0) {
        break;
      }

      t = Math.min(1, Math.max(0, t - off / steep));
    }

    return at(y1, y2, t);
  };
}

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface Point {
  x: number;
  y: number;
}

// Where a bubble's flight starts: the box its ground grows from, and the point its first part — a
// picture, or the first line of its words — stands at in it.
export interface RiseStart {
  box: Rect;
  origin: Point;
}

// One frame of a rise: the ground's box between the start box and the bubble, and where the
// bubble's contents stand, moved but never scaled.
export interface RiseFrame {
  ground: Rect;
  contents: Point;
}

// The frame a share of the way through stands at. The ground grows from the start box to the
// bubble; the contents, laid out at the bubble's own size from the start, move from where their first
// part stood to where it stands in the bubble. `inset` is that part's place inside the bubble.
export function riseFrame(start: RiseStart, bubble: Rect, inset: Point, come: number): RiseFrame {
  const lerp = (from: number, to: number): number => from + (to - from) * come;

  return {
    ground: {
      left: lerp(start.box.left, bubble.left),
      top: lerp(start.box.top, bubble.top),
      width: lerp(start.box.width, bubble.width),
      height: lerp(start.box.height, bubble.height),
    },
    contents: { x: lerp(start.origin.x - inset.x, bubble.left), y: lerp(start.origin.y - inset.y, bubble.top) },
  };
}

// The box a sent bubble rises from: its own, standing where its first part stood in the input box.
// The flight starts at the bubble's own size, so it only moves and its words keep the bubble's line
// breaks from the first frame to the last.
export function riseStart(origin: Point, now: Rect, inset: Point): RiseStart {
  return { box: { left: origin.x - inset.x, top: origin.y - inset.y, width: now.width, height: now.height }, origin };
}

// The owner's bubble rising out of the input bar into its place (docs/web.md, 「What arrives comes in
// at the end」): a layer of the bubble's own colour, corners and tail, and a copy of its words and
// pictures laid out at its final size, both standing from the first frame where its first part stood
// at `origin`, and only moving. Nothing changes size or is scaled, so no line breaks anew, no letter
// stretches and no corner rounds late. The bubble in the list stands hidden under the two until they
// land on it, and each frame reads where it stands now, so a list gliding under the rise is followed.
// Where the bubble leaves the page while it flies — the archived message taking the place of the one
// on its way — `follow` names the bubble standing in its place, and the rise goes on to that one.
export function riseOutOf(bubble: HTMLElement, origin: Point, timing: FlightTiming, follow: () => HTMLElement | null): Promise<void> {
  return fly(bubble, (now, inset) => riseStart(origin, now, inset), timing, follow);
}

// The box a bubble settles from, riding with the list: the row it takes the place of stood at `from`
// against the bubble standing at `first`, and stands the same way against wherever the bubble stands
// now. Its first part starts where it stands in the bubble, measured from that box's corner, so the
// words only move with the box and the ground grows round them.
export function settleStart(from: Rect, first: Rect, now: Rect, inset: Point): RiseStart {
  const box = { left: from.left + now.left - first.left, top: from.top + now.top - first.top, width: from.width, height: from.height };

  return { box, origin: { x: box.left + inset.x, y: box.top + inset.y } };
}

// An answer settling into the place of the turn that was running for it (docs/web.md, the room screen's section on what is
// being done now): the same flight as the owner's rise, but its ground growing from the box the turn's words stood
// in, on no ground of their own, to the answer's bubble. The list may glide while it settles, so the
// box it grows from rides along with the bubble.
export function settleInto(bubble: HTMLElement, from: Rect, timing: FlightTiming): Promise<void> {
  const first = bubble.getBoundingClientRect();

  return fly(bubble, (now, inset) => settleStart(from, first, now, inset), timing, () => null);
}

// The layer a flight is drawn in: the one the list the bubble stands in marks as its own, an isolated
// stacking context of the list's. The flight is appended last in it and carries no number, so it
// paints over the rows and under whatever the list itself lays over them, and the list's whole layer —
// the flight with it — stays under everything the page lays over the list: its bars, its floating
// buttons, and what the component library floats into the body. A bubble in no marked list flies in
// the body.
//
// Within the layer the flight goes into the place the layer keeps for it, its last child of no box
// of its own: a flight appended beside the rows would make the list stop being the layer's last and
// only child as it comes and again as it goes, and every rule reading those would have the whole list
// styled again at both moments.
export function flightLayer(bubble: HTMLElement): HTMLElement {
  const layer = bubble.closest<HTMLElement>("[data-flight-layer]");

  return layer === null ? bubble.ownerDocument.body : (layer.querySelector<HTMLElement>(":scope > [data-flight-host]") ?? layer);
}

// One flight: the ground grows and the contents move from where `startAt` says they began, against
// where the bubble stands this frame. Both stand at the layer's corner and are moved by `transform`
// alone, each on a layer of its own from the start, so a frame lays nothing out: only a ground that
// grows — an answer settling into a box of another size — is given its new size, and a rise's never
// changes. The clock starts at the first frame drawn rather than when the flight is laid down, so the
// work of the moment it begins — the sending, the list taking the new row — never eats its start.
// A bubble gone from the page hands the flight to the one `follow` names in its place, hidden as it
// was, and with none the flight ends at that frame.
function fly(bubble: HTMLElement, startAt: (now: Rect, inset: Point) => RiseStart, timing: FlightTiming, follow: () => HTMLElement | null): Promise<void> {
  const page = bubble.ownerDocument;
  const look = getComputedStyle(bubble);
  const first = bubble.querySelector("img, [data-words]") ?? bubble;
  const box = bubble.getBoundingClientRect();
  const part = first.getBoundingClientRect();
  const inset = { x: part.left - box.left, y: part.top - box.top };
  const ease = easing(timing.ease);

  const ground = page.createElement("div");
  ground.setAttribute("aria-hidden", "true");
  ground.dataset.riseGround = "";
  Object.assign(ground.style, {
    position: "fixed",
    left: "0",
    top: "0",
    pointerEvents: "none",
    willChange: "transform",
    backgroundColor: look.backgroundColor,
    borderRadius: [look.borderTopLeftRadius, look.borderTopRightRadius, look.borderBottomRightRadius, look.borderBottomLeftRadius].join(" "),
  });
  ground.style.setProperty("--bubble", look.backgroundColor);

  const tail = bubble.querySelector(".bubble-tail");

  if (tail !== null) {
    ground.append(tail.cloneNode(true));
  }

  const contents = bubble.cloneNode(true) as HTMLElement;
  contents.querySelector(".bubble-tail")?.remove();
  contents.removeAttribute("data-bubble");
  contents.setAttribute("aria-hidden", "true");
  contents.dataset.riseContents = "";
  Object.assign(contents.style, {
    position: "fixed",
    left: "0",
    top: "0",
    margin: "0",
    pointerEvents: "none",
    willChange: "transform",
    width: `${box.width}px`,
    height: `${box.height}px`,
    backgroundColor: "transparent",
  });

  flightLayer(bubble).append(ground, contents);
  bubble.style.visibility = "hidden";

  const size = { width: "", height: "" };
  let landing = bubble;

  function draw(share: number): void {
    const at = landing.getBoundingClientRect();
    const stands = { left: at.left, top: at.top, width: at.width, height: at.height };
    const drawn = riseFrame(startAt(stands, inset), stands, inset, ease(share));
    const width = `${drawn.ground.width}px`;
    const height = `${drawn.ground.height}px`;

    if (width !== size.width || height !== size.height) {
      Object.assign(ground.style, { width, height });
      Object.assign(size, { width, height });
    }

    ground.style.transform = `translate(${drawn.ground.left}px, ${drawn.ground.top}px)`;
    contents.style.transform = `translate(${drawn.contents.x}px, ${drawn.contents.y}px)`;
  }

  draw(0);

  return new Promise((done) => {
    let began: number | null = null;

    function frame(now: number): void {
      began ??= now;

      const share = Math.min(1, (now - began) / (timing.duration * 1000));
      const standing = landing.isConnected ? landing : follow();

      if (standing !== null && standing !== landing) {
        standing.style.visibility = "hidden";
        landing = standing;
      }

      if (standing !== null) {
        draw(share);
      }

      if (share < 1 && standing !== null) {
        requestAnimationFrame(frame);

        return;
      }

      ground.remove();
      contents.remove();
      landing.style.visibility = "";
      done();
    }

    requestAnimationFrame(frame);
  });
}
