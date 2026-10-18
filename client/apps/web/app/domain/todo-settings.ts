import { getCurrentInstance, onMounted } from "vue";

import { clampPanelWidth, conditionsOf, DEFAULT_VIEW, isTodoFilter, PANEL_DEFAULT_WIDTH } from "./todos";

import type { TodoView } from "./todos";
import type { Ref } from "vue";

// The TODO panel's own choices, kept in the browser they were made in, so the owner finds the panel as he
// left it: how wide it was dragged, whether it was folded away, and which state it shows narrowed by what.

const PANEL_KEY = "bmo-web.todo-panel";
const TODO_VIEW_KEY = "bmo-web.todo-view";

export interface TodoPanelLayout {
  width: Ref<number>;
  collapsed: Ref<boolean>;
  setWidth: (next: number) => void;
  setCollapsed: (next: boolean) => void;
}

export interface TodoViewSettings {
  view: Ref<TodoView>;
  setView: (next: TodoView) => void;
}

let restoredPanel = false;
let restoredView = false;

// A choice kept in this browser is put back once the component asking for it has mounted. The server drew
// the default, and a value put back before hydration leaves the attributes the server drew — a label, a
// width — standing on the elements, since hydration mends text and not those.
function afterMount(restore: () => void): void {
  if (getCurrentInstance() === null) {
    restore();
    return;
  }

  onMounted(restore);
}

// The panel's width and whether it is folded away.
export function useTodoPanelLayout(): TodoPanelLayout {
  const width = useState<number>("bmo-todo-panel-width", () => PANEL_DEFAULT_WIDTH);
  const collapsed = useState<boolean>("bmo-todo-panel-collapsed", () => false);

  if (import.meta.client && !restoredPanel) {
    restoredPanel = true;
    afterMount(() => {
      const held = readPanel();

      width.value = held.width;
      collapsed.value = held.collapsed;
    });
  }

  function store(): void {
    if (!import.meta.client) {
      return;
    }

    try {
      window.localStorage.setItem(PANEL_KEY, JSON.stringify({ width: width.value, collapsed: collapsed.value }));
    } catch {
      // A browser that refuses storage still keeps the panel as it stands for this visit.
    }
  }

  function setWidth(next: number): void {
    width.value = clampPanelWidth(next);
    store();
  }

  function setCollapsed(next: boolean): void {
    collapsed.value = next;
    store();
  }

  return { width, collapsed, setWidth, setCollapsed };
}

// Which state the panel is showing and in what order.
export function useTodoView(): TodoViewSettings {
  const view = useState<TodoView>("bmo-todo-view", () => DEFAULT_VIEW);

  if (import.meta.client && !restoredView) {
    restoredView = true;
    afterMount(() => {
      view.value = readView();
    });
  }

  function setView(next: TodoView): void {
    view.value = next;

    if (import.meta.client) {
      try {
        window.localStorage.setItem(TODO_VIEW_KEY, JSON.stringify(next));
      } catch {
        // A browser that refuses storage still shows what was picked for this visit.
      }
    }
  }

  return { view, setView };
}

function readPanel(): { width: number; collapsed: boolean } {
  try {
    const raw = window.localStorage.getItem(PANEL_KEY);
    const parsed: unknown = raw === null ? null : JSON.parse(raw);

    if (parsed === null || typeof parsed !== "object") {
      return { width: PANEL_DEFAULT_WIDTH, collapsed: false };
    }

    const held = parsed as { width?: unknown; collapsed?: unknown };

    return {
      width: clampPanelWidth(typeof held.width === "number" ? held.width : PANEL_DEFAULT_WIDTH),
      collapsed: held.collapsed === true
    };
  } catch {
    return { width: PANEL_DEFAULT_WIDTH, collapsed: false };
  }
}

function readView(): TodoView {
  try {
    const raw = window.localStorage.getItem(TODO_VIEW_KEY);
    const parsed: unknown = raw === null ? null : JSON.parse(raw);
    const held = parsed === null || typeof parsed !== "object" ? {} : (parsed as Record<string, unknown>);

    return {
      filter: isTodoFilter(held.filter) ? held.filter : DEFAULT_VIEW.filter,
      conditions: conditionsOf(held.conditions)
    };
  } catch {
    return DEFAULT_VIEW;
  }
}
