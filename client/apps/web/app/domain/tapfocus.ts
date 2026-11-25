// A message row turns a finger's press away from the browser, so a held finger chooses messages
// rather than letters, and the focus the press would have moved goes with it. A tap on a control in
// the row — a chip, a card, a download — then leaves the focus where it stood, and where that is the
// input box a phone raises its keyboard again on every tap. So the tap lets go of a focus held
// outside the row, the way the browser's own press would have; blurring rather than focusing the
// control keeps the focus ring and a tooltip's keyboard opening away, since a focus moved by a
// script out of a text box counts as the keyboard's (docs/web.md, the room screen's chip line section).
export function releaseFocusOutside(row: Element): void {
  const focused = row.ownerDocument.activeElement;

  if (focused instanceof HTMLElement && focused !== row.ownerDocument.body && !row.contains(focused)) {
    focused.blur();
  }
}
