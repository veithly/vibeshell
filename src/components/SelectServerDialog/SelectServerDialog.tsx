import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useGSAP } from '@gsap/react';
import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import {
  Grid2X2,
  List,
  Monitor,
  Pencil,
  Plus,
  Search,
  Server as ServerIcon,
  Terminal,
  Wifi,
  X,
} from 'lucide-react';
import { cn } from '../../lib/utils';
import { useServerStore, type Server, type Group } from '../../stores/serverStore';
import { useSessionStore } from '../../stores/sessionStore';
import { useAvailableShells, useLocalShellStore, type ShellInfo } from '../../stores/localShellStore';

gsap.registerPlugin(useGSAP, ScrollTrigger);

type ConnectionTab = 'local' | 'ssh';
type LauncherView = 'list' | 'icons';

interface SelectServerDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onSelectServer: (server: Server) => void;
  onSelectLocalShell?: (shell: ShellInfo) => void;
  onAddServer: () => void;
  onEditServer?: (server: Server) => void;
  onNewSession?: (server: Server) => void;
  connectedServerIds?: Set<string>;
}

function loadLauncherView(): LauncherView {
  try {
    return globalThis.localStorage?.getItem('vibeshell-connection-view') === 'icons' ? 'icons' : 'list';
  } catch {
    return 'list';
  }
}

function loadConnectionTab(): ConnectionTab {
  try {
    return globalThis.localStorage?.getItem('newConnectionTab') === 'ssh' ? 'ssh' : 'local';
  } catch {
    return 'local';
  }
}

function saveLauncherPreference(key: string, value: string) {
  try {
    globalThis.localStorage?.setItem(key, value);
  } catch {
    // The current launcher state remains usable without persistence.
  }
}

function shellMark(shell: ShellInfo) {
  if (shell.id === 'pwsh' || shell.id.includes('powershell')) return 'PS';
  if (shell.id === 'cmd') return 'CMD';
  return '$';
}

function ServerCard({
  server,
  group,
  connected,
  sessionCount,
  view,
  onConnect,
  onEdit,
  onNewSession,
}: {
  server: Server;
  group?: Group;
  connected: boolean;
  sessionCount: number;
  view: LauncherView;
  onConnect: () => void;
  onEdit?: () => void;
  onNewSession?: () => void;
}) {
  return (
    <div
      className={cn(
        'connection-card group relative flex min-w-0 overflow-hidden bg-tokyo-bg transition-colors duration-200 hover:bg-tokyo-bg-hl',
        view === 'icons' ? 'h-[132px] flex-col border border-tokyo-bg-hl rounded-lg' : 'min-h-16 border-b border-tokyo-bg-hl last:border-b-0'
      )}
    >
      <button
        className={cn(
          'flex min-w-0 flex-1 text-left focus:outline-none focus:ring-1 focus:ring-inset focus:ring-tokyo-blue',
          view === 'icons' ? 'flex-col justify-between p-3' : 'items-center gap-3 p-4'
        )}
        onClick={onConnect}
      >
        <span className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-md bg-tokyo-selection text-tokyo-blue transition-transform duration-700 ease-out group-hover:scale-105">
          {connected ? <Wifi className="h-4 w-4 text-tokyo-green" /> : <ServerIcon className="h-4 w-4" />}
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm font-semibold text-tokyo-fg">{server.name}</span>
          <span className="mt-1 block truncate font-mono text-[11px] text-tokyo-comment">
            {server.username}@{server.host}:{server.port}
          </span>
          {group && <span className="mt-1 block truncate text-[10px] text-tokyo-comment">{group.name}</span>}
        </span>
      </button>
      <div className={cn('flex items-center gap-0.5', view === 'icons' ? 'absolute right-2 top-2' : 'pr-2')}>
        {connected && onNewSession && (
          <button className="icon-button" onClick={onNewSession} aria-label={`New ${server.name} session`} title="New session">
            <Plus className="h-3.5 w-3.5" />
          </button>
        )}
        {onEdit && (
          <button className="icon-button" onClick={onEdit} aria-label={`Edit ${server.name}`} title="Edit server">
            <Pencil className="h-3.5 w-3.5" />
          </button>
        )}
      </div>
      {sessionCount > 0 && (
        <span className="absolute bottom-2 right-2 font-mono text-[10px] text-tokyo-green">{sessionCount}</span>
      )}
    </div>
  );
}

