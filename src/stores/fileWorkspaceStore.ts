import { create } from 'zustand';
import { forgetTextBuffer } from '../lib/fileEditBuffer';
import { getFileViewerKind, type FileViewerKind } from '../lib/fileWorkspace';

export const LOCAL_FILE_ORIGIN = 'local-files';

export interface OpenFileInput {
  /** Local documents use an origin identity, never a fabricated shell session. */
  source?: 'local';
  sessionId: string;
  viewerKind?: FileViewerKind;
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
  moveTabBefore: (fromId: string, toId: string) => void;
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
          kind: file.viewerKind ?? getFileViewerKind(file.name),
          dirty: false,
        },
      ],
      activeTabId: id,
    };
  }),

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

    forgetTextBuffer(tabId);
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
      (tab) => tab.source !== 'local' && !retainedSessionIds.has(tab.sessionId)
    );
    if (firstClosingIndex === -1) return state;

    const tabs = state.tabs.filter((tab) => tab.source === 'local' || retainedSessionIds.has(tab.sessionId));
    if (tabs.some((tab) => tab.id === state.activeTabId)) return { tabs };

    return {
      tabs,
      activeTabId: tabs[Math.min(firstClosingIndex, tabs.length - 1)]?.id ?? null,
    };
  }),

  setDirty: (tabId, dirty) => set((state) => {
    if (!state.tabs.some((tab) => tab.id === tabId && tab.dirty !== dirty)) return state;
    return { tabs: state.tabs.map((tab) => tab.id === tabId ? { ...tab, dirty } : tab) };
  }),
}));
