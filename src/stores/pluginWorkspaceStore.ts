import { create } from 'zustand';
import type {
  PluginExecutionResult,
  PluginInputValues,
  PluginSessionType,
} from '../plugins/types';

export interface OpenPluginTabInput {
  pluginId: string;
  sessionId: string;
  sessionType: PluginSessionType;
  serverName: string;
}

/**
 * One plugin opened against one session. The same plugin can be open for
 * several sessions at the same time; the id dedupes per (session, plugin).
 */
export interface PluginWorkspaceTab extends OpenPluginTabInput {
  id: string;
}

/** Execution state shared by every surface that shows a plugin panel. */
export interface PluginPanelState {
  activeActionId: string | null;
  inputValues: PluginInputValues;
  result: PluginExecutionResult | null;
}

interface PluginWorkspaceState {
  tabs: PluginWorkspaceTab[];
  /** null → terminal view (same sentinel convention as the file workspace). */
  activeTabId: string | null;
  panels: Record<string, PluginPanelState>;
  openPluginTab: (input: OpenPluginTabInput) => PluginWorkspaceTab;
  activateTab: (tabId: string | null) => void;
  closeTab: (tabId: string) => void;
  moveTabBefore: (fromId: string, toId: string) => void;
  closeTabsForSession: (sessionId: string) => void;
  retainTabsForSessions: (sessionIds: readonly string[]) => void;
  setActiveAction: (tabId: string, actionId: string | null) => void;
  setInputValue: (tabId: string, inputId: string, value: string | number | boolean) => void;
  setResult: (tabId: string, result: PluginExecutionResult | null) => void;
}

export function pluginPanelKey(sessionId: string, pluginId: string): string {
  return `${sessionId}::${pluginId}`;
}

const emptyPanel: PluginPanelState = {
  activeActionId: null,
  inputValues: {},
  result: null,
};

export const usePluginWorkspaceStore = create<PluginWorkspaceState>((set) => ({
  tabs: [],
  activeTabId: null,
  panels: {},

  openPluginTab: (input) => {
    const id = pluginPanelKey(input.sessionId, input.pluginId);
    const tab: PluginWorkspaceTab = { ...input, id };
    set((state) => ({
      tabs: state.tabs.some((existing) => existing.id === id)
        ? state.tabs
        : [...state.tabs, tab],
      activeTabId: id,
    }));
    return tab;
  },

  activateTab: (tabId) => set({ activeTabId: tabId }),

  moveTabBefore: (fromId, toId) => set((state) => {
    if (fromId === toId) return state;
    const fromIndex = state.tabs.findIndex((tab) => tab.id === fromId);
    const toIndex = state.tabs.findIndex((tab) => tab.id === toId);
    if (fromIndex === -1 || toIndex === -1) return state;
    const tabs = [...state.tabs];
    const [moved] = tabs.splice(fromIndex, 1);
    tabs.splice(toIndex, 0, moved);
    return { tabs };
  }),

  closeTab: (tabId) => set((state) => {
    const closingIndex = state.tabs.findIndex((tab) => tab.id === tabId);
    if (closingIndex === -1) return state;

    const tabs = state.tabs.filter((tab) => tab.id !== tabId);
    const { [tabId]: _closedPanel, ...panels } = state.panels;
    if (state.activeTabId !== tabId) return { tabs, panels };

    const nextActive = tabs[Math.min(closingIndex, tabs.length - 1)]?.id ?? null;
    return { tabs, panels, activeTabId: nextActive };
  }),

  closeTabsForSession: (sessionId) => set((state) => {
    const firstClosingIndex = state.tabs.findIndex((tab) => tab.sessionId === sessionId);
    if (firstClosingIndex === -1) return state;

    const closingIds = new Set(
      state.tabs.filter((tab) => tab.sessionId === sessionId).map((tab) => tab.id)
    );
    const tabs = state.tabs.filter((tab) => tab.sessionId !== sessionId);
    const panels = Object.fromEntries(
      Object.entries(state.panels).filter(([key]) => !closingIds.has(key))
    );
    if (!closingIds.has(state.activeTabId ?? '')) return { tabs, panels };

    return {
      tabs,
      panels,
      activeTabId: tabs[Math.min(firstClosingIndex, tabs.length - 1)]?.id ?? null,
    };
  }),

  retainTabsForSessions: (sessionIds) => set((state) => {
    const retainedSessionIds = new Set(sessionIds);
    const firstClosingIndex = state.tabs.findIndex(
      (tab) => !retainedSessionIds.has(tab.sessionId)
    );
    if (firstClosingIndex === -1) return state;

    const closingIds = new Set(
      state.tabs.filter((tab) => !retainedSessionIds.has(tab.sessionId)).map((tab) => tab.id)
    );
    const tabs = state.tabs.filter((tab) => retainedSessionIds.has(tab.sessionId));
    const panels = Object.fromEntries(
      Object.entries(state.panels).filter(([key]) => !closingIds.has(key))
    );
    if (tabs.some((tab) => tab.id === state.activeTabId)) return { tabs, panels };

    return {
      tabs,
      panels,
      activeTabId: tabs[Math.min(firstClosingIndex, tabs.length - 1)]?.id ?? null,
    };
  }),

  setActiveAction: (tabId, actionId) => set((state) => ({
    panels: {
      ...state.panels,
      [tabId]: {
        activeActionId: actionId,
        inputValues: {},
        result: null,
      },
    },
  })),

  setInputValue: (tabId, inputId, value) => set((state) => {
    const panel = state.panels[tabId] ?? emptyPanel;
    return {
      panels: {
        ...state.panels,
        [tabId]: { ...panel, inputValues: { ...panel.inputValues, [inputId]: value } },
      },
    };
  }),

  setResult: (tabId, result) => set((state) => {
    const panel = state.panels[tabId] ?? emptyPanel;
    return {
      panels: {
        ...state.panels,
        [tabId]: { ...panel, result },
      },
    };
  }),
}));
