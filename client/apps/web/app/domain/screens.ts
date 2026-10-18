import type { Translate } from "#shared/wording";
import type { NavigationMenuItem } from "@nuxt/ui";

// The console's own screens: the flat group at the top of the sidebar, in the order they stand.
// The sidebar and the console search both read this list, so a screen added here is in both.
export interface Screen {
  label: string;
  to: string;
  icon: string;
  // Whether the screen's own pages under its address light it as well.
  nested: boolean;
  // The tabs a screen keeps in its address as `?tab=`. The first is the screen's own address.
  tabs?: readonly ScreenTab[];
  // The mark the screen's entry carries while something on it wants looking at, read from what the
  // console knows live. An entry carries a mark and never a count, so nothing grows beside its name.
  status?: (live: ScreenLive) => ScreenStatus | null;
}

// What the console knows live that a screen's entry may mark, each fact by the name the screen that marks
// it reads it under.
export type ScreenLive = Readonly<Record<string, boolean>>;

export interface ScreenStatus {
  icon: string;
  // What the mark says, for the entry's tooltip and for a reader that cannot see it.
  label: string;
  // The mark's colour classes where it stands as a cell of its own at the end of the row.
  tone: string;
  // The same mark where the sidebar is folded and the row is its icon alone: drawn over the icon's
  // own corner rather than given a cell, which would push that row's icon off the centre the rows
  // without a mark stand on. Written out class and all, since the stylesheet is built from the
  // words in this file.
  folded: string;
}

export const QUIET: ScreenLive = {};

export interface ScreenTab {
  value: string;
  label: string;
  icon: string;
}

// The screens that stand in this console now, in the order the sidebar shows them. A screen's group
// puts its entry here as it brings the screen; a unit not yet split stands as its placeholder's entry.
export const SCREENS: readonly Screen[] = [
  // The Archive: the documents kept for reading, each with its attachments. Its pages are the
  // bmo-artifact bundle's, and its pages below the address light it.
  { label: "screens.label.archive", to: "/archive", icon: "i-material-symbols-archive-outline", nested: true }
];

export function isScreenActive(screen: Screen, path: string): boolean {
  return path === screen.to || (screen.nested && path.startsWith(`${screen.to}/`));
}

export function tabAddress(screen: Screen, tab: ScreenTab): string {
  return tab.value === screen.tabs?.[0]?.value ? screen.to : `${screen.to}?tab=${encodeURIComponent(tab.value)}`;
}

// Where the mark stands on a folded row: over the icon's own top corner, on whole pixels, in the
// colour the status names.
export const FOLDED_MARK = "relative after:absolute after:top-1 after:end-1 after:size-2 after:rounded-full after:content-['']";

export function screenItems(t: Translate, path: string, live: ScreenLive = QUIET, folded = false): NavigationMenuItem[] {
  return SCREENS.map((screen) => {
    const status = screen.status?.(live) ?? null;
    const entry = {
      label: t(screen.label),
      to: screen.to,
      icon: screen.icon,
      active: isScreenActive(screen, path)
    };

    if (status === null) {
      return entry;
    }

    // Folded, the name is gone and the row is its icon alone. A mark given a cell at the row's end
    // is dropped there by the menu, so the row says nothing at all; drawn over the icon's corner it
    // says the same thing and moves no icon.
    return folded
      ? { ...entry, ui: { link: `${FOLDED_MARK} ${status.folded}` }, "aria-description": t(status.label) }
      : { ...entry, trailingIcon: status.icon, ui: { linkTrailingIcon: status.tone }, "aria-description": t(status.label) };
  });
}

// Folded, a row's words are gone and it is its icon alone, so its name is handed to it as its
// accessible name as well as its tooltip.
export function namedWhenFolded(items: NavigationMenuItem[], folded: boolean): NavigationMenuItem[] {
  return folded ? items.map((item) => (item.type === "label" ? item : { ...item, "aria-label": item.label })) : items;
}
