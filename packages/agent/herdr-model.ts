// The arrangement a herdr keeps — workspaces, their tabs, the panes in them and what was reported on
// each — for a test's stand-in to answer from.
//
// Only the structure is here, and only in the shape this machine's herdr answered it: a workspace is
// made with a tab and a shell pane in it; a layout named by its tab replaces that tab with a new one,
// and a layout named by its workspace adds a tab; a pane moved into another tab keeps its id inside
// its workspace and takes its emptied tab away; a tab with no pane left closes, and a workspace with
// no tab left is gone. What runs in a pane is not: a stand-in that needs a run to answer does that
// itself, beside this.

export interface ModelPane {
  pane_id: string;
  workspace_id: string;
  tab_id: string;
  label: string | null;
  command: string[] | null;
  tokens: Record<string, string> | null;
}

export interface HerdrModel {
  // Answers one call, or undefined for a method the arrangement has nothing to say about.
  answer(method: string, params: Record<string, unknown>): unknown;
  panes(): ModelPane[];
  workspaces(): { workspace_id: string; label: string; number: number; tokens: Record<string, string> | null }[];
  // A pane whose process ended by itself.
  exit(paneId: string): void;
  // herdr started again: every workspace and pane revived with its label and command, its tokens gone.
  restart(): void;
}

