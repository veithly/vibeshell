import { type FormEvent, useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Bot, Check, FolderOpen, Loader2, Play, RefreshCw } from 'lucide-react';
import { cn } from '../../lib/utils';
import { safeInvoke } from '../../lib/tauri';
import { useSessionStore } from '../../stores/sessionStore';
import type {
  CodingAgentAccessMode,
  CodingAgentId,
  CodingAgentStartMode,
  CodingAgentTool,
} from '../../types/codingAgent';

interface CodingAgentLauncherProps {
  initialWorkspace?: string;
  onLaunched: (sessionId: string) => void;
}

const START_MODES: CodingAgentStartMode[] = ['new', 'continue_last', 'resume_picker'];
const ACCESS_MODES: CodingAgentAccessMode[] = ['default', 'read_only', 'auto_edit'];

function readPreference(key: string): string {
  try {
    return globalThis.localStorage?.getItem(key) ?? '';
  } catch {
    return '';
  }
}

function writePreference(key: string, value: string) {
  try {
    globalThis.localStorage?.setItem(key, value);
  } catch {
    // Preferences are optional; the active launch form remains usable.
  }
}

export function CodingAgentLauncher({ initialWorkspace, onLaunched }: CodingAgentLauncherProps) {
  const { t } = useTranslation();
  const launchCodingAgentSession = useSessionStore((state) => state.launchCodingAgentSession);
  const [tools, setTools] = useState<CodingAgentTool[]>([]);
  const [selectedId, setSelectedId] = useState<CodingAgentId | null>(null);
  const [workspace, setWorkspace] = useState(
    () => initialWorkspace || readPreference('vibeshell-coding-workspace')
  );
  const [startMode, setStartMode] = useState<CodingAgentStartMode>('new');
  const [accessMode, setAccessMode] = useState<CodingAgentAccessMode>('default');
  const [prompt, setPrompt] = useState('');
  const [loadingTools, setLoadingTools] = useState(true);
  const [launching, setLaunching] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refreshTools = useCallback(async () => {
    setLoadingTools(true);
    setError(null);
    const result = await safeInvoke<CodingAgentTool[]>('coding_agent_list');
    if (!result.success) {
      setTools([]);
      setError(result.error.message);
      setLoadingTools(false);
      return;
    }

    setTools(result.data);
    setSelectedId((current) => {
      if (current && result.data.some((tool) => tool.id === current && tool.installed)) {
        return current;
      }
      const preferred = readPreference('vibeshell-coding-agent') as CodingAgentId;
      return result.data.find((tool) => tool.id === preferred && tool.installed)?.id
        ?? result.data.find((tool) => tool.installed)?.id
        ?? result.data[0]?.id
        ?? null;
    });
    setLoadingTools(false);
  }, []);

  useEffect(() => {
    void refreshTools();
  }, [refreshTools]);

  useEffect(() => {
    if (initialWorkspace) {
      setWorkspace(initialWorkspace);
    }
  }, [initialWorkspace]);

  const selectedTool = useMemo(
    () => tools.find((tool) => tool.id === selectedId) ?? null,
    [selectedId, tools]
  );
  const promptDeferredToTui = startMode === 'resume_picker'
    && (selectedTool?.id === 'claude' || selectedTool?.id === 'codex');

  useEffect(() => {
    if (!selectedTool) return;
    if (!selectedTool.startModes.includes(startMode)) {
      setStartMode(selectedTool.startModes[0] ?? 'new');
    }
    if (!selectedTool.accessModes.includes(accessMode)) {
      setAccessMode(selectedTool.accessModes[0] ?? 'default');
    }
  }, [accessMode, selectedTool, startMode]);

  const selectTool = useCallback((tool: CodingAgentTool) => {
    setSelectedId(tool.id);
    setError(null);
    if (tool.installed) {
      writePreference('vibeshell-coding-agent', tool.id);
    }
  }, []);

  const pickWorkspace = useCallback(async () => {
    const result = await safeInvoke<string | null>('pick_workspace_directory');
    if (!result.success) {
      setError(result.error.message);
      return;
    }
    if (result.data) {
      setWorkspace(result.data);
      writePreference('vibeshell-coding-workspace', result.data);
      setError(null);
    }
  }, []);

  const handleLaunch = useCallback(async (event: FormEvent) => {
    event.preventDefault();
    if (!selectedTool?.installed || !workspace.trim() || launching) return;

    setLaunching(true);
    setError(null);
    writePreference('vibeshell-coding-workspace', workspace.trim());
    const session = await launchCodingAgentSession({
      agentId: selectedTool.id,
      cwd: workspace.trim(),
      prompt: promptDeferredToTui ? undefined : (prompt.trim() || undefined),
      startMode,
      accessMode,
      cols: 100,
      rows: 30,
    });
    setLaunching(false);

    if (!session) {
      setError(useSessionStore.getState().error ?? t('codingAgent.launchFailed'));
      return;
    }
    onLaunched(session.id);
  }, [
    accessMode,
    launchCodingAgentSession,
    launching,
    onLaunched,
    prompt,
    promptDeferredToTui,
    selectedTool,
    startMode,
    t,
    workspace,
  ]);

  return (
    <div className="coding-agent-launcher grid min-h-[480px] grid-cols-[14rem_minmax(0,1fr)]">
      <aside className="min-w-0 border-r border-tokyo-bg-hl bg-tokyo-bg-dark">
        <div className="flex h-10 items-center justify-between border-b border-tokyo-bg-hl px-3">
          <span className="text-xs font-medium text-tokyo-fg">{t('codingAgent.available')}</span>
          <button
            className="icon-button h-7 w-7"
            onClick={() => { void refreshTools(); }}
            disabled={loadingTools}
            aria-label={t('codingAgent.refresh')}
            title={t('codingAgent.refresh')}
          >
            <RefreshCw className={cn('h-3.5 w-3.5', loadingTools && 'animate-spin')} />
          </button>
        </div>

        <div className="coding-agent-tool-list divide-y divide-tokyo-bg-hl">
          {loadingTools && tools.length === 0
            ? Array.from({ length: 4 }, (_, index) => (
              <div key={index} className="flex h-16 items-center gap-3 px-3" aria-hidden="true">
                <span className="h-8 w-8 animate-pulse rounded-md bg-tokyo-bg-hl" />
                <span className="h-3 w-24 animate-pulse rounded bg-tokyo-bg-hl" />
              </div>
            ))
            : tools.map((tool) => (
              <button
                key={tool.id}
                className={cn(
                  'coding-agent-tool flex min-h-16 w-full items-center gap-3 px-3 text-left transition-colors duration-150',
                  selectedId === tool.id ? 'bg-tokyo-selection' : 'hover:bg-tokyo-bg-hl'
                )}
                onClick={() => selectTool(tool)}
                aria-pressed={selectedId === tool.id}
              >
                <span className={cn(
                  'flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-md',
                  tool.installed ? 'bg-tokyo-bg text-tokyo-blue' : 'bg-tokyo-bg text-tokyo-comment'
                )}>
                  <Bot className="h-4 w-4" aria-hidden="true" />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium text-tokyo-fg">{tool.name}</span>
                  <span className={cn(
                    'mt-0.5 block truncate font-mono text-[10px]',
                    tool.installed ? 'text-tokyo-comment' : 'text-tokyo-red'
                  )}>
                    {tool.installed ? tool.executablePath : t('codingAgent.notFound')}
                  </span>
                </span>
                {tool.installed && selectedId === tool.id && (
                  <Check className="h-3.5 w-3.5 flex-shrink-0 text-tokyo-green" aria-hidden="true" />
                )}
              </button>
            ))}
        </div>
      </aside>

      <form className="flex min-w-0 flex-col bg-tokyo-bg" onSubmit={handleLaunch}>
        <header className="flex h-10 flex-shrink-0 items-center border-b border-tokyo-bg-hl px-4">
          <h3 className="truncate text-sm font-medium text-tokyo-fg">
            {selectedTool?.name ?? t('codingAgent.title')}
          </h3>
        </header>

        <div className="min-h-0 flex-1 space-y-5 overflow-y-auto px-5 py-5">
          <label className="block">
            <span className="mb-1.5 block text-xs font-medium text-tokyo-fg">
              {t('codingAgent.workspace')}
            </span>
            <span className="flex gap-2">
              <input
                value={workspace}
                onChange={(event) => setWorkspace(event.target.value)}
                className="h-9 min-w-0 flex-1 rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark px-3 font-mono text-xs text-tokyo-fg placeholder:text-tokyo-comment focus:outline-none focus:ring-1 focus:ring-tokyo-blue"
                placeholder={t('codingAgent.workspacePlaceholder')}
                spellCheck={false}
              />
              <button
                type="button"
                className="icon-button border border-tokyo-bg-hl bg-tokyo-bg-dark"
                onClick={() => { void pickWorkspace(); }}
                aria-label={t('codingAgent.pickWorkspace')}
                title={t('codingAgent.pickWorkspace')}
              >
                <FolderOpen className="h-4 w-4" />
              </button>
            </span>
          </label>

          <fieldset>
            <legend className="mb-1.5 text-xs font-medium text-tokyo-fg">
              {t('codingAgent.startMode')}
            </legend>
            <div className="flex w-full max-w-md rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark p-0.5">
              {START_MODES.map((mode) => {
                const supported = selectedTool?.startModes.includes(mode) ?? false;
                return (
                  <button
                    key={mode}
                    type="button"
                    className={cn(
                      'h-8 min-w-0 flex-1 px-2 text-xs transition-colors duration-150 first:rounded-l last:rounded-r',
                      startMode === mode && supported
                        ? 'bg-tokyo-selection font-medium text-tokyo-fg'
                        : 'text-tokyo-comment hover:text-tokyo-fg',
                      !supported && 'cursor-not-allowed opacity-40'
                    )}
                    onClick={() => supported && setStartMode(mode)}
                    disabled={!supported}
                    aria-pressed={startMode === mode}
                  >
                    {t(`codingAgent.startModes.${mode}`)}
                  </button>
                );
              })}
            </div>
          </fieldset>

          <fieldset>
            <legend className="mb-1.5 text-xs font-medium text-tokyo-fg">
              {t('codingAgent.accessMode')}
            </legend>
            <div className="flex w-full max-w-md rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark p-0.5">
              {ACCESS_MODES.map((mode) => {
                const supported = selectedTool?.accessModes.includes(mode) ?? false;
                return (
                  <button
                    key={mode}
                    type="button"
                    className={cn(
                      'h-8 min-w-0 flex-1 px-2 text-xs transition-colors duration-150 first:rounded-l last:rounded-r',
                      accessMode === mode && supported
                        ? 'bg-tokyo-selection font-medium text-tokyo-fg'
                        : 'text-tokyo-comment hover:text-tokyo-fg',
                      !supported && 'cursor-not-allowed opacity-40'
                    )}
                    onClick={() => supported && setAccessMode(mode)}
                    disabled={!supported}
                    aria-pressed={accessMode === mode}
                  >
                    {t(`codingAgent.accessModes.${mode}`)}
                  </button>
                );
              })}
            </div>
          </fieldset>

          <label className="block">
            <span className="mb-1.5 block text-xs font-medium text-tokyo-fg">
              {t('codingAgent.prompt')}
            </span>
            <textarea
              value={prompt}
              onChange={(event) => setPrompt(event.target.value)}
              disabled={promptDeferredToTui}
              rows={6}
              className="w-full resize-y rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark px-3 py-2 text-sm leading-5 text-tokyo-fg placeholder:text-tokyo-comment focus:outline-none focus:ring-1 focus:ring-tokyo-blue disabled:cursor-not-allowed disabled:opacity-50"
              placeholder={promptDeferredToTui ? t('codingAgent.promptInTui') : t('codingAgent.promptPlaceholder')}
            />
          </label>

          {error && (
            <p role="alert" className="rounded-md bg-tokyo-red/10 px-3 py-2 text-xs text-tokyo-red">
              {error}
            </p>
          )}
        </div>

        <footer className="flex h-14 flex-shrink-0 items-center justify-end border-t border-tokyo-bg-hl bg-tokyo-bg-dark px-5">
          <button
            type="submit"
            disabled={!selectedTool?.installed || !workspace.trim() || launching}
            className="inline-flex h-9 items-center gap-2 rounded-md bg-tokyo-blue px-4 text-sm font-medium text-tokyo-on-accent transition-colors duration-150 hover:bg-tokyo-blue/85 focus:outline-none focus:ring-2 focus:ring-tokyo-blue/40 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {launching
              ? <Loader2 className="h-4 w-4 animate-spin" />
              : <Play className="h-4 w-4" />}
            {launching ? t('codingAgent.starting') : t('codingAgent.start')}
          </button>
        </footer>
      </form>
    </div>
  );
}
