import type { MosaicNode } from 'react-mosaic-component2';
import { useSessionStore, type Session } from '../stores/sessionStore';
import { useFileWorkspaceStore, type FileWorkspaceTab } from '../stores/fileWorkspaceStore';
import { usePluginWorkspaceStore, type PluginWorkspaceTab } from '../stores/pluginWorkspaceStore';
import { parsePaneId, sessionPaneId, pluginPaneId, filePaneId } from './paneIds';
import { sanitizeTree } from './docking';
import { readDetachedLayout, saveDetachedLayout, restoreDetachedWindows, type DetachedLayoutEntry } from './detach';
import { readTextBuffer, writeTextBuffer } from './fileEditBuffer';

const KEY = 'vibeshell.workspace-layout.v2';
export type SavedSession = Pick<Session, 'id' | 'serverId' | 'serverName' | 'sessionType' | 'purpose' | 'cwd'>;
export interface WorkspaceLayout {
  version: 2;
  tree: MosaicNode<string> | null;
  focusedPane: string | null;
  sessions: SavedSession[];
  files: FileWorkspaceTab[];
  plugins: PluginWorkspaceTab[];
  activeSessionId: string | null;
  activeFileId: string | null;
  activePluginId: string | null;
  detached: DetachedLayoutEntry[];
}
const string = (value: unknown): value is string => typeof value === 'string' && value.length > 0 && value.length < 8192;

export function readWorkspaceLayout(): WorkspaceLayout | null {
  try {
    const value = JSON.parse(localStorage.getItem(KEY) ?? 'null') as WorkspaceLayout | null;
    if (!value || value.version !== 2 || !Array.isArray(value.sessions)
      || !Array.isArray(value.files) || !Array.isArray(value.plugins) || !Array.isArray(value.detached)) return null;
    return {
      ...value,
      tree: sanitizeTree(value.tree),
      focusedPane: string(value.focusedPane) ? value.focusedPane : null,
      sessions: value.sessions.slice(0, 64).filter((s) => s && string(s.id) && string(s.serverName)
        && string(s.serverId) && (s.sessionType === 'ssh' || s.sessionType === 'local')),
      files: value.files.slice(0, 128).filter((f) => f && string(f.id) && string(f.sessionId) && string(f.path) && string(f.name) && Number.isFinite(f.size)),
      plugins: value.plugins.slice(0, 128).filter((p) => p && string(p.id) && string(p.pluginId) && string(p.sessionId)),
      detached: value.detached.slice(0, 32).filter((d) => d?.target && string(d.target.sessionId)
        && ['terminal', 'file', 'plugin'].includes(d.target.kind) && string(d.geometryKey)),
    };
  } catch { return null; }
}

export function saveWorkspaceLayout(tree: MosaicNode<string> | null, focusedPane: string | null): void {
  const sessions = useSessionStore.getState();
  const files = useFileWorkspaceStore.getState();
  const plugins = usePluginWorkspaceStore.getState();
  const layout: WorkspaceLayout = {
    version: 2, tree: sanitizeTree(tree), focusedPane,
    // Explicit allowlist: no authentication fields, commands, plugin inputs or output.
    sessions: sessions.sessions.map(({ id, serverId, serverName, sessionType, purpose, cwd }) => ({ id, serverId, serverName, sessionType, purpose, cwd })),
    files: files.tabs, plugins: plugins.tabs,
    activeSessionId: sessions.activeSessionId, activeFileId: files.activeTabId, activePluginId: plugins.activeTabId,
    detached: readDetachedLayout(),
  };
  localStorage.setItem(KEY, JSON.stringify(layout));
}

