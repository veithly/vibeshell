import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  AlertCircle,
  Archive,
  Download,
  FileQuestion,
  Folder,
  Loader2,
  RefreshCw,
  RotateCw,
  Save,
  ZoomIn,
  ZoomOut,
} from 'lucide-react';
import { safeInvoke } from '../../lib/tauri';
import { cn } from '../../lib/utils';
import {
  ARCHIVE_PREVIEW_LIMIT_BYTES,
  BINARY_PREVIEW_LIMIT_BYTES,
  TEXT_PREVIEW_LIMIT_BYTES,
  getBrowserMimeType,
  isArchiveListable,
  shouldReadAsBinary,
  type FileViewerKind,
} from '../../lib/fileWorkspace';
import { decodeBase64, listArchiveEntries, type ArchiveEntry } from '../../lib/archivePreview';
import { useFileWorkspaceStore, type FileWorkspaceTab } from '../../stores/fileWorkspaceStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { useRuntimeCapabilitiesStore } from '../../stores/runtimeCapabilitiesStore';
import { FileIcon, getSyntaxLanguage } from '../SftpPanel/FileIcon';
import { CodeEditor } from './CodeEditor';

interface SftpFileContent {
  content: string;
  isBinary: boolean;
  size: number;
  truncated: boolean;
  mimeType: string;
}

interface FileWorkspaceProps {
  tab: FileWorkspaceTab;
  isActive: boolean;
}

function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const unitIndex = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / (1024 ** unitIndex)).toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
}

function localDownloadPath(directory: string, name: string): string {
  const separator = directory.includes('\\') && !directory.includes('/') ? '\\' : '/';
  return `${directory.replace(/[/\\]+$/, '')}${separator}${name}`;
}

function viewerLimit(kind: FileViewerKind): number {
  if (kind === 'text') return TEXT_PREVIEW_LIMIT_BYTES;
  if (kind === 'archive') return ARCHIVE_PREVIEW_LIMIT_BYTES;
  return BINARY_PREVIEW_LIMIT_BYTES;
}