function ShellCard({ shell, view, onClick }: { shell: ShellInfo; view: LauncherView; onClick: () => void }) {
  return (
    <button
      className={cn(
        'connection-card group flex min-w-0 overflow-hidden bg-tokyo-bg text-left transition-colors duration-200 hover:bg-tokyo-bg-hl',
        'focus:outline-none focus:ring-1 focus:ring-inset focus:ring-tokyo-blue',
        view === 'icons'
          ? 'h-[132px] flex-col justify-between rounded-lg border border-tokyo-bg-hl p-3'
          : 'min-h-16 items-center gap-3 border-b border-tokyo-bg-hl p-4 last:border-b-0'
      )}
      onClick={onClick}
    >
      <span className="flex h-8 min-w-8 items-center justify-center rounded-md bg-tokyo-selection px-1 font-mono text-[10px] font-bold text-tokyo-blue transition-transform duration-700 ease-out group-hover:scale-105">
        {shellMark(shell)}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-semibold text-tokyo-fg">{shell.name}</span>
        <span className="mt-1 block truncate font-mono text-[11px] text-tokyo-comment">{shell.path}</span>
      </span>
      {shell.isDefault && <span className="text-[10px] text-tokyo-blue">Default</span>}
    </button>
  );
}

export function SelectServerDialog({
  isOpen,
  onClose,
  onSelectServer,
  onSelectLocalShell,
  onAddServer,
  onEditServer,
  onNewSession,
  connectedServerIds = new Set(),
}: SelectServerDialogProps) {
  const { t } = useTranslation();
  const rootRef = useRef<HTMLDivElement>(null);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const toolbarRef = useRef<HTMLDivElement>(null);
  const { servers, groups, fetchServers, fetchGroups } = useServerStore();
  const sessions = useSessionStore((state) => state.sessions);
  const createLocalShellSession = useSessionStore((state) => state.createLocalShellSession);
  const setActiveSession = useSessionStore((state) => state.setActiveSession);
  const fetchAvailableShells = useLocalShellStore((state) => state.fetchAvailableShells);
  const fetchDefaultShell = useLocalShellStore((state) => state.fetchDefaultShell);
  const setLastSelectedShell = useLocalShellStore((state) => state.setLastSelectedShell);
  const { shells } = useAvailableShells();
  const [activeTab, setActiveTab] = useState<ConnectionTab>(loadConnectionTab);
  const [view, setView] = useState<LauncherView>(loadLauncherView);
  const [query, setQuery] = useState('');

  useEffect(() => {
    if (!isOpen) return;
    void Promise.all([
      fetchAvailableShells(),
      fetchDefaultShell(),
      fetchServers(),
      fetchGroups(),
    ]);
    setQuery('');
  }, [isOpen, fetchAvailableShells, fetchDefaultShell, fetchServers, fetchGroups]);

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  useGSAP(() => {
    if (!isOpen) return;
    const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (!reduceMotion) {
      gsap.fromTo(
        '.connection-card',
        { autoAlpha: 0, y: 26, scale: 0.98 },
        { autoAlpha: 1, y: 0, scale: 1, duration: 0.48, stagger: 0.035, ease: 'power3.out' }
      );
    }

    const scroller = scrollerRef.current;
    const toolbar = toolbarRef.current;
    if (!scroller || !toolbar) return;

    const pin = ScrollTrigger.create({
      trigger: toolbar,
      scroller,
      pin: toolbar,
      pinSpacing: true,
      start: 'top top',
      end: () => `+=${Math.max(1, scroller.scrollHeight - scroller.clientHeight)}`,
    });

    return () => pin.kill();
  }, { scope: rootRef, dependencies: [isOpen, activeTab, view, query] });

  const changeTab = useCallback((tab: ConnectionTab) => {
    setActiveTab(tab);
    saveLauncherPreference('newConnectionTab', tab);
  }, []);

  const changeView = useCallback((nextView: LauncherView) => {
    setView(nextView);
    saveLauncherPreference('vibeshell-connection-view', nextView);
  }, []);

  const sessionCounts = useMemo(() => {
    const counts = new Map<string, number>();
    sessions.forEach((session) => {
      if (session.sessionType === 'ssh' && (session.state === 'connected' || session.state === 'connecting')) {
        counts.set(session.serverId, (counts.get(session.serverId) ?? 0) + 1);
      }
    });
    return counts;
  }, [sessions]);

  const groupById = useMemo(() => new Map(groups.map((group) => [group.id, group])), [groups]);
  const normalizedQuery = query.trim().toLowerCase();
  const filteredServers = useMemo(() => servers.filter((server) => {
    if (!normalizedQuery) return true;
    return [server.name, server.host, server.username, ...server.tags]
      .some((value) => value.toLowerCase().includes(normalizedQuery));
  }), [servers, normalizedQuery]);
  const filteredShells = useMemo(() => shells.filter((shell) => {
    if (!normalizedQuery) return true;
    return `${shell.name} ${shell.path}`.toLowerCase().includes(normalizedQuery);
  }), [shells, normalizedQuery]);

  const handleShellClick = useCallback(async (shell: ShellInfo) => {
    setLastSelectedShell(shell.id);
    if (onSelectLocalShell) {
      onSelectLocalShell(shell);
      onClose();
      return;
    }
    const session = await createLocalShellSession(shell.id, 80, 24);
    if (session) {
      setActiveSession(session.id);
      onClose();
    }
  }, [createLocalShellSession, onClose, onSelectLocalShell, setActiveSession, setLastSelectedShell]);

  const handleServerClick = useCallback((server: Server) => {
    onSelectServer(server);
    onClose();
  }, [onClose, onSelectServer]);

  const handleAddServer = useCallback(() => {
    onClose();
    onAddServer();
  }, [onAddServer, onClose]);

  const handleEditServer = useCallback((server: Server) => {
    onClose();
    onEditServer?.(server);
  }, [onClose, onEditServer]);

  const handleNewServerSession = useCallback((server: Server) => {
    onClose();
    onNewSession?.(server);
  }, [onClose, onNewSession]);

  if (!isOpen) return null;

  const items = activeTab === 'ssh' ? filteredServers : filteredShells;

  return (
    <div ref={rootRef} className="fixed inset-0 z-50 flex items-start justify-center px-3 pb-3 pt-14 sm:px-6 sm:pt-20">
      <button className="absolute inset-0 bg-black/55" onClick={onClose} aria-label={t('common.close')} />
      <section
        role="dialog"
        aria-modal="true"
        aria-labelledby="connection-launcher-title"
        className="relative flex h-[min(720px,calc(100vh-6rem))] w-full max-w-5xl flex-col overflow-hidden rounded-lg border border-tokyo-bg-hl bg-tokyo-bg shadow-2xl"
      >
        <header className="flex h-14 flex-shrink-0 items-center justify-between gap-4 border-b border-tokyo-bg-hl bg-tokyo-bg-dark px-4 sm:px-5">
          <div className="flex min-w-0 items-center gap-3">
            <span className="flex h-8 w-8 items-center justify-center rounded-md bg-tokyo-fg text-tokyo-bg">
              <Plus className="h-4 w-4" />
            </span>
            <h2 id="connection-launcher-title" className="truncate text-base font-semibold text-tokyo-fg">
              {t('connect.launcherTitle')}
            </h2>
          </div>
          <button className="icon-button" onClick={onClose} aria-label={t('common.close')} title={t('common.close')}>
            <X className="h-4 w-4" />
          </button>
        </header>

        {servers.length > 1 && (
          <div className="connection-marquee h-7 flex-shrink-0 overflow-hidden border-b border-tokyo-bg-hl bg-tokyo-bg-dark" aria-hidden="true">
            <div className="connection-marquee-track flex h-full w-max items-center gap-8 whitespace-nowrap px-4 font-mono text-[10px] text-tokyo-comment">
              {[...servers, ...servers].map((server, index) => (
                <span key={`${server.id}-${index}`}>{server.name} / {server.host}</span>
              ))}
            </div>
          </div>
        )}

        <div ref={scrollerRef} className="min-h-0 flex-1 overflow-y-auto">
          <div ref={toolbarRef} className="z-20 flex flex-wrap items-center gap-3 border-b border-tokyo-bg-hl bg-tokyo-bg px-4 py-3 sm:px-5">
            <div className="flex rounded-md bg-tokyo-bg-dark p-0.5">
              <button
                className={cn('workspace-action', activeTab === 'local' && 'is-active')}
                onClick={() => changeTab('local')}
              >
                <Monitor className="h-4 w-4" />
                {t('session.localShell')}
              </button>
              <button
                className={cn('workspace-action', activeTab === 'ssh' && 'is-active')}
                onClick={() => changeTab('ssh')}
              >
                <ServerIcon className="h-4 w-4" />
                SSH
              </button>
            </div>

            <label className="relative min-w-[180px] flex-1">
              <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-tokyo-comment" />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={t('connect.searchConnections')}
                autoFocus
                className="h-9 w-full rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark pl-9 pr-3 text-sm text-tokyo-fg placeholder:text-tokyo-comment focus:outline-none focus:ring-1 focus:ring-tokyo-blue"
              />
            </label>

            <div className="flex items-center rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark p-0.5">
              <button
                className={cn('icon-button h-7 w-7', view === 'list' && 'bg-tokyo-selection text-tokyo-fg')}
                onClick={() => changeView('list')}
                aria-label={t('connect.listView')}
                title={t('connect.listView')}
              >
                <List className="h-4 w-4" />
              </button>
              <button
                className={cn('icon-button h-7 w-7', view === 'icons' && 'bg-tokyo-selection text-tokyo-fg')}
                onClick={() => changeView('icons')}
                aria-label={t('connect.iconsView')}
                title={t('connect.iconsView')}
              >
                <Grid2X2 className="h-4 w-4" />
              </button>
            </div>

            {activeTab === 'ssh' && (
              <button className="workspace-action border-tokyo-bg-hl bg-tokyo-fg text-tokyo-bg hover:bg-tokyo-fg hover:text-tokyo-bg" onClick={handleAddServer}>
                <Plus className="h-4 w-4" />
                {t('sidebar.addServer')}
              </button>
            )}
          </div>

          <div className="p-4 sm:p-5">
            {items.length === 0 ? (
              <div className="flex min-h-56 flex-col items-center justify-center text-center">
                <Terminal className="mb-4 h-7 w-7 text-tokyo-comment" />
                <p className="text-sm font-medium text-tokyo-fg">
                  {activeTab === 'ssh' ? t('server.noServers') : t('connect.noShells')}
                </p>
              </div>
            ) : activeTab === 'ssh' ? (
              <div
                className={cn(
                  'grid grid-flow-dense',
                  view === 'list'
                    ? 'grid-cols-1 overflow-hidden rounded-lg border border-tokyo-bg-hl'
                    : 'grid-cols-[repeat(auto-fill,minmax(160px,1fr))] gap-3'
                )}
              >
                {filteredServers.map((server) => (
                  <ServerCard
                    key={server.id}
                    server={server}
                    group={server.group_id ? groupById.get(server.group_id) : undefined}
                    connected={connectedServerIds.has(server.id)}
                    sessionCount={sessionCounts.get(server.id) ?? 0}
                    view={view}
                    onConnect={() => handleServerClick(server)}
                    onEdit={onEditServer ? () => handleEditServer(server) : undefined}
                    onNewSession={onNewSession ? () => handleNewServerSession(server) : undefined}
                  />
                ))}
              </div>
            ) : (
              <div
                className={cn(
                  'grid grid-flow-dense',
                  view === 'list'
                    ? 'grid-cols-1 overflow-hidden rounded-lg border border-tokyo-bg-hl'
                    : 'grid-cols-[repeat(auto-fill,minmax(160px,1fr))] gap-3'
                )}
              >
                {filteredShells.map((shell) => (
                  <ShellCard key={shell.id} shell={shell} view={view} onClick={() => handleShellClick(shell)} />
                ))}
              </div>
            )}
          </div>
        </div>
      </section>
    </div>
  );
}

export type { SelectServerDialogProps };