/** A runtime session ID is not a persistent identity. Re-map every reference together. */
export function remapWorkspace(layout: WorkspaceLayout, sessions: Map<string, Session>): WorkspaceLayout {
  const files = layout.files.flatMap((tab) => {
    if (tab.source === 'local') return [tab];
    const session = sessions.get(tab.sessionId);
    return session ? [{ ...tab, sessionId: session.id, id: `${session.id}\u0000${tab.path}` }] : [];
  });
  const plugins = layout.plugins.flatMap((tab) => {
    const session = sessions.get(tab.sessionId);
    return session ? [{ ...tab, sessionId: session.id, sessionType: session.sessionType, serverName: session.serverName, id: `${session.id}::${tab.pluginId}` }] : [];
  });
  const ids = new Map<string, string>();
  for (const [previous, session] of sessions) ids.set(sessionPaneId(previous), sessionPaneId(session.id));
  for (const tab of layout.files) {
    if (tab.source === 'local') { ids.set(filePaneId(tab.id), filePaneId(tab.id)); continue; }
    const session = sessions.get(tab.sessionId);
    if (session) ids.set(filePaneId(tab.id), filePaneId(`${session.id}\u0000${tab.path}`));
  }
  for (const tab of layout.plugins) {
    const session = sessions.get(tab.sessionId);
    if (session) ids.set(pluginPaneId(tab.id), pluginPaneId(`${session.id}::${tab.pluginId}`));
  }
  const visit = (node: MosaicNode<string> | null): MosaicNode<string> | null => {
    if (node === null) return null;
    if (typeof node === 'string') return ids.get(node) ?? null;
    const first = visit(node.first); const second = visit(node.second);
    return first && second ? { ...node, first, second } : first ?? second;
  };
  const detached = layout.detached.flatMap((entry): DetachedLayoutEntry[] => {
    if (entry.target.kind === 'file' && entry.target.source === 'local') return [entry];
    const session = sessions.get(entry.target.sessionId);
    if (!session) return [];
    const target = { ...entry.target, sessionId: session.id };
    if (target.kind === 'plugin') { target.sessionType = session.sessionType; target.serverName = session.serverName; }
    return [{ target, geometryKey: entry.geometryKey }];
  });
  const activeFilePane = layout.activeFileId ? ids.get(filePaneId(layout.activeFileId)) : null;
  const activePluginPane = layout.activePluginId ? ids.get(pluginPaneId(layout.activePluginId)) : null;
  return {
    ...layout, tree: sanitizeTree(visit(layout.tree)), files, plugins, detached,
    sessions: [...sessions.values()],
    focusedPane: layout.focusedPane ? ids.get(layout.focusedPane) ?? null : null,
    activeSessionId: layout.activeSessionId ? sessions.get(layout.activeSessionId)?.id ?? null : null,
    activeFileId: activeFilePane ? parsePaneId(activeFilePane).id : null,
    activePluginId: activePluginPane ? parsePaneId(activePluginPane).id : null,
  };
}

export async function restoreWorkspaceLayout(localShell: boolean): Promise<{ layout: WorkspaceLayout | null; warnings: string[] }> {
  const saved = readWorkspaceLayout();
  if (!saved) return { layout: null, warnings: [] };
  const mapping = new Map<string, Session>();
  const used = new Set<string>();
  const warnings: string[] = [];
  for (const descriptor of saved.sessions) {
    const state = useSessionStore.getState();
    let session: Session | null | undefined = state.sessions.find((s) => s.id === descriptor.id && !used.has(s.id));
    // Daemon-backed sessions can survive a GUI restart with new client bookkeeping.
    if (!session && descriptor.sessionType === 'ssh') {
      session = state.sessions.find((s) => !used.has(s.id) && s.sessionType === 'ssh' && s.serverId === descriptor.serverId);
    }
    if (!session) {
      if (descriptor.sessionType === 'ssh') session = await state.connectSession(descriptor.serverName);
      else if (localShell) {
        session = await state.createLocalShellSession(descriptor.purpose === 'coding_agent' ? undefined : descriptor.serverId, 80, 24);
        if (descriptor.purpose === 'coding_agent') warnings.push(descriptor.serverName);
      }
    }
    if (session) { mapping.set(descriptor.id, session); used.add(session.id); }
    else warnings.push(descriptor.serverName);
  }
  const layout = remapWorkspace(saved, mapping);
  for (const previous of saved.files) {
    const session = mapping.get(previous.sessionId);
    if (!session) continue;
    const buffer = readTextBuffer(previous.id);
    if (buffer) writeTextBuffer(`${session.id}\u0000${previous.path}`, buffer);
  }
  // Keep live sessions not referenced by the saved workspace at the end of the rail.
  useSessionStore.setState((state) => ({ sessions: [...mapping.values(), ...state.sessions.filter((s) => !used.has(s.id))], activeSessionId: layout.activeSessionId ?? state.activeSessionId }));
  useFileWorkspaceStore.setState({ tabs: layout.files, activeTabId: layout.activeFileId });
  usePluginWorkspaceStore.setState({ tabs: layout.plugins, activeTabId: layout.activePluginId });
  saveDetachedLayout(layout.detached);
  return { layout, warnings };
}

export async function restoreWorkspaceWindows(layout: WorkspaceLayout | null): Promise<void> {
  if (layout) await restoreDetachedWindows(layout.detached);
}