export function FileWorkspace({ tab, isActive }: FileWorkspaceProps) {
  const { t } = useTranslation();
  const setDirty = useFileWorkspaceStore((state) => state.setDirty);
  const pathTransferEnabled = useRuntimeCapabilitiesStore(
    (state) => state.capabilities.directoryTransfer
  );
  const { success: notifySuccess, error: notifyError } = useNotificationStore();
  const [content, setContent] = useState<SftpFileContent | null>(null);
  const [textContent, setTextContent] = useState('');
  const [savedTextContent, setSavedTextContent] = useState('');
  const [binaryBytes, setBinaryBytes] = useState<Uint8Array | null>(null);
  const [objectUrl, setObjectUrl] = useState<string | null>(null);
  const [archiveEntries, setArchiveEntries] = useState<ArchiveEntry[]>([]);
  const [archiveSearch, setArchiveSearch] = useState('');
  const [archiveError, setArchiveError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [zoom, setZoom] = useState(1);
  const [rotation, setRotation] = useState(0);
  const textContentRef = useRef('');

  const { id, sessionId, path, name, kind } = tab;
  const listableArchive = kind === 'archive' && isArchiveListable(name);

  const loadFile = useCallback(async () => {
    setError(null);
    setArchiveError(null);
    setArchiveEntries([]);
    setArchiveSearch('');
    setContent(null);
    setBinaryBytes(null);
    setZoom(1);
    setRotation(0);

    if (kind === 'unsupported' || (kind === 'archive' && !isArchiveListable(name))) {
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    try {
      const result = await safeInvoke<SftpFileContent>('sftp_read_file', {
        request: {
          sessionId,
          path,
          asBinary: shouldReadAsBinary(kind),
          maxSize: viewerLimit(kind),
        },
      });
      if (!result.success) throw new Error(result.error.message);

      if (kind === 'text') {
        setContent(result.data);
        setTextContent(result.data.content);
        setSavedTextContent(result.data.content);
        setDirty(id, false);
      } else if (result.data.isBinary) {
        setContent({ ...result.data, content: '' });
        const bytes = decodeBase64(result.data.content);
        if (kind === 'archive') {
          try {
            setArchiveEntries(await listArchiveEntries(bytes, name));
          } catch (archiveListError) {
            setArchiveError(
              archiveListError instanceof Error ? archiveListError.message : t('fileWorkspace.archiveReadFailed')
            );
          }
        } else {
          setBinaryBytes(bytes);
        }
      } else {
        setContent(result.data);
      }
    } catch (loadError) {
      const message = loadError instanceof Error ? loadError.message : t('fileWorkspace.loadFailed');
      setError(message);
    } finally {
      setIsLoading(false);
    }
  }, [id, kind, name, path, sessionId, setDirty, t]);

  useEffect(() => {
    void loadFile();
  }, [loadFile]);

  useEffect(() => {
    textContentRef.current = textContent;
  }, [textContent]);

  useEffect(() => {
    if (!binaryBytes || !content || !['image', 'pdf', 'video', 'audio'].includes(kind)) {
      setObjectUrl(null);
      return;
    }

    const url = URL.createObjectURL(new Blob(
      [binaryBytes],
      { type: getBrowserMimeType(name, content.mimeType) }
    ));
    setObjectUrl(url);
    return () => URL.revokeObjectURL(url);
  }, [binaryBytes, content, kind, name]);

  const saveFile = useCallback(async () => {
    if (kind !== 'text' || isSaving || textContent === savedTextContent || content?.truncated) return;
    const contentToSave = textContent;
    setIsSaving(true);
    try {
      const result = await safeInvoke('sftp_write_file', {
        request: { sessionId, path, content: contentToSave },
      });
      if (!result.success) throw new Error(result.error.message);
      setSavedTextContent(contentToSave);
      setDirty(id, textContentRef.current !== contentToSave);
      notifySuccess(t('fileWorkspace.saved'), t('fileWorkspace.savedMessage', { name }));
    } catch (saveError) {
      const message = saveError instanceof Error ? saveError.message : t('fileWorkspace.saveFailed');
      notifyError(t('fileWorkspace.saveFailed'), message);
    } finally {
      setIsSaving(false);
    }
  }, [content?.truncated, id, isSaving, kind, name, notifyError, notifySuccess, path, savedTextContent, sessionId, setDirty, t, textContent]);

  useEffect(() => {
    if (!isActive) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's' && kind === 'text') {
        event.preventDefault();
        void saveFile();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isActive, kind, saveFile]);

  const downloadFile = useCallback(async () => {
    try {
      const directoryResult = await safeInvoke<string | null>('pick_download_directory');
      if (!directoryResult.success) throw new Error(directoryResult.error.message);
      if (!directoryResult.data) return;

      const result = await safeInvoke('sftp_download_file', {
        request: {
          sessionId,
          remotePath: path,
          localPath: localDownloadPath(directoryResult.data, name),
        },
      });
      if (!result.success) throw new Error(result.error.message);
      notifySuccess(t('fileWorkspace.downloaded'), t('fileWorkspace.downloadedMessage', { name }));
    } catch (downloadError) {
      const message = downloadError instanceof Error ? downloadError.message : t('fileWorkspace.downloadFailed');
      notifyError(t('fileWorkspace.downloadFailed'), message);
    }
  }, [name, notifyError, notifySuccess, path, sessionId, t]);

  const handleReload = useCallback(() => {
    if (tab.dirty && !window.confirm(t('fileWorkspace.discardChanges'))) return;
    void loadFile();
  }, [loadFile, t, tab.dirty]);

  const filteredArchiveEntries = useMemo(() => {
    const query = archiveSearch.trim().toLowerCase();
    return query
      ? archiveEntries.filter((entry) => entry.path.toLowerCase().includes(query))
      : archiveEntries;
  }, [archiveEntries, archiveSearch]);

  const renderContent = () => {
    if (isLoading) {
      return (
        <div className="flex h-full items-center justify-center gap-3 text-sm text-tokyo-comment">
          <Loader2 className="h-5 w-5 animate-spin text-tokyo-blue" />
          {t('fileWorkspace.loading')}
        </div>
      );
    }

    if (error) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
          <AlertCircle className="h-8 w-8 text-tokyo-red" />
          <p className="max-w-xl text-sm text-tokyo-fg">{error}</p>
          <button className="rounded-md bg-tokyo-bg-hl px-3 py-1.5 text-sm text-tokyo-fg" onClick={handleReload}>
            {t('common.retry')}
          </button>
        </div>
      );
    }

    if (kind === 'text') {
      return (
        <div className="flex h-full min-h-0 flex-col">
          {content?.truncated && (
            <div className="border-b border-tokyo-yellow/40 bg-tokyo-yellow/10 px-4 py-2 text-xs text-tokyo-yellow">
              {t('fileWorkspace.truncated')}
            </div>
          )}
          <div className="min-h-0 flex-1">
            <CodeEditor
              value={textContent}
              language={getSyntaxLanguage(name)}
              readOnly={content?.truncated}
              onChange={(value) => {
                setTextContent(value);
                setDirty(id, value !== savedTextContent);
              }}
            />
          </div>
        </div>
      );
    }

    if (kind === 'image' && objectUrl) {
      return (
        <div className="relative flex h-full items-center justify-center overflow-auto bg-tokyo-bg-dark p-6">
          <img
            src={objectUrl}
            alt={name}
            className="max-h-full max-w-full select-none object-contain transition-transform duration-150"
            style={{ transform: `scale(${zoom}) rotate(${rotation}deg)` }}
          />
          <div className="absolute bottom-4 left-1/2 flex -translate-x-1/2 items-center gap-1 rounded-lg border border-tokyo-bg-hl bg-tokyo-bg/90 p-1">
            <button className="icon-button" onClick={() => setZoom((value) => Math.max(0.25, value - 0.25))} aria-label={t('fileWorkspace.zoomOut')}><ZoomOut className="h-4 w-4" /></button>
            <span className="min-w-14 text-center text-xs text-tokyo-fg">{Math.round(zoom * 100)}%</span>
            <button className="icon-button" onClick={() => setZoom((value) => Math.min(5, value + 0.25))} aria-label={t('fileWorkspace.zoomIn')}><ZoomIn className="h-4 w-4" /></button>
            <button className="icon-button" onClick={() => setRotation((value) => (value + 90) % 360)} aria-label={t('fileWorkspace.rotate')}><RotateCw className="h-4 w-4" /></button>
          </div>
        </div>
      );
    }

    if (kind === 'pdf' && objectUrl) {
      return <iframe src={objectUrl} title={name} className="h-full w-full border-0 bg-tokyo-bg-dark" />;
    }

    if (kind === 'video' && objectUrl) {
      return (
        <div className="flex h-full items-center justify-center bg-tokyo-bg-dark p-6">
          <video src={objectUrl} controls className="max-h-full max-w-full" aria-label={name} />
        </div>
      );
    }

    if (kind === 'audio' && objectUrl) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-6 bg-tokyo-bg-dark p-8">
          <FileIcon filename={name} isDirectory={false} size="lg" className="h-20 w-20" />
          <audio src={objectUrl} controls className="w-full max-w-2xl" aria-label={name} />
        </div>
      );
    }

    if (kind === 'archive') {
      if (!listableArchive) {
        return (
          <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
            <Archive className="h-10 w-10 text-tokyo-orange" />
            <p className="text-sm text-tokyo-fg">{t('fileWorkspace.archiveUnsupported')}</p>
            <p className="text-xs text-tokyo-comment">{t('fileWorkspace.archiveSupportedFormats')}</p>
          </div>
        );
      }
      return (
        <div className="flex h-full min-h-0 flex-col">
          <div className="flex items-center gap-3 border-b border-tokyo-bg-hl bg-tokyo-bg-dark px-4 py-2">
            <Archive className="h-4 w-4 text-tokyo-orange" />
            <span className="text-xs text-tokyo-comment">
              {t('fileWorkspace.archiveEntries', { count: archiveEntries.length })}
            </span>
            <input
              value={archiveSearch}
              onChange={(event) => setArchiveSearch(event.target.value)}
              placeholder={t('fileWorkspace.searchArchive')}
              className="ml-auto w-64 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2.5 py-1 text-xs text-tokyo-fg outline-none focus:border-tokyo-blue"
            />
          </div>
          {archiveError ? (
            <div className="flex flex-1 items-center justify-center p-8 text-sm text-tokyo-red">{archiveError}</div>
          ) : (
            <div className="min-h-0 flex-1 overflow-auto">
              <table className="w-full text-left text-xs">
                <thead className="sticky top-0 bg-tokyo-bg-dark text-tokyo-comment">
                  <tr><th className="px-4 py-2 font-medium">{t('fileWorkspace.path')}</th><th className="w-32 px-4 py-2 font-medium">{t('fileWorkspace.size')}</th></tr>
                </thead>
                <tbody>
                  {filteredArchiveEntries.map((entry, index) => (
                    <tr key={`${entry.path}-${index}`} className="border-t border-tokyo-bg-hl/70 hover:bg-tokyo-bg-hl/40">
                      <td className="px-4 py-2 text-tokyo-fg">
                        <span className="flex items-center gap-2">
                          {entry.isDirectory ? <Folder className="h-4 w-4 text-tokyo-yellow" /> : <FileIcon filename={entry.path} isDirectory={false} />}
                          <span className="font-mono">{entry.path}</span>
                        </span>
                      </td>
                      <td className="px-4 py-2 text-tokyo-comment">{entry.isDirectory ? '—' : formatFileSize(entry.size)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      );
    }

    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
        <FileQuestion className="h-10 w-10 text-tokyo-comment" />
        <p className="text-sm text-tokyo-fg">{t('fileWorkspace.unsupported')}</p>
        <p className="text-xs text-tokyo-comment">{t('fileWorkspace.downloadHint')}</p>
      </div>
    );
  };

  return (
    <section className="flex h-full min-h-0 flex-col bg-tokyo-bg" aria-label={t('fileWorkspace.title')}>
      <header className="flex h-12 flex-shrink-0 items-center gap-3 border-b border-tokyo-bg-hl bg-tokyo-bg-dark px-3">
        <FileIcon filename={name} isDirectory={false} size="lg" />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-sm font-medium text-tokyo-fg">{name}</span>
            {tab.dirty && <span className="h-2 w-2 rounded-full bg-tokyo-orange" title={t('fileWorkspace.unsaved')} />}
          </div>
          <div className="truncate font-mono text-[10px] text-tokyo-comment" title={path}>{path}</div>
        </div>
        <span className="hidden text-xs text-tokyo-comment sm:inline">{formatFileSize(content?.size ?? tab.size)}</span>
        {kind === 'text' && (
          <button
            onClick={() => { void saveFile(); }}
            disabled={!tab.dirty || isSaving || content?.truncated}
            className={cn('icon-button', tab.dirty && !content?.truncated && 'text-tokyo-cyan')}
            aria-label={t('common.save')}
            title={`${t('common.save')} (Ctrl/Cmd+S)`}
          >
            {isSaving ? <Loader2 className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}
          </button>
        )}
        {kind !== 'unsupported' && !(kind === 'archive' && !listableArchive) && (
          <button className="icon-button" onClick={handleReload} aria-label={t('common.refresh')} title={t('common.refresh')}>
            <RefreshCw className="h-4 w-4" />
          </button>
        )}
        {pathTransferEnabled && (
          <button className="icon-button" onClick={() => { void downloadFile(); }} aria-label={t('common.download')} title={t('common.download')}>
            <Download className="h-4 w-4" />
          </button>
        )}
      </header>
      <div className="min-h-0 flex-1">{renderContent()}</div>
    </section>
  );
}

export type { FileWorkspaceProps };
