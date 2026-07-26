import { create } from 'zustand';
import { getFileViewerKind, type FileViewerKind } from '../lib/fileWorkspace';

export interface OpenFileInput {
  sessionId: string;
  path: string;
  name: string;
  size: number;
}

export interface FileWorkspaceTab extends OpenFileInput {
  id: string;
  kind: FileViewerKind;
  dirty: boolean;
}

interface FileWorkspaceState {
  tabs: FileWorkspaceTab[];
  activeTabId: string | null;
  openFile: (file: OpenFileInput) => void;
  activateTab: (tabId: string | null) => void;
  closeTab: (tabId: string) => void;
  closeTabsForSession: (sessionId: string) => void;
  retainTabsForSessions: (sessionIds: readonly string[]) => void;
  setDirty: (tabId: string, dirty: boolean) => void;
}

function tabIdFor(file: Pick<OpenFileInput, 'sessionId' | 'path'>): string {
  return `${file.sessionId}\u0000${file.path}`;
}

export const useFileWorkspaceStore = create<FileWorkspaceState>((set) => ({
  tabs: [],
  activeTabId: null,

  openFile: (file) => set((state) => {
    const id = tabIdFor(file);
    const existing = state.tabs.find((tab) => tab.id === id);
    if (existing) {
      return { activeTabId: id };
    }

    return {
      tabs: [
        ...state.tabs,
        {
          ...file,
          id,
          kind: getFileViewerKind(file.name),
          dirty: false,
        },
      ],
      activeTabId: id,
    };
  }),

  activateTab: (tabId) => set({ activeTabId: tabId }),

  closeTab: (tabId) => set((state) => {
    const closingIndex = state.tabs.findIndex((tab) => tab.id === tabId);
    if (closingIndex === -1) return state;

    const tabs = state.tabs.filter((tab) => tab.id !== tabId);
    if (state.activeTabId !== tabId) return { tabs };

    const nextActive = tabs[Math.min(closingIndex, tabs.length - 1)]?.id ?? null;
    return { tabs, activeTabId: nextActive };
  }),

  closeTabsForSession: (sessionId) => set((state) => {
    const firstClosingIndex = state.tabs.findIndex((tab) => tab.sessionId === sessionId);
    if (firstClosingIndex === -1) return state;

    const closingActiveTab = state.tabs.some(
      (tab) => tab.id === state.activeTabId && tab.sessionId === sessionId
    );
    const tabs = state.tabs.filter((tab) => tab.sessionId !== sessionId);
    if (!closingActiveTab) return { tabs };

    return {
      tabs,
      activeTabId: tabs[Math.min(firstClosingIndex, tabs.length - 1)]?.id ?? null,
    };
  }),

  retainTabsForSessions: (sessionIds) => set((state) => {
    const retainedSessionIds = new Set(sessionIds);
    const firstClosingIndex = state.tabs.findIndex(
      (tab) => !retainedSessionIds.has(tab.sessionId)
    );
    if (firstClosingIndex === -1) return state;

    const tabs = state.tabs.filter((tab) => retainedSessionIds.has(tab.sessionId));
    if (tabs.some((tab) => tab.id === state.activeTabId)) return { tabs };

    return {
      tabs,
      activeTabId: tabs[Math.min(firstClosingIndex, tabs.length - 1)]?.id ?? null,
    };
  }),

  setDirty: (tabId, dirty) => set((state) => ({
    tabs: state.tabs.map((tab) => tab.id === tabId ? { ...tab, dirty } : tab),
  })),
}));
