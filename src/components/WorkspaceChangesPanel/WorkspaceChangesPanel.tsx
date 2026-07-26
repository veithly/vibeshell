import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  AlertCircle,
  FileDiff,
  FileMinus2,
  FilePenLine,
  FilePlus2,
  GitBranch,
  Loader2,
  RefreshCw,
  Replace,
  WrapText,
  X,
} from 'lucide-react';
import { cn } from '../../lib/utils';
import { safeInvoke } from '../../lib/tauri';
import {
  buildDiffModel,
  hasTextDiff,
  limitDiffLines,
  summarizeDiff,
  type DiffLineKind,
} from '../../lib/diffModel';
import type {
  GitFileKind,
  GitWorkspaceDiff,
  GitWorkspaceFile,
  GitWorkspaceStatus,
} from '../../types/codingAgent';

interface WorkspaceChangesPanelProps {
  open: boolean;
  cwd?: string;
  sessionName?: string;
  onClose: () => void;
}

const MAX_RENDERED_DIFF_LINES = 4_000;

function splitPath(path: string): { name: string; directory: string } {
  const separator = path.lastIndexOf('/');
  if (separator === -1) return { name: path, directory: '' };
  return { name: path.slice(separator + 1), directory: path.slice(0, separator) };
}

function FileStatusIcon({ kind }: { kind: GitFileKind }) {
  const className = 'h-3.5 w-3.5 flex-shrink-0';
  switch (kind) {
    case 'added':
    case 'untracked':
      return <FilePlus2 className={cn(className, 'text-tokyo-green')} aria-hidden="true" />;
    case 'deleted':
      return <FileMinus2 className={cn(className, 'text-tokyo-red')} aria-hidden="true" />;
    case 'renamed':
      return <Replace className={cn(className, 'text-tokyo-cyan')} aria-hidden="true" />;
    case 'conflicted':
      return <AlertCircle className={cn(className, 'text-tokyo-yellow')} aria-hidden="true" />;
    case 'modified':
      return <FilePenLine className={cn(className, 'text-tokyo-blue')} aria-hidden="true" />;
  }
}

function diffLineClass(kind: DiffLineKind): string {
  if (kind === 'add') return 'workspace-diff-line-add';
  if (kind === 'delete') return 'workspace-diff-line-delete';
  return 'workspace-diff-line-context';
}