export function herdrModel(): HerdrModel {
  const workspaces = new Map<string, { workspace_id: string; label: string; number: number; tokens: Record<string, string> | null; tabs: number; panes: number }>();
  const tabs = new Map<string, { tab_id: string; workspace_id: string; label: string }>();
  const panes = new Map<string, ModelPane>();
  let made = 0;

  const newTab = (workspaceId: string, label: string | null): string => {
    const workspace = workspaces.get(workspaceId);

    if (workspace === undefined) {
      return "";
    }

    workspace.tabs += 1;

    const tabId = `${workspaceId}:t${workspace.tabs}`;

    tabs.set(tabId, { tab_id: tabId, workspace_id: workspaceId, label: label ?? String(workspace.tabs) });

    return tabId;
  };
  const newPane = (tabId: string, node: { label?: string | null; command?: string[] | null }): string => {
    const tab = tabs.get(tabId);
    const workspace = tab === undefined ? undefined : workspaces.get(tab.workspace_id);

    if (tab === undefined || workspace === undefined) {
      return "";
    }

    workspace.panes += 1;

    const paneId = `${tab.workspace_id}:p${workspace.panes}`;

    panes.set(paneId, { pane_id: paneId, workspace_id: tab.workspace_id, tab_id: tabId, label: node.label ?? null, command: node.command ?? null, tokens: null });

    return paneId;
  };
  const tidy = (): void => {
    for (const tabId of [...tabs.keys()]) {
      if (![...panes.values()].some((pane) => pane.tab_id === tabId)) {
        tabs.delete(tabId);
      }
    }

    for (const workspaceId of [...workspaces.keys()]) {
      if (![...tabs.values()].some((tab) => tab.workspace_id === workspaceId)) {
        workspaces.delete(workspaceId);
      }
    }
  };
  const view = () => ({
    workspaces: [...workspaces.values()].map(({ workspace_id, label, number, tokens }) => ({ workspace_id, label, number, tokens })),
    panes: [...panes.values()].map(({ command: _command, ...pane }) => pane),
  });

  return {
    answer(method, params) {
      switch (method) {
        case "workspace.create": {
          made += 1;

          const workspaceId = `w${made}`;

          workspaces.set(workspaceId, { workspace_id: workspaceId, label: String(params.label ?? ""), number: made, tokens: null, tabs: 0, panes: 0 });

          const tabId = newTab(workspaceId, null);
          const paneId = newPane(tabId, {});

          return { type: "workspace_created", workspace: { workspace_id: workspaceId }, tab: { tab_id: tabId }, root_pane: { pane_id: paneId } };
        }

        case "layout.apply": {
          const root = (params.root ?? {}) as { label?: string | null; command?: string[] };
          const replaced = typeof params.tab_id === "string" ? tabs.get(params.tab_id) : undefined;
          const workspaceId = replaced?.workspace_id ?? (typeof params.workspace_id === "string" ? params.workspace_id : "");

          if (!workspaces.has(workspaceId)) {
            return null;
          }

          if (replaced !== undefined) {
            for (const pane of [...panes.values()]) {
              if (pane.tab_id === replaced.tab_id) {
                panes.delete(pane.pane_id);
              }
            }

            tabs.delete(replaced.tab_id);
          }

          const tabId = newTab(workspaceId, typeof params.tab_label === "string" ? params.tab_label : null);
          const paneId = newPane(tabId, root);

          return { type: "layout_apply", layout: { workspace_id: workspaceId, tab_id: tabId, root: { type: "pane", pane_id: paneId, command: root.command } } };
        }

        case "layout.export": {
          // The whole tab the pane is in, as herdr answers it, and not the pane alone.
          const pane = panes.get(String(params.pane_id ?? ""));

          if (pane === undefined) {
            return null;
          }

          const node = (one: ModelPane): Record<string, unknown> => ({ type: "pane", pane_id: one.pane_id, ...(one.command === null ? {} : { command: one.command }) });
          const inTab = [...panes.values()].filter((one) => one.tab_id === pane.tab_id);
          const tree = (rest: ModelPane[]): Record<string, unknown> =>
            rest.length === 1 ? node(rest[0] as ModelPane) : { type: "split", direction: "down", ratio: 0.5, first: node(rest[0] as ModelPane), second: tree(rest.slice(1)) };

          return { layout: { tab_id: pane.tab_id, root: tree(inTab) } };
        }

        case "pane.report_metadata": {
          const pane = panes.get(String(params.pane_id ?? ""));

          if (pane === undefined) {
            return null;
          }

          pane.tokens = { ...(pane.tokens ?? {}), ...((params.tokens ?? {}) as Record<string, string>) };

          return { type: "ok" };
        }

        case "workspace.report_metadata": {
          const workspace = workspaces.get(String(params.workspace_id ?? ""));

          if (workspace === undefined) {
            return null;
          }

          workspace.tokens = { ...(workspace.tokens ?? {}), ...((params.tokens ?? {}) as Record<string, string>) };

          return { type: "ok" };
        }

        case "workspace.rename": {
          const workspace = workspaces.get(String(params.workspace_id ?? ""));

          if (workspace === undefined) {
            return null;
          }

          workspace.label = String(params.label ?? "");

          return { type: "workspace_info" };
        }

        case "pane.move": {
          const pane = panes.get(String(params.pane_id ?? ""));
          const destination = (params.destination ?? {}) as { tab_id?: string };
          const tab = tabs.get(destination.tab_id ?? "");

          if (pane === undefined || tab === undefined || tab.workspace_id !== pane.workspace_id) {
            return null;
          }

          pane.tab_id = tab.tab_id;
          tidy();

          return { type: "pane_moved", move_result: { pane: { ...pane }, previous_pane_id: pane.pane_id } };
        }

        case "pane.close": {
          if (!panes.delete(String(params.pane_id ?? ""))) {
            return null;
          }

          tidy();

          return { type: "ok" };
        }

        case "workspace.close": {
          const workspaceId = String(params.workspace_id ?? "");

          if (!workspaces.has(workspaceId)) {
            return null;
          }

          for (const pane of [...panes.values()]) {
            if (pane.workspace_id === workspaceId) {
              panes.delete(pane.pane_id);
            }
          }

          tidy();
          workspaces.delete(workspaceId);

          return { type: "ok" };
        }

        case "session.snapshot":
          return { type: "session_snapshot", snapshot: view() };

        default:
          return undefined;
      }
    },

    panes: () => [...panes.values()],
    workspaces: () => view().workspaces,

    exit(paneId) {
      panes.delete(paneId);
      tidy();
    },

    restart() {
      for (const workspace of workspaces.values()) workspace.tokens = null;
      for (const pane of panes.values()) pane.tokens = null;
    },
  };
}