export function WorkspaceChangesPanel({
  open,
  cwd,
  sessionName,
  onClose,
}: WorkspaceChangesPanelProps) {
  const { t } = useTranslation();
  const [status, setStatus] = useState<GitWorkspaceStatus | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [diff, setDiff] = useState<GitWorkspaceDiff | null>(null);
  const [loadingStatus, setLoadingStatus] = useState(false);
  const [loadingDiff, setLoadingDiff] = useState(false);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [diffError, setDiffError] = useState<string | null>(null);
  const [wrapLines, setWrapLines] = useState(false);
  const [diffRefresh, setDiffRefresh] = useState({ version: 0, quiet: false });
  const statusRequestRef = useRef(0);
  const diffRequestRef = useRef(0);
  const diffRequestActiveRef = useRef(false);
  const loadedDiffPathRef = useRef<string | null>(null);

  const loadStatus = useCallback(async (quiet = false) => {
    if (!cwd) return;
    const requestId = ++statusRequestRef.current;
    if (!quiet) {
      setLoadingStatus(true);
    }
    const result = await safeInvoke<GitWorkspaceStatus>('coding_agent_workspace_status', {
      request: { cwd },
    });
    if (requestId !== statusRequestRef.current) return;

    setLoadingStatus(false);
    if (!result.success) {
      setStatus(null);
      setSelectedPath(null);
      setDiff(null);
      setStatusError(result.error.message);
      return;
    }

    setStatusError(null);
    setStatus(result.data);
    setSelectedPath((current) => {
      if (current && result.data.files.some((file) => file.path === current)) return current;
      return result.data.files[0]?.path ?? null;
    });
    if (!quiet || !diffRequestActiveRef.current) {
      setDiffRefresh((current) => ({ version: current.version + 1, quiet }));
    }
  }, [cwd]);

  useEffect(() => {
    statusRequestRef.current += 1;
    diffRequestRef.current += 1;
    diffRequestActiveRef.current = false;
    loadedDiffPathRef.current = null;
    setStatus(null);
    setSelectedPath(null);
    setDiff(null);
    setStatusError(null);
    setDiffError(null);
  }, [cwd]);

  useEffect(() => {
    if (!open || !cwd) return;
    let cancelled = false;
    let timeout: number | undefined;
    const poll = async (quiet = false) => {
      if (!quiet || !document.hidden) {
        await loadStatus(quiet);
      }
      if (cancelled) return;
      timeout = window.setTimeout(() => {
        void poll(true);
      }, 2500);
    };
    void poll();
    return () => {
      cancelled = true;
      if (timeout !== undefined) window.clearTimeout(timeout);
    };
  }, [cwd, loadStatus, open]);

  useEffect(() => {
    if (!open) {
      statusRequestRef.current += 1;
      diffRequestRef.current += 1;
      diffRequestActiveRef.current = false;
    }
  }, [open]);

  useEffect(() => {
    if (!open || !cwd || !selectedPath) {
      setDiff(null);
      setDiffError(null);
      setLoadingDiff(false);
      loadedDiffPathRef.current = null;
      return;
    }

    const requestId = ++diffRequestRef.current;
    diffRequestActiveRef.current = true;
    const quietRefresh = diffRefresh.quiet && loadedDiffPathRef.current === selectedPath;
    if (!quietRefresh) setLoadingDiff(true);
    const load = async () => {
      const result = await safeInvoke<GitWorkspaceDiff>('coding_agent_workspace_diff', {
        request: { cwd, path: selectedPath },
      });
      if (requestId !== diffRequestRef.current) return;
      diffRequestActiveRef.current = false;
      setLoadingDiff(false);
      if (!result.success) {
        setDiff(null);
        loadedDiffPathRef.current = null;
        setDiffError(result.error.message);
        return;
      }
      setDiffError(null);
      loadedDiffPathRef.current = selectedPath;
      setDiff(result.data);
    };
    void load();
  }, [cwd, diffRefresh.quiet, diffRefresh.version, open, selectedPath]);

  const parsedDiff = useMemo(() => buildDiffModel(diff?.content ?? ''), [diff?.content]);
  const visibleDiff = useMemo(
    () => limitDiffLines(parsedDiff, MAX_RENDERED_DIFF_LINES),
    [parsedDiff]
  );
  const diffSummary = useMemo(() => summarizeDiff(parsedDiff), [parsedDiff]);
  const textDiffAvailable = useMemo(() => hasTextDiff(parsedDiff), [parsedDiff]);
  const selectedFile = status?.files.find((file) => file.path === selectedPath) ?? null;

  return (
    <aside
      className={cn(
        'workspace-changes-panel min-h-0 flex-shrink-0 flex-col border-l border-tokyo-bg-hl bg-tokyo-bg',
        open ? 'flex' : 'hidden'
      )}
      aria-label={t('workspaceChanges.title')}
    >
      <header className="flex h-10 flex-shrink-0 items-center gap-2 border-b border-tokyo-bg-hl bg-tokyo-bg-dark px-3">
        <FileDiff className="h-4 w-4 flex-shrink-0 text-tokyo-blue" aria-hidden="true" />
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-sm font-medium text-tokyo-fg">{t('workspaceChanges.title')}</h2>
          {sessionName && <p className="truncate text-[10px] text-tokyo-comment">{sessionName}</p>}
        </div>
        <button
          className={cn('icon-button h-7 w-7', wrapLines && 'is-active')}
          onClick={() => setWrapLines((current) => !current)}
          aria-pressed={wrapLines}
          aria-label={t('workspaceChanges.wrap')}
          title={t('workspaceChanges.wrap')}
        >
          <WrapText className="h-3.5 w-3.5" />
        </button>
        <button
          className="icon-button h-7 w-7"
          onClick={() => { void loadStatus(); }}
          disabled={loadingStatus}
          aria-label={t('common.refresh')}
          title={t('common.refresh')}
        >
          <RefreshCw className={cn('h-3.5 w-3.5', loadingStatus && 'animate-spin')} />
        </button>
        <button
          className="icon-button h-7 w-7"
          onClick={onClose}
          aria-label={t('workspaceChanges.close')}
          title={t('workspaceChanges.close')}
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </header>

      {status && (
        <div className="flex h-8 flex-shrink-0 items-center gap-2 border-b border-tokyo-bg-hl bg-tokyo-bg-dark px-3 font-mono text-[10px] text-tokyo-comment">
          <GitBranch className="h-3 w-3" aria-hidden="true" />
          <span className="min-w-0 flex-1 truncate">{status.branch ?? t('workspaceChanges.detached')}</span>
          <span>{status.files.length}</span>
          {diff && (
            <>
              <span className="text-tokyo-green">+{diffSummary.additions}</span>
              <span className="text-tokyo-red">-{diffSummary.deletions}</span>
            </>
          )}
        </div>
      )}

      {!cwd ? (
        <div className="flex min-h-0 flex-1 items-center justify-center px-6 text-center text-xs text-tokyo-comment">
          {t('workspaceChanges.noWorkspace')}
        </div>
      ) : loadingStatus && !status ? (
        <div className="flex min-h-0 flex-1 items-center justify-center">
          <Loader2 className="h-5 w-5 animate-spin text-tokyo-comment" aria-label={t('common.loading')} />
        </div>
      ) : statusError && !status ? (
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
          <AlertCircle className="h-5 w-5 text-tokyo-red" aria-hidden="true" />
          <p className="max-w-[36ch] text-xs leading-5 text-tokyo-comment">{statusError}</p>
        </div>
      ) : status?.files.length === 0 ? (
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
          <FileDiff className="h-5 w-5 text-tokyo-green" aria-hidden="true" />
          <p className="text-xs font-medium text-tokyo-fg">{t('workspaceChanges.clean')}</p>
        </div>
      ) : (
        <div className="workspace-changes-content grid min-h-0 flex-1 grid-cols-[12rem_minmax(0,1fr)]">
          <ol className="workspace-changes-files min-h-0 overflow-y-auto border-r border-tokyo-bg-hl bg-tokyo-bg-dark">
            {status?.files.map((file: GitWorkspaceFile) => {
              const path = splitPath(file.path);
              return (
                <li key={file.path}>
                  <button
                    className={cn(
                      'flex min-h-11 w-full items-start gap-2 border-b border-tokyo-bg-hl px-2.5 py-2 text-left transition-colors duration-150',
                      selectedPath === file.path ? 'bg-tokyo-selection' : 'hover:bg-tokyo-bg-hl'
                    )}
                    onClick={() => setSelectedPath(file.path)}
                    aria-current={selectedPath === file.path ? 'true' : undefined}
                  >
                    <span className="mt-0.5 flex h-5 w-5 flex-shrink-0 items-center justify-center">
                      <FileStatusIcon kind={file.kind} />
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-mono text-[11px] text-tokyo-fg">{path.name}</span>
                      {path.directory && (
                        <span className="mt-0.5 block truncate font-mono text-[9px] text-tokyo-comment">
                          {path.directory}
                        </span>
                      )}
                      <span className="sr-only">
                        {t(`workspaceChanges.fileKinds.${file.kind}`)}.
                        {file.staged ? ` ${t('workspaceChanges.staged')}.` : ''}
                        {file.unstaged ? ` ${t('workspaceChanges.unstaged')}.` : ''}
                      </span>
                    </span>
                    <span className="mt-1 flex flex-shrink-0 items-center gap-1" aria-hidden="true">
                      {file.staged && (
                        <span className="h-1.5 w-1.5 rounded-full bg-tokyo-green" title={t('workspaceChanges.staged')} />
                      )}
                      {file.unstaged && file.staged && (
                        <span className="h-1.5 w-1.5 rounded-full border border-tokyo-blue" title={t('workspaceChanges.unstaged')} />
                      )}
                    </span>
                  </button>
                </li>
              );
            })}
          </ol>

          <div className="flex min-h-0 min-w-0 flex-col bg-tokyo-bg">
            {selectedFile && (
              <div className="flex h-9 flex-shrink-0 items-center gap-2 border-b border-tokyo-bg-hl bg-tokyo-bg-dark px-3">
                <FileStatusIcon kind={selectedFile.kind} />
                <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-tokyo-fg">
                  {selectedFile.path}
                </span>
              </div>
            )}

            {diffError ? (
              <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
                <AlertCircle className="h-5 w-5 text-tokyo-red" aria-hidden="true" />
                <p className="max-w-[42ch] text-xs leading-5 text-tokyo-comment">{diffError}</p>
              </div>
            ) : loadingDiff ? (
              <div className="flex min-h-0 flex-1 items-center justify-center">
                <Loader2 className="h-5 w-5 animate-spin text-tokyo-comment" aria-label={t('common.loading')} />
              </div>
            ) : !textDiffAvailable ? (
              <div className="flex min-h-0 flex-1 items-center justify-center px-6 text-center text-xs text-tokyo-comment">
                {t('workspaceChanges.noTextDiff')}
              </div>
            ) : (
              <div className="min-h-0 flex-1 overflow-auto font-mono text-[11px] leading-5">
                {visibleDiff.files.map((file, fileIndex) => (
                  <div key={`${file.from ?? ''}-${file.to ?? ''}-${fileIndex}`}>
                    {file.hunks.map((hunk, hunkIndex) => (
                      <div key={`${hunk.header}-${hunkIndex}`}>
                        <div className={cn(
                          'workspace-diff-hunk sticky top-0 z-10 grid border-y border-tokyo-bg-hl text-tokyo-blue',
                          wrapLines
                            ? 'min-w-0 grid-cols-[3rem_3rem_1.25rem_minmax(0,1fr)]'
                            : 'min-w-max grid-cols-[3rem_3rem_1.25rem_minmax(20rem,1fr)]'
                        )}>
                          <span />
                          <span />
                          <span className="col-span-2 px-2">{hunk.header}</span>
                        </div>
                        {hunk.lines.map((line, lineIndex) => (
                          <div
                            key={`${hunkIndex}-${lineIndex}`}
                            className={cn(
                              'workspace-diff-line grid',
                              wrapLines
                                ? 'min-w-0 grid-cols-[3rem_3rem_1.25rem_minmax(0,1fr)]'
                                : 'min-w-max grid-cols-[3rem_3rem_1.25rem_minmax(20rem,1fr)]',
                              diffLineClass(line.kind)
                            )}
                          >
                            <span className="workspace-diff-line-number">{line.oldLine ?? ''}</span>
                            <span className="workspace-diff-line-number">{line.newLine ?? ''}</span>
                            <span className={cn(
                              'select-none text-center font-semibold',
                              line.kind === 'add' && 'text-tokyo-green',
                              line.kind === 'delete' && 'text-tokyo-red'
                            )}>
                              {line.marker}
                            </span>
                            <code className={cn('px-2', wrapLines ? 'whitespace-pre-wrap break-all' : 'whitespace-pre')}>
                              {line.content || ' '}
                            </code>
                          </div>
                        ))}
                      </div>
                    ))}
                  </div>
                ))}
                {(diff?.truncated || visibleDiff.truncated) && (
                  <div className="border-t border-tokyo-bg-hl px-3 py-2 text-xs text-tokyo-yellow">
                    {t('workspaceChanges.truncated')}
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      )}
    </aside>
  );
}
