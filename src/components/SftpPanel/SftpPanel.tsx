import { useState, useEffect, useImperativeHandle, forwardRef, useCallback, useRef, useMemo, useLayoutEffect } from 'react';
import { createPortal } from 'react-dom';
import type { SessionType } from '../../stores/sessionStore';
import {
  ChevronUp,
  ChevronDown,
  Maximize2,
  Minimize2,
  Folder,
  FolderPlus,
  Upload,
  Download,
  Trash2,
  Edit3,
  RefreshCw,
  Home,
  ChevronRight,
  AlertCircle,
  Loader2,
  GripHorizontal,
  Archive,
  FolderOpen,
  CheckSquare,
  Eye,
  Check,
  Minus,
  Copy,
  Columns3,
  Grid2X2,
  List,
  GripVertical,
  MoreHorizontal,
  X,
} from 'lucide-react';
import { cn } from '../../lib/utils';
import { safeInvoke } from '../../lib/tauri';
import { useNotificationStore } from '../../stores/notificationStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useRuntimeCapabilitiesStore } from '../../stores/runtimeCapabilitiesStore';
import { useFileWorkspaceStore } from '../../stores/fileWorkspaceStore';
import { ConfirmDialog } from '../ConfirmDialog';
import { FileIcon } from './FileIcon';

function usesCoarsePointer(): boolean {
  return typeof window !== 'undefined'
    && typeof window.matchMedia === 'function'
    && window.matchMedia('(pointer: coarse)').matches;
}

/**
 * Archive format types supported for compression
 */
type ArchiveFormat = 'tar.gz' | 'zip';
type SftpDock = 'bottom' | 'right';
type SftpViewMode = 'details' | 'columns' | 'icons';

/**
 * Represents a file or directory in the SFTP listing
 */
export interface SftpEntry {
  name: string;
  path: string;
  isDirectory: boolean;
  size: number;
  modifiedAt: number;
  permissions: string;
}

interface SftpColumn {
  path: string;
  entries: SftpEntry[];
}

interface DirectoryTransferSummary {
  mode: 'upload' | 'sync';
  localRoot: string;
  remoteRoot: string;
  directoriesTotal: number;
  filesTotal: number;
  createdDirectories: number;
  uploadedFiles: number;
  skippedFiles: number;
  deletedEntries: number;
  transferredBytes: number;
}

interface TransferBatchState {
  operation: 'upload' | 'download';
  status: 'running' | 'completed' | 'failed';
  current: number;
  total: number;
  currentName: string;
  completed: number;
  failed: number;
}

/**
 * Handle exposed by SftpPanel for external control
 */
export interface SftpPanelHandle {
  expand: () => void;
  collapse: () => void;
  toggle: () => void;
  isCollapsed: () => boolean;
  enterFullscreen: () => void;
  exitFullscreen: () => void;
  toggleFullscreen: () => void;
  isFullscreen: () => boolean;
}

/**
 * Context menu position and visibility state
 */
interface ContextMenuState {
  visible: boolean;
  x: number;
  y: number;
}

interface SftpPanelProps {
  /** Session ID for the SFTP connection */
  sessionId?: string;
  /** Session type to distinguish local vs ssh behavior */
  sessionType?: SessionType;
  /** Whether the panel is initially collapsed */
  defaultCollapsed?: boolean;
  /** Default height of the panel in pixels */
  defaultHeight?: number;
  /** Minimum height of the panel in pixels */
  minHeight?: number;
  /** Maximum height of the panel in pixels (or percentage of viewport) */
  maxHeight?: number;
  /** Dock the browser under the terminal or as a right-side inspector. */
  dock?: SftpDock;
  /** Default width when docked on the right. */
  defaultWidth?: number;
  minWidth?: number;
  maxWidth?: number;
  /** Callback when expand button is clicked */
  onExpand?: () => void;
  /** Callback when fullscreen state changes */
  onFullscreenChange?: (isFullscreen: boolean) => void;
  /** Notifies the top-level SFTP command button about panel visibility. */
  onCollapsedChange?: (isCollapsed: boolean) => void;
}

/**
 * Format file size in human readable format
 */
function formatFileSize(bytes: number): string {
  if (bytes === 0) return '-';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`;
}

/**
 * Format timestamp to date string
 */
function formatDate(timestamp: number): string {
  if (timestamp === 0) return '-';
  const date = new Date(timestamp * 1000);
  return date.toLocaleDateString() + ' ' + date.toLocaleTimeString();
}

/**
 * Check if a file is an archive that can be extracted
 */
function isArchiveFile(name: string): boolean {
  const lowerName = name.toLowerCase();
  return (
    lowerName.endsWith('.tar.gz') ||
    lowerName.endsWith('.tgz') ||
    lowerName.endsWith('.tar.bz2') ||
    lowerName.endsWith('.tar.xz') ||
    lowerName.endsWith('.tar') ||
    lowerName.endsWith('.zip') ||
    lowerName.endsWith('.gz') ||
    lowerName.endsWith('.bz2') ||
    lowerName.endsWith('.xz') ||
    lowerName.endsWith('.7z') ||
    lowerName.endsWith('.rar')
  );
}

function basename(path: string): string {
  return path.split(/[/\\]/).filter(Boolean).pop() || 'folder';
}

function joinRemotePath(parent: string, child: string): string {
  if (!parent || parent === '/') return `/${child}`;
  return `${parent.replace(/\/+$/, '')}/${child}`;
}

function joinLocalPath(parent: string, child: string): string {
  const separator = parent.includes('\\') && !parent.includes('/') ? '\\' : '/';
  return `${parent.replace(/[/\\]+$/, '')}${separator}${child}`;
}

function summarizeFailures(failures: string[]): string {
  const visible = failures.slice(0, 3).join('; ');
  return failures.length > 3 ? `${visible}; and ${failures.length - 3} more` : visible;
}

function getErrorMessage(err: unknown, fallback: string): string {
  return err instanceof Error ? err.message : fallback;
}

function sortDirectoryEntries(entries: SftpEntry[]): SftpEntry[] {
  return [...entries].sort((a, b) => {
    if (a.isDirectory && !b.isDirectory) return -1;
    if (!a.isDirectory && b.isDirectory) return 1;
    return a.name.localeCompare(b.name);
  });
}

function isSftpConnectionError(message: string): boolean {
  const normalized = message.toLowerCase();
  return [
    'sftp not initialized',
    'ssh client not connected',
    'failed to open sftp',
    'subsystem',
    'connection closed',
    'connection reset',
    'broken pipe',
    'channel',
    'session not found',
    'eof',
  ].some((needle) => normalized.includes(needle));
}

function loadSftpViewMode(): SftpViewMode {
  try {
    const stored = globalThis.localStorage?.getItem('vibeshell-sftp-view');
    if (stored === 'columns' || stored === 'icons') return stored;
  } catch {
    // Storage can be unavailable in hardened webviews and test environments.
  }
  return 'details';
}

/**
 * Collapsible SFTP file browser panel with full functionality
 */
export const SftpPanel = forwardRef<SftpPanelHandle, SftpPanelProps>(function SftpPanel(
  {
    sessionId,
    sessionType = 'ssh',
    defaultCollapsed = true,
    defaultHeight = 256,
    minHeight = 150,
    maxHeight = 600,
    dock = 'bottom',
    defaultWidth = 420,
    minWidth = 320,
    maxWidth = 720,
    onExpand: _onExpand,
    onFullscreenChange,
    onCollapsedChange,
  },
  ref
) {
  const [isCollapsed, setIsCollapsed] = useState(defaultCollapsed);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [panelHeight, setPanelHeight] = useState(defaultHeight);
  const [panelWidth, setPanelWidth] = useState(defaultWidth);
  const [viewMode, setViewMode] = useState<SftpViewMode>(loadSftpViewMode);
  const [currentPath, setCurrentPath] = useState<string>('~');
  const [entries, setEntries] = useState<SftpEntry[]>([]);
  const [columns, setColumns] = useState<SftpColumn[]>([]);
  const [columnSelections, setColumnSelections] = useState<Record<string, string>>({});
  const [selectedEntries, setSelectedEntries] = useState<Set<string>>(new Set());
  const [lastSelectedIndex, setLastSelectedIndex] = useState<number | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [transferBatch, setTransferBatch] = useState<TransferBatchState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isInitialized, setIsInitialized] = useState(false);
  const uploadIgnoreConfig = useSettingsStore((state) => state.uploadIgnoreConfig);
  const pathTransferEnabled = useRuntimeCapabilitiesStore(
    (state) => state.capabilities.directoryTransfer
  );

  // Context menu state
  const [contextMenu, setContextMenu] = useState<ContextMenuState>({ visible: false, x: 0, y: 0 });
  const [isToolbarMenuOpen, setIsToolbarMenuOpen] = useState(false);

  // Resize drag state
  const [isDragging, setIsDragging] = useState(false);
  const dragStartPoint = useRef<number>(0);
  const dragStartSize = useRef<number>(0);
  const panelRef = useRef<HTMLDivElement>(null);

  // Rename dialog state
  const [renameEntry, setRenameEntry] = useState<SftpEntry | null>(null);
  const [newName, setNewName] = useState('');

  // New folder dialog state
  const [showNewFolderDialog, setShowNewFolderDialog] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');

  // Use an in-app confirmation dialog because native window.confirm is not
  // reliably surfaced by the Tauri webview in packaged desktop builds.
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);

  // Compress dialog state
  const [showCompressDialog, setShowCompressDialog] = useState(false);
  const [compressFormat, setCompressFormat] = useState<ArchiveFormat>('tar.gz');
  const [archiveName, setArchiveName] = useState('');

  // Drag-and-drop state
  const [isDragOver, setIsDragOver] = useState(false);
  const fileListRef = useRef<HTMLDivElement>(null);
  const columnsScrollerRef = useRef<HTMLDivElement>(null);
  const columnLoadRequestRef = useRef(0);
  const directoryLoadRequestRef = useRef(0);
  const directoryFetchesRef = useRef<Map<string, Promise<SftpEntry[]>>>(new Map());
  const contextMenuRef = useRef<HTMLDivElement>(null);
  const columnsRef = useRef<SftpColumn[]>([]);
  const viewModeRef = useRef<SftpViewMode>(viewMode);

  const { success: notifySuccess, error: notifyError } = useNotificationStore();
  const openFile = useFileWorkspaceStore((state) => state.openFile);

  useEffect(() => {
    columnsRef.current = columns;
  }, [columns]);

  useEffect(() => {
    viewModeRef.current = viewMode;
  }, [viewMode]);

  // Get selected entry for single selection operations (backward compatibility)
  const selectedEntry = useMemo(() => {
    if (selectedEntries.size === 1) {
      const path = Array.from(selectedEntries)[0];
      return entries.find(e => e.path === path) || null;
    }
    return null;
  }, [selectedEntries, entries]);

  // Get all selected entries as array
  const selectedEntriesArray = useMemo(() => {
    return entries.filter(e => selectedEntries.has(e.path));
  }, [selectedEntries, entries]);

  const selectedFiles = useMemo(() => {
    return selectedEntriesArray.filter((entry) => !entry.isDirectory);
  }, [selectedEntriesArray]);

  // Check if any selected entries are archives (for extract option)
  const hasSelectedArchives = useMemo(() => {
    return selectedEntriesArray.some(e => !e.isDirectory && isArchiveFile(e.name));
  }, [selectedEntriesArray]);

  // Any regular file opens in a workspace tab. Unsupported formats still get
  // a useful metadata/download view instead of doing nothing.
  const canOpenSelected = useMemo(() => {
    return selectedEntry && !selectedEntry.isDirectory;
  }, [selectedEntry]);

  // Reset SFTP browser state whenever the owning terminal session changes.
  useEffect(() => {
    setIsInitialized(false);
    setEntries([]);
    setColumns([]);
    setColumnSelections({});
    setSelectedEntries(new Set());
    setLastSelectedIndex(null);
    setCurrentPath('~');
    setError(null);
    columnLoadRequestRef.current += 1;
    directoryLoadRequestRef.current += 1;
    directoryFetchesRef.current.clear();
    setShowDeleteDialog(false);

    if (!sessionId) {
      setIsCollapsed(true);
      setIsFullscreen(false);
    }
  }, [sessionId]);

  // Handle fullscreen change callback
  useEffect(() => {
    onFullscreenChange?.(isFullscreen);
  }, [isFullscreen, onFullscreenChange]);

  useEffect(() => {
    onCollapsedChange?.(isCollapsed);
  }, [isCollapsed, onCollapsedChange]);

  // Close context menu on click outside
  useEffect(() => {
    const handleClickOutside = () => {
      if (contextMenu.visible) {
        setContextMenu({ visible: false, x: 0, y: 0 });
      }
      setIsToolbarMenuOpen(false);
    };
    document.addEventListener('click', handleClickOutside);
    return () => document.removeEventListener('click', handleClickOutside);
  }, [contextMenu.visible]);

  useLayoutEffect(() => {
    if (!contextMenu.visible || !contextMenuRef.current) return;

    const viewportMargin = 8;
    const fitMenuToViewport = () => {
      const menu = contextMenuRef.current;
      if (!menu) return;

      const rect = menu.getBoundingClientRect();
      const maxX = Math.max(viewportMargin, window.innerWidth - rect.width - viewportMargin);
      const maxY = Math.max(viewportMargin, window.innerHeight - rect.height - viewportMargin);

      setContextMenu((previous) => {
        if (!previous.visible) return previous;
        const x = Math.min(Math.max(viewportMargin, previous.x), maxX);
        const y = Math.min(Math.max(viewportMargin, previous.y), maxY);
        return x === previous.x && y === previous.y ? previous : { ...previous, x, y };
      });
    };

    fitMenuToViewport();
    window.addEventListener('resize', fitMenuToViewport);
    return () => window.removeEventListener('resize', fitMenuToViewport);
  }, [contextMenu.visible, contextMenu.x, contextMenu.y]);

  // Handle resize dragging
  useEffect(() => {
    if (!isDragging) return;

    const stopDragging = () => {
      setIsDragging(false);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };

    const handleMouseMove = (e: MouseEvent) => {
      if (dock === 'right') {
        const deltaX = dragStartPoint.current - e.clientX;
        setPanelWidth(Math.min(maxWidth, Math.max(minWidth, dragStartSize.current + deltaX)));
      } else {
        const deltaY = dragStartPoint.current - e.clientY;
        setPanelHeight(Math.min(maxHeight, Math.max(minHeight, dragStartSize.current + deltaY)));
      }
    };

    const handleVisibilityChange = () => {
      if (document.hidden) stopDragging();
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', stopDragging);
    document.addEventListener('mouseleave', stopDragging);
    document.addEventListener('visibilitychange', handleVisibilityChange);
    window.addEventListener('blur', stopDragging);
    document.body.style.cursor = dock === 'right' ? 'ew-resize' : 'ns-resize';
    document.body.style.userSelect = 'none';

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', stopDragging);
      document.removeEventListener('mouseleave', stopDragging);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      window.removeEventListener('blur', stopDragging);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
  }, [dock, isDragging, minHeight, maxHeight, minWidth, maxWidth]);

  const expandPanel = useCallback(() => setIsCollapsed(false), []);
  const collapsePanel = useCallback(() => {
    setIsFullscreen(false);
    setIsCollapsed(true);
  }, []);
  const togglePanel = useCallback(() => {
    setIsFullscreen(false);
    setIsCollapsed((previous) => !previous);
  }, []);
  const enterFullscreen = useCallback(() => {
    setIsCollapsed(false);
    setIsFullscreen(true);
  }, []);
  const exitFullscreen = useCallback(() => setIsFullscreen(false), []);
  const togglePanelFullscreen = useCallback(() => {
    if (isFullscreen) {
      exitFullscreen();
    } else {
      enterFullscreen();
    }
  }, [enterFullscreen, exitFullscreen, isFullscreen]);

  // Expose methods to parent via ref while preserving valid panel states.
  useImperativeHandle(ref, () => ({
    expand: expandPanel,
    collapse: collapsePanel,
    toggle: togglePanel,
    isCollapsed: () => isCollapsed,
    enterFullscreen,
    exitFullscreen,
    toggleFullscreen: togglePanelFullscreen,
    isFullscreen: () => isFullscreen,
  }), [collapsePanel, enterFullscreen, exitFullscreen, expandPanel, isCollapsed, isFullscreen, togglePanel, togglePanelFullscreen]);

  const fetchDirectoryEntries = useCallback((path: string): Promise<SftpEntry[]> => {
    if (!sessionId) return Promise.resolve([]);

    const requestKey = `${sessionId}\0${path}`;
    const pendingRequest = directoryFetchesRef.current.get(requestKey);
    if (pendingRequest) return pendingRequest;

    const request = (async () => {
      const result = await safeInvoke<SftpEntry[]>('sftp_list_dir', {
        request: {
          sessionId,
          path,
        },
      });

      if (!result.success) {
        throw new Error(result.error.message);
      }
      return sortDirectoryEntries(result.data);
    })();

    directoryFetchesRef.current.set(requestKey, request);
    const clearRequest = () => {
      if (directoryFetchesRef.current.get(requestKey) === request) {
        directoryFetchesRef.current.delete(requestKey);
      }
    };
    void request.then(clearRequest, clearRequest);
    return request;
  }, [sessionId]);

  const loadDirectory = useCallback(async (path: string) => {
    if (!sessionId) return;

    const requestId = ++directoryLoadRequestRef.current;
    setIsLoading(true);
    setError(null);
    setSelectedEntries(new Set());
    setLastSelectedIndex(null);

    try {
      const sorted = await fetchDirectoryEntries(path);
      if (requestId !== directoryLoadRequestRef.current) return;
      setEntries(sorted);
      setCurrentPath(path);
      const existingColumnIndex = viewModeRef.current === 'columns'
        ? columnsRef.current.findIndex((column) => column.path === path)
        : -1;
      if (existingColumnIndex >= 0) {
        setColumns([
          ...columnsRef.current.slice(0, existingColumnIndex),
          { path, entries: sorted },
        ]);
        setColumnSelections((previous) => {
          const next: Record<string, string> = {};
          columnsRef.current.slice(0, existingColumnIndex).forEach((column) => {
            if (previous[column.path]) next[column.path] = previous[column.path];
          });
          return next;
        });
      } else {
        setColumns([{ path, entries: sorted }]);
        setColumnSelections({});
      }
    } catch (err) {
      if (requestId !== directoryLoadRequestRef.current) return;
      console.error('[SftpPanel] Failed to load directory:', err);
      const message = getErrorMessage(err, 'Failed to load directory');
      if (isSftpConnectionError(message)) {
        setIsInitialized(false);
      }
      setError(message);
    } finally {
      if (requestId === directoryLoadRequestRef.current) {
        setIsLoading(false);
      }
    }
  }, [fetchDirectoryEntries, sessionId]);

  const initializeSftp = useCallback(async () => {
    if (!sessionId) return;

    setIsLoading(true);
    setError(null);

    try {
      // Initialize SFTP session
      const initResult = await safeInvoke<boolean>('sftp_init', {
        request: {
          sessionId: sessionId,
        },
      });

      if (!initResult.success) {
        throw new Error(initResult.error.message);
      }

      // Get current working directory
      const pwdResult = await safeInvoke<string>('sftp_pwd', {
        request: {
          sessionId: sessionId,
        },
      });

      let initialPath = '~';
      if (pwdResult.success && pwdResult.data) {
        initialPath = pwdResult.data;
        setCurrentPath(initialPath);
      }

      setIsInitialized(true);

      // Load directory listing
      await loadDirectory(initialPath);
    } catch (err) {
      console.error('[SftpPanel] Failed to initialize SFTP:', err);
      setIsInitialized(false);
      setEntries([]);
      setError(getErrorMessage(err, 'Failed to initialize SFTP'));
    } finally {
      setIsLoading(false);
    }
  }, [sessionId, loadDirectory]);

  // Initialize SFTP and load initial directory when expanded
  useEffect(() => {
    if (sessionId && !isCollapsed && !isInitialized) {
      initializeSftp();
    }
  }, [sessionId, isCollapsed, isInitialized, initializeSftp]);

  const navigateUp = useCallback(() => {
    if (currentPath === '/' || currentPath === '~') return;

    const parts = currentPath.split('/').filter(Boolean);
    parts.pop();
    const parentPath = parts.length > 0 ? '/' + parts.join('/') : '/';
    loadDirectory(parentPath);
  }, [currentPath, loadDirectory]);

  const navigateToEntry = useCallback((entry: SftpEntry) => {
    if (entry.isDirectory) {
      loadDirectory(entry.path);
    }
  }, [loadDirectory]);

  const handleColumnEntryClick = useCallback(async (
    column: SftpColumn,
    columnIndex: number,
    entry: SftpEntry
  ) => {
    setColumnSelections((previous) => {
      const next: Record<string, string> = {};
      columns.slice(0, columnIndex).forEach(({ path }) => {
        if (previous[path]) next[path] = previous[path];
      });
      next[column.path] = entry.path;
      return next;
    });
    setColumns((previous) => previous.slice(0, columnIndex + 1));
    setCurrentPath(column.path);
    setEntries(column.entries);
    setSelectedEntries(new Set(entry.isDirectory ? [] : [entry.path]));
    setLastSelectedIndex(null);

    if (!entry.isDirectory) return;

    const requestId = ++columnLoadRequestRef.current;
    setIsLoading(true);
    setError(null);
    try {
      const childEntries = await fetchDirectoryEntries(entry.path);
      if (requestId !== columnLoadRequestRef.current) return;
      setColumns((previous) => [
        ...previous.slice(0, columnIndex + 1),
        { path: entry.path, entries: childEntries },
      ]);
      setCurrentPath(entry.path);
      setEntries(childEntries);
      setSelectedEntries(new Set());
    } catch (err) {
      if (requestId !== columnLoadRequestRef.current) return;
      const message = getErrorMessage(err, 'Failed to load directory');
      if (isSftpConnectionError(message)) setIsInitialized(false);
      setError(message);
    } finally {
      if (requestId === columnLoadRequestRef.current) setIsLoading(false);
    }
  }, [columns, fetchDirectoryEntries]);

  useEffect(() => {
    if (viewMode !== 'columns') return;
    requestAnimationFrame(() => {
      const scroller = columnsScrollerRef.current;
      if (scroller) scroller.scrollLeft = scroller.scrollWidth;
    });
  }, [columns.length, viewMode]);

  const handleRefresh = useCallback(() => {
    if (!isInitialized) {
      initializeSftp();
      return;
    }
    loadDirectory(currentPath);
  }, [currentPath, isInitialized, initializeSftp, loadDirectory]);

  const handleGoHome = useCallback(() => {
    loadDirectory('~');
  }, [loadDirectory]);

  const handleUpload = useCallback(async () => {
    if (!sessionId) return;

    try {
      const pickResult = await safeInvoke<string[]>('pick_files_for_upload');

      if (!pickResult.success) throw new Error(pickResult.error.message);
      if (pickResult.data.length === 0) return;

      const paths = pickResult.data;
      const failures: string[] = [];
      let completed = 0;

      setIsLoading(true);
      setTransferBatch({
        operation: 'upload',
        status: 'running',
        current: 1,
        total: paths.length,
        currentName: basename(paths[0]),
        completed: 0,
        failed: 0,
      });

      for (const [index, localPath] of paths.entries()) {
        const fileName = basename(localPath);
        setTransferBatch((previous) => previous ? {
          ...previous,
          current: index + 1,
          currentName: fileName,
          completed,
          failed: failures.length,
        } : previous);

        try {
          const uploadResult = await safeInvoke('sftp_upload_file', {
            request: {
              sessionId,
              localPath,
              remotePath: joinRemotePath(currentPath, fileName),
            },
          });
          if (uploadResult.success) completed += 1;
          else failures.push(`${fileName}: ${uploadResult.error.message}`);
        } catch (err) {
          failures.push(`${fileName}: ${getErrorMessage(err, 'Upload failed')}`);
        }
      }

      setTransferBatch({
        operation: 'upload',
        status: failures.length > 0 ? 'failed' : 'completed',
        current: paths.length,
        total: paths.length,
        currentName: basename(paths[paths.length - 1]),
        completed,
        failed: failures.length,
      });

      if (completed > 0) notifySuccess('Upload Complete', `${completed} file(s) uploaded successfully`);
      if (failures.length > 0) notifyError('Upload Failed', summarizeFailures(failures));
      if (completed > 0) await loadDirectory(currentPath);
    } catch (err) {
      console.error('[SftpPanel] Upload failed:', err);
      notifyError('Upload Failed', err instanceof Error ? err.message : 'Failed to upload file');
    } finally {
      setIsLoading(false);
    }
  }, [sessionId, currentPath, loadDirectory, notifySuccess, notifyError]);

  const uploadDirectory = useCallback(async (
    localPath: string,
    remotePath: string,
    mode: 'upload' | 'sync'
  ): Promise<DirectoryTransferSummary> => {
    const result = await safeInvoke<DirectoryTransferSummary>('sftp_upload_directory', {
      request: {
        sessionId,
        localPath,
        remotePath,
        mode,
        deleteExtra: false,
        respectGitignore: uploadIgnoreConfig.respectGitignore,
        excludedPaths: uploadIgnoreConfig.excludedPaths,
      },
    });

    if (!result.success) {
      throw new Error(result.error.message);
    }
    return result.data;
  }, [sessionId, uploadIgnoreConfig]);

  const handleUploadDirectory = useCallback(async (mode: 'upload' | 'sync') => {
    if (!sessionId) return;

    try {
      const pickResult = await safeInvoke<string | null>('pick_directory_for_upload');
      if (!pickResult.success || !pickResult.data) return;

      const localPath = pickResult.data;
      const folderName = basename(localPath);
      const remotePath = mode === 'sync' ? currentPath : joinRemotePath(currentPath, folderName);

      setIsLoading(true);
      const summary = await uploadDirectory(localPath, remotePath, mode);
      notifySuccess(
        mode === 'sync' ? 'Sync Complete' : 'Upload Complete',
        `${summary.uploadedFiles} uploaded, ${summary.skippedFiles} skipped`
      );
      await loadDirectory(currentPath);
    } catch (err) {
      console.error('[SftpPanel] Directory transfer failed:', err);
      notifyError(
        'Directory Transfer Failed',
        err instanceof Error ? err.message : 'Failed to transfer directory'
      );
    } finally {
      setIsLoading(false);
    }
  }, [sessionId, currentPath, uploadDirectory, loadDirectory, notifySuccess, notifyError]);

  // Handle files dropped via drag-and-drop
  const handleDropFiles = useCallback(async (paths: string[]) => {
    if (!sessionId || paths.length === 0) return;

    setIsLoading(true);
    let successCount = 0;
    let failCount = 0;
    setTransferBatch({
      operation: 'upload',
      status: 'running',
      current: 1,
      total: paths.length,
      currentName: basename(paths[0]),
      completed: 0,
      failed: 0,
    });

    for (const [index, localPath] of paths.entries()) {
      const fileName = localPath.split(/[/\\]/).pop() || 'file';
      const remotePath = joinRemotePath(currentPath, fileName);
      setTransferBatch((previous) => previous ? {
        ...previous,
        current: index + 1,
        currentName: fileName,
        completed: successCount,
        failed: failCount,
      } : previous);

      try {
        const result = await safeInvoke('sftp_upload_file', {
          request: {
            sessionId,
            localPath,
            remotePath,
          },
        });

        if (result.success) {
          successCount++;
        } else {
          try {
            await uploadDirectory(localPath, remotePath, 'upload');
            successCount++;
          } catch (directoryErr) {
            failCount++;
            console.error(
              `[SftpPanel] Upload failed for ${fileName}:`,
              result.error.message,
              directoryErr
            );
          }
        }
      } catch (err) {
        try {
          await uploadDirectory(localPath, remotePath, 'upload');
          successCount++;
        } catch (directoryErr) {
          failCount++;
          console.error(`[SftpPanel] Upload failed for ${fileName}:`, err, directoryErr);
        }
      }
    }

    setTransferBatch({
      operation: 'upload',
      status: failCount > 0 ? 'failed' : 'completed',
      current: paths.length,
      total: paths.length,
      currentName: basename(paths[paths.length - 1]),
      completed: successCount,
      failed: failCount,
    });

    if (successCount > 0) {
      notifySuccess('Upload Complete', `${successCount} file(s) uploaded successfully`);
    }
    if (failCount > 0) {
      notifyError('Upload Failed', `${failCount} file(s) failed to upload`);
    }

    await loadDirectory(currentPath);
    setIsLoading(false);
  }, [sessionId, currentPath, loadDirectory, notifySuccess, notifyError, uploadDirectory]);

  // Drag-and-drop event listener (Tauri native)
  useEffect(() => {
    if (isCollapsed || !sessionId) return;

    let unlisten: (() => void) | null = null;

    const setup = async () => {
      try {
        const { getCurrentWebview } = await import('@tauri-apps/api/webview');
        unlisten = await getCurrentWebview().onDragDropEvent((event) => {
          const payload = event.payload;

          if (payload.type === 'enter' || payload.type === 'over') {
            const el = fileListRef.current;
            if (el) {
              const rect = el.getBoundingClientRect();
              const dpr = window.devicePixelRatio || 1;
              const x = payload.position.x / dpr;
              const y = payload.position.y / dpr;
              const inside = x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
              setIsDragOver(inside);
            }
          } else if (payload.type === 'drop') {
            const el = fileListRef.current;
            if (el && payload.paths.length > 0) {
              const rect = el.getBoundingClientRect();
              const dpr = window.devicePixelRatio || 1;
              const x = payload.position.x / dpr;
              const y = payload.position.y / dpr;
              if (x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom) {
                handleDropFiles(payload.paths);
              }
            }
            setIsDragOver(false);
          } else {
            // leave
            setIsDragOver(false);
          }
        });
      } catch (e) {
        console.warn('[SftpPanel] Failed to setup drag-drop listener:', e);
      }
    };

    setup();

    return () => {
      unlisten?.();
      setIsDragOver(false);
    };
  }, [isCollapsed, sessionId, handleDropFiles]);

  const handleDownload = useCallback(async () => {
    if (!sessionId || selectedFiles.length === 0) return;

    try {
      const pickResult = await safeInvoke<string | null>('pick_download_directory');

      if (!pickResult.success || !pickResult.data) return;

      const targetDirectory = pickResult.data;
      const failures: string[] = [];
      let completed = 0;

      setIsLoading(true);
      setTransferBatch({
        operation: 'download',
        status: 'running',
        current: 1,
        total: selectedFiles.length,
        currentName: selectedFiles[0].name,
        completed: 0,
        failed: 0,
      });

      for (const [index, entry] of selectedFiles.entries()) {
        setTransferBatch((previous) => previous ? {
          ...previous,
          current: index + 1,
          currentName: entry.name,
          completed,
          failed: failures.length,
        } : previous);

        try {
          const downloadResult = await safeInvoke('sftp_download_file', {
            request: {
              sessionId,
              remotePath: entry.path,
              localPath: joinLocalPath(targetDirectory, entry.name),
            },
          });
          if (downloadResult.success) completed += 1;
          else failures.push(`${entry.name}: ${downloadResult.error.message}`);
        } catch (err) {
          failures.push(`${entry.name}: ${getErrorMessage(err, 'Download failed')}`);
        }
      }

      setTransferBatch({
        operation: 'download',
        status: failures.length > 0 ? 'failed' : 'completed',
        current: selectedFiles.length,
        total: selectedFiles.length,
        currentName: selectedFiles[selectedFiles.length - 1].name,
        completed,
        failed: failures.length,
      });

      if (completed > 0) notifySuccess('Download Complete', `${completed} file(s) downloaded successfully`);
      if (failures.length > 0) notifyError('Download Failed', summarizeFailures(failures));
    } catch (err) {
      console.error('[SftpPanel] Download failed:', err);
      notifyError('Download Failed', err instanceof Error ? err.message : 'Failed to download file');
    } finally {
      setIsLoading(false);
    }
  }, [sessionId, selectedFiles, notifySuccess, notifyError]);

  const handleDelete = useCallback(() => {
    if (!sessionId || selectedEntriesArray.length === 0) return;
    setShowDeleteDialog(true);
  }, [sessionId, selectedEntriesArray.length]);

  const handleConfirmDelete = useCallback(async () => {
    if (!sessionId || selectedEntriesArray.length === 0) {
      setShowDeleteDialog(false);
      return;
    }

    setShowDeleteDialog(false);
    try {
      setIsLoading(true);

      let successCount = 0;
      const failures: string[] = [];

      for (const entry of selectedEntriesArray) {
        const deleteResult = await safeInvoke('sftp_delete', {
          request: {
            sessionId: sessionId,
            path: entry.path,
            recursive: entry.isDirectory,
          },
        });

        if (deleteResult.success) {
          successCount++;
        } else {
          failures.push(`${entry.name}: ${deleteResult.error.message}`);
        }
      }

      if (successCount > 0) {
        notifySuccess('Deleted', `${successCount} item(s) deleted successfully`);
      }
      if (failures.length > 0) {
        notifyError('Delete Failed', summarizeFailures(failures));
      }

      setSelectedEntries(new Set());
      await loadDirectory(currentPath);
    } catch (err) {
      console.error('[SftpPanel] Delete failed:', err);
      notifyError('Delete Failed', err instanceof Error ? err.message : 'Failed to delete');
    } finally {
      setIsLoading(false);
    }
  }, [sessionId, selectedEntriesArray, currentPath, loadDirectory, notifySuccess, notifyError]);

  const handleRename = useCallback(async () => {
    if (!sessionId || !renameEntry || !newName.trim()) return;

    try {
      setIsLoading(true);

      const parentPath = renameEntry.path.substring(0, renameEntry.path.lastIndexOf('/'));
      const newPath = `${parentPath}/${newName.trim()}`;

      const renameResult = await safeInvoke('sftp_rename', {
        request: {
          sessionId: sessionId,
          oldPath: renameEntry.path,
          newPath: newPath,
        },
      });

      if (renameResult.success) {
        notifySuccess('Renamed', `${renameEntry.name} renamed to ${newName}`);
        setRenameEntry(null);
        setNewName('');
        await loadDirectory(currentPath);
      } else {
        throw new Error(renameResult.error.message);
      }
    } catch (err) {
      console.error('[SftpPanel] Rename failed:', err);
      notifyError('Rename Failed', err instanceof Error ? err.message : 'Failed to rename');
    } finally {
      setIsLoading(false);
    }
  }, [sessionId, renameEntry, newName, currentPath, loadDirectory, notifySuccess, notifyError]);

  const handleCreateFolder = useCallback(async () => {
    if (!sessionId || !newFolderName.trim()) return;

    try {
      setIsLoading(true);

      const folderPath = `${currentPath}/${newFolderName.trim()}`;

      const mkdirResult = await safeInvoke('sftp_mkdir', {
        request: {
          sessionId: sessionId,
          path: folderPath,
        },
      });

      if (mkdirResult.success) {
        notifySuccess('Folder Created', `${newFolderName} created successfully`);
        setShowNewFolderDialog(false);
        setNewFolderName('');
        await loadDirectory(currentPath);
      } else {
        throw new Error(mkdirResult.error.message);
      }
    } catch (err) {
      console.error('[SftpPanel] Create folder failed:', err);
      notifyError('Create Failed', err instanceof Error ? err.message : 'Failed to create folder');
    } finally {
      setIsLoading(false);
    }
  }, [sessionId, newFolderName, currentPath, loadDirectory, notifySuccess, notifyError]);

  // Handle multi-selection with Ctrl and Shift modifiers
  const handleEntryClick = useCallback((entry: SftpEntry, index: number, e: React.MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();

    if (e.ctrlKey || e.metaKey) {
      // Ctrl+click: Toggle selection
      setSelectedEntries(prev => {
        const next = new Set(prev);
        if (next.has(entry.path)) {
          next.delete(entry.path);
        } else {
          next.add(entry.path);
        }
        return next;
      });
      setLastSelectedIndex(index);
    } else if (e.shiftKey && lastSelectedIndex !== null) {
      // Shift+click: Range selection
      const start = Math.min(lastSelectedIndex, index);
      const end = Math.max(lastSelectedIndex, index);
      const rangePaths = entries.slice(start, end + 1).map(e => e.path);
      setSelectedEntries(new Set(rangePaths));
    } else {
      // Normal click: Single selection
      setSelectedEntries(new Set([entry.path]));
      setLastSelectedIndex(index);
    }
  }, [entries, lastSelectedIndex]);

  // Handle context menu (right-click)
  const handleContextMenu = useCallback((e: React.MouseEvent, entry?: SftpEntry, index?: number) => {
    e.preventDefault();
    e.stopPropagation();

    // If right-clicking on an entry that's not selected, select it
    if (entry && !selectedEntries.has(entry.path)) {
      setSelectedEntries(new Set([entry.path]));
      if (index !== undefined) {
        setLastSelectedIndex(index);
      }
    }

    setContextMenu({
      visible: true,
      x: e.clientX,
      y: e.clientY,
    });
  }, [selectedEntries]);

  // Select all entries
  const handleSelectAll = useCallback(() => {
    setSelectedEntries(new Set(entries.map(e => e.path)));
    setContextMenu({ visible: false, x: 0, y: 0 });
  }, [entries]);

  // Clear selection
  const handleClearSelection = useCallback(() => {
    setSelectedEntries(new Set());
    setLastSelectedIndex(null);
    setContextMenu({ visible: false, x: 0, y: 0 });
  }, []);

  // Copy selected file paths, or the current directory path when nothing is selected.
  const handleCopyPaths = useCallback(async () => {
    const pathsToCopy = selectedEntriesArray.length > 0
      ? selectedEntriesArray.map(entry => entry.path)
      : [currentPath];

    try {
      await navigator.clipboard.writeText(pathsToCopy.join('\n'));
      notifySuccess(
        pathsToCopy.length === 1 ? 'Path Copied' : 'Paths Copied',
        pathsToCopy.length === 1 ? pathsToCopy[0] : `${pathsToCopy.length} paths copied`
      );
    } catch (err) {
      console.error('[SftpPanel] Failed to copy path:', err);
      notifyError('Copy Failed', err instanceof Error ? err.message : 'Failed to copy path');
    } finally {
      setContextMenu({ visible: false, x: 0, y: 0 });
    }
  }, [selectedEntriesArray, currentPath, notifySuccess, notifyError]);

  // Open compress dialog
  const handleOpenCompressDialog = useCallback(() => {
    if (selectedEntriesArray.length === 0) return;

    // Generate default archive name
    const defaultName = selectedEntriesArray.length === 1
      ? selectedEntriesArray[0].name.replace(/\.[^.]+$/, '')
      : 'archive';

    setArchiveName(defaultName);
    setCompressFormat('tar.gz');
    setShowCompressDialog(true);
    setContextMenu({ visible: false, x: 0, y: 0 });
  }, [selectedEntriesArray]);

  // Compress selected files
  const handleCompress = useCallback(async () => {
    if (!sessionId || selectedEntriesArray.length === 0 || !archiveName.trim()) return;

    try {
      setIsLoading(true);
      setShowCompressDialog(false);

      const archiveFileName = `${archiveName.trim()}.${compressFormat}`;
      const archivePath = `${currentPath}/${archiveFileName}`;
      const filePaths = selectedEntriesArray.map(e => e.path);

      const result = await safeInvoke('sftp_compress', {
        request: {
          sessionId: sessionId,
          paths: filePaths,
          archivePath: archivePath,
          format: compressFormat,
        },
      });

      if (result.success) {
        notifySuccess('Compressed', `Created ${archiveFileName}`);
        await loadDirectory(currentPath);
      } else {
        throw new Error(result.error.message);
      }
    } catch (err) {
      console.error('[SftpPanel] Compress failed:', err);
      notifyError('Compress Failed', err instanceof Error ? err.message : 'Failed to compress files');
    } finally {
      setIsLoading(false);
      setArchiveName('');
    }
  }, [sessionId, selectedEntriesArray, archiveName, compressFormat, currentPath, loadDirectory, notifySuccess, notifyError]);

  // Extract archive
  const handleExtract = useCallback(async () => {
    if (!sessionId) return;

    // Get archive files from selection
    const archiveEntries = selectedEntriesArray.filter(e => !e.isDirectory && isArchiveFile(e.name));
    if (archiveEntries.length === 0) return;

    setContextMenu({ visible: false, x: 0, y: 0 });

    try {
      setIsLoading(true);

      let successCount = 0;
      let failCount = 0;

      for (const entry of archiveEntries) {
        const result = await safeInvoke('sftp_extract', {
          request: {
            sessionId: sessionId,
            archivePath: entry.path,
            destinationPath: currentPath,
          },
        });

        if (result.success) {
          successCount++;
        } else {
          failCount++;
        }
      }

      if (successCount > 0) {
        notifySuccess('Extracted', `${successCount} archive(s) extracted successfully`);
      }
      if (failCount > 0) {
        notifyError('Extract Failed', `Failed to extract ${failCount} archive(s)`);
      }

      await loadDirectory(currentPath);
    } catch (err) {
      console.error('[SftpPanel] Extract failed:', err);
      notifyError('Extract Failed', err instanceof Error ? err.message : 'Failed to extract archive');
    } finally {
      setIsLoading(false);
    }
  }, [sessionId, selectedEntriesArray, currentPath, loadDirectory, notifySuccess, notifyError]);

  // Open files in the shared workspace tab strip, deduplicated by session/path.
  const handleOpenFile = useCallback((entry?: SftpEntry) => {
    const targetEntry = entry || selectedEntry;
    if (!sessionId || !targetEntry || targetEntry.isDirectory) return;
    openFile({
      sessionId,
      path: targetEntry.path,
      name: targetEntry.name,
      size: targetEntry.size,
    });
    setContextMenu({ visible: false, x: 0, y: 0 });
  }, [openFile, selectedEntry, sessionId]);

  const handleTouchEntryOpen = useCallback((entry: SftpEntry) => {
    if (!usesCoarsePointer()) return;
    if (entry.isDirectory) {
      navigateToEntry(entry);
    } else {
      handleOpenFile(entry);
    }
  }, [handleOpenFile, navigateToEntry]);

  // Start resize drag
  const handleResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragStartPoint.current = dock === 'right' ? e.clientX : e.clientY;
    dragStartSize.current = dock === 'right' ? panelWidth : panelHeight;
    setIsDragging(true);
  }, [dock, panelHeight, panelWidth]);

  const toggleCollapse = () => {
    if (isCollapsed) {
      expandPanel();
    } else {
      collapsePanel();
    }
  };

  const toggleFullscreen = () => {
    togglePanelFullscreen();
  };

  const changeViewMode = (nextMode: SftpViewMode) => {
    if (nextMode === 'columns') {
      setColumns([{ path: currentPath, entries }]);
      setColumnSelections({});
      if (dock === 'right') {
        setPanelWidth((current) => Math.max(current, Math.min(maxWidth, 680)));
      }
    }
    setViewMode(nextMode);
    try {
      globalThis.localStorage?.setItem('vibeshell-sftp-view', nextMode);
    } catch {
      // The selected mode still applies for the current session.
    }
  };

  if (!sessionId) {
    return null;
  }

  // Parse path for breadcrumb navigation
  const pathParts = currentPath.split('/').filter(Boolean);
  const isCompactToolbar = dock === 'right' && !isFullscreen && panelWidth < 620;
  const transferProcessed = transferBatch ? transferBatch.completed + transferBatch.failed : 0;
  const transferPercent = transferBatch
    ? Math.round((transferProcessed / Math.max(transferBatch.total, 1)) * 100)
    : 0;

  const getPanelStyle = (): React.CSSProperties => {
    if (isFullscreen) return { width: '100%', height: '100%' };
    if (dock === 'right') {
      return {
        width: isCollapsed ? '0px' : `${panelWidth}px`,
        height: '100%',
        flexShrink: 0,
      };
    }
    return { width: '100%', height: isCollapsed ? '40px' : `${panelHeight}px` };
  };

  return (
    <div
      ref={panelRef}
      className={cn(
        'sftp-panel-shell relative bg-tokyo-bg-dark transition-[width,height] duration-200',
        dock === 'right' ? 'border-l border-tokyo-bg-hl' : 'border-t border-tokyo-bg-hl',
        dock === 'right' && isCollapsed && !isFullscreen && 'pointer-events-none overflow-hidden border-l-0',
        isFullscreen && 'absolute inset-0 z-40 border-l-0 border-t-0'
      )}
      style={getPanelStyle()}
      aria-hidden={dock === 'right' && isCollapsed && !isFullscreen}
      data-testid="sftp-panel"
    >
      {/* Resize Handle (when not collapsed and not fullscreen) */}
      {!isCollapsed && !isFullscreen && (
        <div
          className={cn(
            'sftp-resize-handle absolute z-10 group flex items-center justify-center',
            dock === 'right'
              ? 'bottom-0 left-0 top-0 w-2 cursor-ew-resize'
              : 'left-0 right-0 top-0 h-2 cursor-ns-resize',
            'hover:bg-tokyo-blue/30 transition-colors',
            isDragging && 'bg-tokyo-blue/50'
          )}
          onMouseDown={handleResizeStart}
          title="Drag to resize"
        >
          <div className={cn(
            'flex items-center justify-center bg-tokyo-bg-hl/80 border border-tokyo-bg-hl',
            dock === 'right'
              ? '-ml-2 h-12 w-4 rounded-r border-l-0'
              : '-mt-2 h-4 w-12 rounded-t border-b-0',
            'opacity-60 group-hover:opacity-100 transition-opacity',
            isDragging && 'opacity-100 bg-tokyo-blue/30'
          )}>
            {dock === 'right'
              ? <GripVertical className="w-4 h-4 text-tokyo-comment" />
              : <GripHorizontal className="w-4 h-4 text-tokyo-comment" />}
          </div>
        </div>
      )}

      {/* Header */}
      <div
        className={cn(
          'flex items-center justify-between px-3 h-10',
          'border-b border-tokyo-bg-hl'
        )}
      >
        <div className="flex items-center gap-2">
          <button
            className="p-1 hover:bg-tokyo-bg-hl rounded transition-colors"
            onClick={(e) => {
              e.stopPropagation();
              toggleCollapse();
            }}
            aria-label={isCollapsed ? 'Expand SFTP panel' : 'Collapse SFTP panel'}
          >
            {isCollapsed ? (
              <ChevronUp className="w-4 h-4 text-tokyo-comment" />
            ) : (
              <ChevronDown className="w-4 h-4 text-tokyo-comment" />
            )}
          </button>
          <Folder className="w-4 h-4 text-tokyo-blue" />
          <span className="text-sm font-medium text-tokyo-fg">
            {sessionType === 'local' ? 'Local Files' : 'SFTP'}
          </span>
          {selectedEntries.size > 1 && (
            <span className="text-xs text-tokyo-comment bg-tokyo-bg-hl px-2 py-0.5 rounded">
              {selectedEntries.size} selected
            </span>
          )}
          {isLoading && (
            <Loader2 className="w-4 h-4 text-tokyo-blue animate-spin ml-2" />
          )}
        </div>

        <div className="flex items-center gap-1">
          {!isCollapsed && (
            <div className="mr-1 flex items-center rounded-md border border-tokyo-bg-hl bg-tokyo-bg p-0.5">
              <button
                className={cn('icon-button h-7 w-7', viewMode === 'details' && 'bg-tokyo-selection text-tokyo-fg')}
                onClick={(e) => { e.stopPropagation(); changeViewMode('details'); }}
                aria-label="Details view"
                title="Details view"
              >
                <List className="h-4 w-4" />
              </button>
              <button
                className={cn('icon-button h-7 w-7', viewMode === 'columns' && 'bg-tokyo-selection text-tokyo-fg')}
                onClick={(e) => { e.stopPropagation(); changeViewMode('columns'); }}
                aria-label="Column view"
                title="Column view"
              >
                <Columns3 className="h-4 w-4" />
              </button>
              <button
                className={cn('icon-button h-7 w-7', viewMode === 'icons' && 'bg-tokyo-selection text-tokyo-fg')}
                onClick={(e) => { e.stopPropagation(); changeViewMode('icons'); }}
                aria-label="Icon view"
                title="Icon view"
              >
                <Grid2X2 className="h-4 w-4" />
              </button>
            </div>
          )}
          {/* Fullscreen toggle */}
          <button
            className={cn(
              'p-1.5 rounded hover:bg-tokyo-bg-hl',
              'transition-colors duration-150'
            )}
            onClick={(e) => {
              e.stopPropagation();
              toggleFullscreen();
            }}
            aria-label={isFullscreen ? 'Exit fullscreen' : 'Enter fullscreen'}
          >
            {isFullscreen ? (
              <Minimize2 className="w-4 h-4 text-tokyo-comment hover:text-tokyo-fg" />
            ) : (
              <Maximize2 className="w-4 h-4 text-tokyo-comment hover:text-tokyo-fg" />
            )}
          </button>
        </div>
      </div>

      {/* Content (when expanded) */}
      {!isCollapsed && (
        <div className="relative flex h-[calc(100%-40px)] min-h-0 flex-col">
          {/* Toolbar */}
          <div className="flex items-center gap-1 px-2 py-1 border-b border-tokyo-bg-hl bg-tokyo-bg" data-testid="sftp-toolbar">
            <button
              onClick={(e) => { e.stopPropagation(); handleGoHome(); }}
              disabled={isLoading}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="Home"
            >
              <Home className="w-4 h-4 text-tokyo-comment" />
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); navigateUp(); }}
              disabled={isLoading || currentPath === '/' || currentPath === '~'}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="Go up"
            >
              <ChevronUp className="w-4 h-4 text-tokyo-comment" />
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); handleRefresh(); }}
              disabled={isLoading}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="Refresh"
            >
              <RefreshCw className={cn('w-4 h-4 text-tokyo-comment', isLoading && 'animate-spin')} />
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); handleCopyPaths(); }}
              disabled={isLoading}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title={selectedEntries.size > 0 ? 'Copy selected path(s)' : 'Copy current path'}
            >
              <Copy className="w-4 h-4 text-tokyo-comment" />
            </button>

            <div className="w-px h-4 bg-tokyo-bg-hl mx-1" />

            <button
              onClick={(e) => { e.stopPropagation(); setShowNewFolderDialog(true); }}
              disabled={isLoading}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="New folder"
            >
              <FolderPlus className="w-4 h-4 text-tokyo-comment" />
            </button>
            {pathTransferEnabled && (
              <button
                onClick={(e) => { e.stopPropagation(); handleUpload(); }}
                disabled={isLoading}
                className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
                title="Upload file"
              >
                <Upload className="w-4 h-4 text-tokyo-comment" />
              </button>
            )}
            {!isCompactToolbar && (
              <>
            {pathTransferEnabled && (
              <>
            <button
              onClick={(e) => { e.stopPropagation(); handleUploadDirectory('upload'); }}
              disabled={isLoading}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="Upload folder"
            >
              <FolderOpen className="w-4 h-4 text-tokyo-comment" />
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); handleUploadDirectory('sync'); }}
              disabled={isLoading}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="Sync folder into current path"
            >
              <RefreshCw className="w-4 h-4 text-tokyo-comment" />
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); handleDownload(); }}
              disabled={isLoading || selectedFiles.length === 0}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title={selectedFiles.length > 1 ? `Download ${selectedFiles.length} files` : 'Download'}
            >
              <Download className="w-4 h-4 text-tokyo-comment" />
            </button>
              </>
            )}
            <button
              onClick={(e) => { e.stopPropagation(); handleOpenFile(); }}
              disabled={isLoading || !canOpenSelected}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="Open in Tab"
            >
              <Eye className="w-4 h-4 text-tokyo-comment" />
            </button>

            <div className="w-px h-4 bg-tokyo-bg-hl mx-1" />

            <button
              onClick={(e) => {
                e.stopPropagation();
                if (selectedEntry) {
                  setRenameEntry(selectedEntry);
                  setNewName(selectedEntry.name);
                }
              }}
              disabled={isLoading || !selectedEntry}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="Rename"
            >
              <Edit3 className="w-4 h-4 text-tokyo-comment" />
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); handleDelete(); }}
              disabled={isLoading || selectedEntries.size === 0}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="Delete"
            >
              <Trash2 className="w-4 h-4 text-tokyo-red" />
            </button>

            <div className="w-px h-4 bg-tokyo-bg-hl mx-1" />

            {/* Compress button */}
            <button
              onClick={(e) => { e.stopPropagation(); handleOpenCompressDialog(); }}
              disabled={isLoading || selectedEntries.size === 0}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="Compress selected"
            >
              <Archive className="w-4 h-4 text-tokyo-comment" />
            </button>

            {/* Extract button */}
            <button
              onClick={(e) => { e.stopPropagation(); handleExtract(); }}
              disabled={isLoading || !hasSelectedArchives}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="Extract archive"
            >
              <FolderOpen className="w-4 h-4 text-tokyo-comment" />
            </button>

            {/* Select all button */}
            <button
              onClick={(e) => { e.stopPropagation(); handleSelectAll(); }}
              disabled={isLoading || entries.length === 0}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="Select all"
            >
              <CheckSquare className="w-4 h-4 text-tokyo-comment" />
            </button>
              </>
            )}

            {isCompactToolbar && (
              <div className="relative ml-auto">
                <button
                  className={cn(
                    'p-1.5 rounded transition-colors hover:bg-tokyo-bg-hl',
                    isToolbarMenuOpen && 'bg-tokyo-selection text-tokyo-fg'
                  )}
                  onClick={(event) => {
                    event.stopPropagation();
                    setIsToolbarMenuOpen((open) => !open);
                  }}
                  aria-label="More SFTP actions"
                  title="More actions"
                >
                  <MoreHorizontal className="h-4 w-4 text-tokyo-comment" />
                </button>
                {isToolbarMenuOpen && (
                  <div
                    className="absolute right-0 top-full z-50 mt-1 max-h-64 w-52 overflow-y-auto rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark py-1 shadow-xl"
                    onClick={(event) => event.stopPropagation()}
                  >
                    {pathTransferEnabled && (
                      <>
                    <button
                      className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-tokyo-fg hover:bg-tokyo-bg-hl disabled:opacity-40"
                      onClick={() => { setIsToolbarMenuOpen(false); void handleUploadDirectory('upload'); }}
                      disabled={isLoading}
                    >
                      <FolderOpen className="h-4 w-4 text-tokyo-comment" /> Upload folder
                    </button>
                    <button
                      className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-tokyo-fg hover:bg-tokyo-bg-hl disabled:opacity-40"
                      onClick={() => { setIsToolbarMenuOpen(false); void handleUploadDirectory('sync'); }}
                      disabled={isLoading}
                    >
                      <RefreshCw className="h-4 w-4 text-tokyo-comment" /> Sync current folder
                    </button>
                    <button
                      className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-tokyo-fg hover:bg-tokyo-bg-hl disabled:opacity-40"
                      onClick={() => { setIsToolbarMenuOpen(false); void handleDownload(); }}
                      disabled={isLoading || selectedFiles.length === 0}
                    >
                      <Download className="h-4 w-4 text-tokyo-comment" />
                      {selectedFiles.length > 1 ? `Download ${selectedFiles.length} files` : 'Download'}
                    </button>
                      </>
                    )}
                    <button
                      className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-tokyo-fg hover:bg-tokyo-bg-hl disabled:opacity-40"
                      onClick={() => { setIsToolbarMenuOpen(false); handleOpenFile(); }}
                      disabled={isLoading || !canOpenSelected}
                    >
                      <Eye className="h-4 w-4 text-tokyo-comment" /> Open in Tab
                    </button>
                    <div className="my-1 border-t border-tokyo-bg-hl" />
                    <button
                      className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-tokyo-fg hover:bg-tokyo-bg-hl disabled:opacity-40"
                      onClick={() => {
                        setIsToolbarMenuOpen(false);
                        if (selectedEntry) {
                          setRenameEntry(selectedEntry);
                          setNewName(selectedEntry.name);
                        }
                      }}
                      disabled={isLoading || !selectedEntry}
                    >
                      <Edit3 className="h-4 w-4 text-tokyo-comment" /> Rename
                    </button>
                    <button
                      className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-tokyo-red hover:bg-tokyo-bg-hl disabled:opacity-40"
                      onClick={() => { setIsToolbarMenuOpen(false); void handleDelete(); }}
                      disabled={isLoading || selectedEntries.size === 0}
                    >
                      <Trash2 className="h-4 w-4" /> Delete
                    </button>
                    <div className="my-1 border-t border-tokyo-bg-hl" />
                    <button
                      className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-tokyo-fg hover:bg-tokyo-bg-hl disabled:opacity-40"
                      onClick={() => { setIsToolbarMenuOpen(false); handleOpenCompressDialog(); }}
                      disabled={isLoading || selectedEntries.size === 0}
                    >
                      <Archive className="h-4 w-4 text-tokyo-comment" /> Compress
                    </button>
                    <button
                      className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-tokyo-fg hover:bg-tokyo-bg-hl disabled:opacity-40"
                      onClick={() => { setIsToolbarMenuOpen(false); void handleExtract(); }}
                      disabled={isLoading || !hasSelectedArchives}
                    >
                      <FolderOpen className="h-4 w-4 text-tokyo-comment" /> Extract
                    </button>
                    <button
                      className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-tokyo-fg hover:bg-tokyo-bg-hl disabled:opacity-40"
                      onClick={() => { setIsToolbarMenuOpen(false); handleSelectAll(); }}
                      disabled={isLoading || entries.length === 0}
                    >
                      <CheckSquare className="h-4 w-4 text-tokyo-comment" /> Select all
                    </button>
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Address bar */}
          <div className="flex h-8 flex-shrink-0 items-center gap-1 overflow-hidden border-b border-tokyo-bg-hl bg-tokyo-bg-dark px-3 text-xs text-tokyo-comment" data-testid="sftp-address-bar">
              <span
                className="cursor-pointer text-tokyo-fg hover:text-tokyo-blue hover:underline"
                onClick={(e) => { e.stopPropagation(); loadDirectory('/'); }}
              >/</span>
              {pathParts.map((part, index) => {
                const isLast = index === pathParts.length - 1;
                const segmentPath = '/' + pathParts.slice(0, index + 1).join('/');
                return (
                  <span key={index} className="flex items-center">
                    {index > 0 && <ChevronRight className="w-3 h-3" />}
                    <span
                      className={cn(
                        'truncate',
                        isLast
                          ? 'text-tokyo-fg'
                          : 'cursor-pointer hover:text-tokyo-blue hover:underline'
                      )}
                      onClick={isLast ? undefined : (e) => { e.stopPropagation(); loadDirectory(segmentPath); }}
                    >
                      {part}
                    </span>
                  </span>
                );
              })}
          </div>

          {transferBatch && (
            <div
              data-testid="sftp-transfer-progress"
              className="flex-shrink-0 border-b border-tokyo-bg-hl bg-tokyo-bg px-3 py-2"
              aria-live="polite"
            >
              <div className="flex min-w-0 items-center gap-2 text-xs">
                {transferBatch.status === 'running' ? (
                  <Loader2 className="h-3.5 w-3.5 flex-shrink-0 animate-spin text-tokyo-blue" />
                ) : transferBatch.status === 'completed' ? (
                  <Check className="h-3.5 w-3.5 flex-shrink-0 text-tokyo-green" />
                ) : (
                  <AlertCircle className="h-3.5 w-3.5 flex-shrink-0 text-tokyo-red" />
                )}
                <span className="flex-shrink-0 font-medium text-tokyo-fg">
                  {transferBatch.status === 'running'
                    ? `${transferBatch.operation === 'upload' ? 'Uploading' : 'Downloading'} ${transferBatch.current} of ${transferBatch.total}`
                    : `${transferBatch.operation === 'upload' ? 'Upload' : 'Download'} ${transferBatch.status}`}
                </span>
                <span className="min-w-0 flex-1 truncate text-tokyo-comment" title={transferBatch.currentName}>
                  {transferBatch.status === 'running'
                    ? transferBatch.currentName
                    : `${transferBatch.completed} succeeded${transferBatch.failed > 0 ? `, ${transferBatch.failed} failed` : ''}`}
                </span>
                {transferBatch.status !== 'running' && (
                  <button
                    onClick={() => setTransferBatch(null)}
                    className="icon-button h-6 w-6 flex-shrink-0"
                    title="Dismiss transfer status"
                    aria-label="Dismiss transfer status"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                )}
              </div>
              <div
                className="mt-1.5 h-1.5 overflow-hidden rounded-sm bg-tokyo-bg-hl"
                role="progressbar"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={transferPercent}
              >
                <div
                  className={cn(
                    'h-full transition-[width] duration-200',
                    transferBatch.status === 'failed' ? 'bg-tokyo-red' : 'bg-tokyo-blue'
                  )}
                  style={{ width: `${transferPercent}%` }}
                />
              </div>
            </div>
          )}

          {/* Error display */}
          {error && (
            <div className="flex items-center gap-2 px-3 py-2 bg-tokyo-red/10 border-b border-tokyo-red/30 text-tokyo-red text-sm">
              <AlertCircle className="w-4 h-4 flex-shrink-0" />
              <span className="truncate">{error}</span>
              <button
                onClick={() => {
                  setError(null);
                  handleRefresh();
                }}
                className="ml-auto text-tokyo-red hover:text-tokyo-fg"
              >
                Retry
              </button>
              <button
                onClick={() => setError(null)}
                className="text-tokyo-red hover:text-tokyo-fg"
              >
                Dismiss
              </button>
            </div>
          )}

          {/* File list */}
          <div
            ref={fileListRef}
            className={cn(
              'relative min-h-0 flex-1 overflow-auto',
              viewMode !== 'columns' && 'pb-6'
            )}
            onClick={(e) => {
              e.stopPropagation();
              // Clear selection when clicking empty space
              handleClearSelection();
            }}
            onContextMenu={(e) => handleContextMenu(e)}
            onMouseDown={(e) => e.stopPropagation()}
            onMouseUp={(e) => e.stopPropagation()}
            onDoubleClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => e.stopPropagation()}
          >
            {/* Drag-and-drop overlay */}
            {isDragOver && (
              <div className="absolute inset-0 z-40 flex items-center justify-center bg-tokyo-blue/10 border-2 border-dashed border-tokyo-blue rounded-md pointer-events-none">
                <div className="flex flex-col items-center gap-2 text-tokyo-blue">
                  <Upload className="w-8 h-8" />
                  <span className="text-sm font-medium">Drop files here to upload</span>
                </div>
              </div>
            )}
            {viewMode === 'columns' ? (
              <div
                ref={columnsScrollerRef}
                className="flex h-full min-h-0 overflow-x-auto overflow-y-hidden bg-tokyo-bg"
                data-testid="sftp-column-browser"
              >
                {columns.map((column, columnIndex) => (
                  <section
                    key={column.path}
                    className="flex h-full min-w-[220px] max-w-[280px] flex-1 flex-col border-r border-tokyo-bg-hl last:border-r-0"
                    data-sftp-column={column.path}
                  >
                    <div className="flex h-8 flex-shrink-0 items-center border-b border-tokyo-bg-hl bg-tokyo-bg-dark px-2">
                      <span className="truncate font-mono text-[10px] text-tokyo-comment" title={column.path}>
                        {column.path === '/' ? '/' : basename(column.path)}
                      </span>
                    </div>
                    <div className="min-h-0 flex-1 overflow-y-auto pb-6 pt-1">
                      {column.entries.length === 0 ? (
                        <div className="px-3 py-4 text-xs text-tokyo-comment">Empty directory</div>
                      ) : column.entries.map((entry) => (
                        <button
                          key={entry.path}
                          title={entry.name}
                          className={cn(
                            'group flex h-8 w-full min-w-0 items-center gap-2 px-2 text-left transition-colors hover:bg-tokyo-bg-hl',
                            'focus:outline-none focus:ring-1 focus:ring-inset focus:ring-tokyo-blue',
                            columnSelections[column.path] === entry.path && 'bg-tokyo-selection text-tokyo-fg'
                          )}
                          onClick={(event) => {
                            event.stopPropagation();
                            void handleColumnEntryClick(column, columnIndex, entry);
                          }}
                          onDoubleClick={(event) => {
                            event.stopPropagation();
                            if (!entry.isDirectory) handleOpenFile(entry);
                          }}
                          onContextMenu={(event) => {
                            event.preventDefault();
                            event.stopPropagation();
                            setCurrentPath(column.path);
                            setEntries(column.entries);
                            setSelectedEntries(new Set([entry.path]));
                            setColumnSelections((previous) => ({ ...previous, [column.path]: entry.path }));
                            setContextMenu({ visible: true, x: event.clientX, y: event.clientY });
                          }}
                        >
                          <FileIcon filename={entry.name} isDirectory={entry.isDirectory} />
                          <span className="min-w-0 flex-1 truncate text-xs text-tokyo-fg">{entry.name}</span>
                          {entry.isDirectory ? (
                            <ChevronRight className="h-3.5 w-3.5 flex-shrink-0 text-tokyo-comment" />
                          ) : (
                            <span className="flex-shrink-0 text-[9px] text-tokyo-comment">{formatFileSize(entry.size)}</span>
                          )}
                        </button>
                      ))}
                    </div>
                  </section>
                ))}
              </div>
            ) : entries.length === 0 && !isLoading && !error ? (
              <div className="flex items-center justify-center h-full text-tokyo-comment text-sm">
                Empty directory
              </div>
            ) : viewMode === 'details' ? (
              <table className="w-full text-sm">
                <thead className="sticky top-0 bg-tokyo-bg border-b border-tokyo-bg-hl">
                  <tr className="text-left text-tokyo-comment">
                    <th className="px-2 py-1 font-medium w-8">
                      <div
                        className="flex items-center justify-center cursor-pointer"
                        onClick={(e) => {
                          e.stopPropagation();
                          if (selectedEntries.size > 0) {
                            handleClearSelection();
                          } else {
                            handleSelectAll();
                          }
                        }}
                      >
                        <div
                          className={cn(
                            'w-4 h-4 rounded border flex items-center justify-center hover:border-tokyo-blue',
                            selectedEntries.size === 0
                              ? 'border-tokyo-comment/50'
                              : 'bg-tokyo-blue border-tokyo-blue'
                          )}
                        >
                          {selectedEntries.size > 0 && selectedEntries.size === entries.length && (
                            <Check className="w-3 h-3 text-tokyo-on-accent" />
                          )}
                          {selectedEntries.size > 0 && selectedEntries.size < entries.length && (
                            <Minus className="w-3 h-3 text-tokyo-on-accent" />
                          )}
                        </div>
                      </div>
                    </th>
                    <th className="px-2 py-1 font-medium">Name</th>
                    <th className="px-2 py-1 font-medium w-20">Size</th>
                    <th className="px-2 py-1 font-medium w-36">Modified</th>
                  </tr>
                </thead>
                <tbody>
                  {entries.map((entry, index) => (
                    <tr
                      key={entry.path}
                      onClick={(e) => {
                        handleEntryClick(entry, index, e);
                        handleTouchEntryOpen(entry);
                      }}
                      onContextMenu={(e) => handleContextMenu(e, entry, index)}
                      onDoubleClick={(e) => {
                        e.stopPropagation();
                        e.preventDefault();
                        if (entry.isDirectory) {
                          navigateToEntry(entry);
                        } else {
                          handleOpenFile(entry);
                        }
                      }}
                      onMouseDown={(e) => e.stopPropagation()}
                      onMouseUp={(e) => e.stopPropagation()}
                      className={cn(
                        'cursor-pointer hover:bg-tokyo-bg-hl/50 transition-colors',
                        selectedEntries.has(entry.path) && 'bg-tokyo-blue/20'
                      )}
                    >
                      <td className="px-2 py-1 w-8">
                        <div
                          className="flex items-center justify-center"
                          onClick={(e) => {
                            e.stopPropagation();
                            setSelectedEntries(prev => {
                              const next = new Set(prev);
                              if (next.has(entry.path)) {
                                next.delete(entry.path);
                              } else {
                                next.add(entry.path);
                              }
                              return next;
                            });
                            setLastSelectedIndex(index);
                          }}
                        >
                          <div
                            className={cn(
                              'w-4 h-4 rounded border flex items-center justify-center hover:border-tokyo-blue',
                              selectedEntries.has(entry.path)
                                ? 'bg-tokyo-blue border-tokyo-blue'
                                : 'border-tokyo-comment/50'
                            )}
                          >
                            {selectedEntries.has(entry.path) && (
                              <Check className="w-3 h-3 text-tokyo-on-accent" />
                            )}
                          </div>
                        </div>
                      </td>
                      <td className="px-2 py-1">
                        <div className="flex items-center gap-2">
                          <FileIcon filename={entry.name} isDirectory={entry.isDirectory} />
                          <span className="truncate text-tokyo-fg">{entry.name}</span>
                        </div>
                      </td>
                      <td className="px-2 py-1 text-tokyo-comment">
                        {entry.isDirectory ? '-' : formatFileSize(entry.size)}
                      </td>
                      <td className="px-2 py-1 text-tokyo-comment text-xs">
                        {formatDate(entry.modifiedAt)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : (
              <div
                className="grid grid-flow-dense grid-cols-[repeat(auto-fill,minmax(92px,1fr))] gap-2 p-2"
              >
                {entries.map((entry, index) => (
                  <button
                    key={entry.path}
                    title={entry.name}
                    onClick={(e) => {
                      handleEntryClick(entry, index, e);
                      handleTouchEntryOpen(entry);
                    }}
                    onContextMenu={(e) => handleContextMenu(e, entry, index)}
                    onDoubleClick={(e) => {
                      e.stopPropagation();
                      e.preventDefault();
                      if (entry.isDirectory) {
                        navigateToEntry(entry);
                      } else {
                        handleOpenFile(entry);
                      }
                    }}
                    className={cn(
                      'group relative min-w-0 overflow-hidden bg-tokyo-bg text-left transition-colors hover:bg-tokyo-bg-hl',
                      'focus:outline-none focus:ring-1 focus:ring-inset focus:ring-tokyo-blue',
                      'flex aspect-square max-h-28 flex-col items-center justify-center gap-2 rounded-md border border-tokyo-bg-hl p-2 text-center',
                      selectedEntries.has(entry.path) && 'bg-tokyo-selection'
                    )}
                  >
                    <FileIcon
                      filename={entry.name}
                      isDirectory={entry.isDirectory}
                      size="lg"
                      className="transition-transform duration-700 ease-out group-hover:scale-105"
                    />
                    <span className="w-full min-w-0 truncate text-xs text-tokyo-fg">
                      {entry.name}
                    </span>
                    {selectedEntries.has(entry.path) && (
                      <span className="absolute right-1.5 top-1.5 flex h-4 w-4 items-center justify-center rounded-sm bg-tokyo-blue text-tokyo-on-accent">
                        <Check className="h-3 w-3" />
                      </span>
                    )}
                  </button>
                ))}
              </div>
            )}
          </div>

          {/* Rename Dialog */}
          {renameEntry && (
            <div className="absolute inset-0 flex items-center justify-center bg-black/50 z-50">
              <div className="bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg p-4 w-80">
                <h3 className="text-sm font-medium text-tokyo-fg mb-3">
                  Rename {renameEntry.isDirectory ? 'folder' : 'file'}
                </h3>
                <input
                  type="text"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') handleRename();
                    if (e.key === 'Escape') setRenameEntry(null);
                  }}
                  autoFocus
                  className={cn(
                    'w-full px-3 py-2 rounded-md',
                    'bg-tokyo-bg border border-tokyo-bg-hl',
                    'text-tokyo-fg placeholder-tokyo-comment',
                    'focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
                  )}
                />
                <div className="flex justify-end gap-2 mt-3">
                  <button
                    onClick={() => setRenameEntry(null)}
                    className="px-3 py-1.5 rounded text-sm text-tokyo-fg hover:bg-tokyo-bg-hl"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={handleRename}
                    disabled={!newName.trim() || newName === renameEntry.name}
                    className="px-3 py-1.5 rounded text-sm bg-tokyo-blue text-tokyo-on-accent hover:bg-tokyo-blue/80 disabled:opacity-50"
                  >
                    Rename
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* New Folder Dialog */}
          {showNewFolderDialog && (
            <div className="absolute inset-0 flex items-center justify-center bg-black/50 z-50">
              <div className="bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg p-4 w-80">
                <h3 className="text-sm font-medium text-tokyo-fg mb-3">New Folder</h3>
                <input
                  type="text"
                  value={newFolderName}
                  onChange={(e) => setNewFolderName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') handleCreateFolder();
                    if (e.key === 'Escape') setShowNewFolderDialog(false);
                  }}
                  placeholder="Folder name"
                  autoFocus
                  className={cn(
                    'w-full px-3 py-2 rounded-md',
                    'bg-tokyo-bg border border-tokyo-bg-hl',
                    'text-tokyo-fg placeholder-tokyo-comment',
                    'focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
                  )}
                />
                <div className="flex justify-end gap-2 mt-3">
                  <button
                    onClick={() => setShowNewFolderDialog(false)}
                    className="px-3 py-1.5 rounded text-sm text-tokyo-fg hover:bg-tokyo-bg-hl"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={handleCreateFolder}
                    disabled={!newFolderName.trim()}
                    className="px-3 py-1.5 rounded text-sm bg-tokyo-blue text-tokyo-on-accent hover:bg-tokyo-blue/80 disabled:opacity-50"
                  >
                    Create
                  </button>
                </div>
              </div>
            </div>
          )}

          <ConfirmDialog
            isOpen={showDeleteDialog}
            title={selectedEntriesArray.length === 1 ? 'Delete item?' : `Delete ${selectedEntriesArray.length} items?`}
            message={selectedEntriesArray.length === 1
              ? `Are you sure you want to permanently delete ${selectedEntriesArray[0]?.name ?? 'this item'}?`
              : `Are you sure you want to permanently delete these ${selectedEntriesArray.length} items?`}
            confirmLabel="Delete"
            variant="danger"
            onConfirm={() => { void handleConfirmDelete(); }}
            onCancel={() => setShowDeleteDialog(false)}
          />

          {/* Compress Dialog */}
          {showCompressDialog && (
            <div className="absolute inset-0 flex items-center justify-center bg-black/50 z-50">
              <div className="bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg p-4 w-80">
                <h3 className="text-sm font-medium text-tokyo-fg mb-3">
                  Compress {selectedEntriesArray.length} item(s)
                </h3>
                <div className="space-y-3">
                  <div>
                    <label className="block text-xs text-tokyo-comment mb-1">Archive Name</label>
                    <input
                      type="text"
                      value={archiveName}
                      onChange={(e) => setArchiveName(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') handleCompress();
                        if (e.key === 'Escape') setShowCompressDialog(false);
                      }}
                      placeholder="archive"
                      autoFocus
                      className={cn(
                        'w-full px-3 py-2 rounded-md',
                        'bg-tokyo-bg border border-tokyo-bg-hl',
                        'text-tokyo-fg placeholder-tokyo-comment',
                        'focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
                      )}
                    />
                  </div>
                  <div>
                    <label className="block text-xs text-tokyo-comment mb-1">Format</label>
                    <select
                      value={compressFormat}
                      onChange={(e) => setCompressFormat(e.target.value as ArchiveFormat)}
                      className={cn(
                        'w-full px-3 py-2 rounded-md',
                        'bg-tokyo-bg border border-tokyo-bg-hl',
                        'text-tokyo-fg',
                        'focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
                      )}
                    >
                      <option value="tar.gz">tar.gz (gzip)</option>
                      <option value="zip">zip</option>
                    </select>
                  </div>
                  <div className="text-xs text-tokyo-comment">
                    Output: {archiveName || 'archive'}.{compressFormat}
                  </div>
                </div>
                <div className="flex justify-end gap-2 mt-4">
                  <button
                    onClick={() => setShowCompressDialog(false)}
                    className="px-3 py-1.5 rounded text-sm text-tokyo-fg hover:bg-tokyo-bg-hl"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={handleCompress}
                    disabled={!archiveName.trim()}
                    className="px-3 py-1.5 rounded text-sm bg-tokyo-blue text-tokyo-on-accent hover:bg-tokyo-blue/80 disabled:opacity-50"
                  >
                    Compress
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* Context Menu */}
          {contextMenu.visible && createPortal(
            <div
              ref={contextMenuRef}
              role="menu"
              data-testid="sftp-context-menu"
              className={cn(
                'fixed bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg shadow-lg py-1 z-[60]',
                'min-w-[160px] overflow-y-auto overscroll-contain'
              )}
              style={{
                left: contextMenu.x,
                top: contextMenu.y,
                maxHeight: 'calc(100vh - 16px)',
              }}
              onClick={(e) => e.stopPropagation()}
            >
              {selectedEntries.size > 0 && (
                <>
                  {/* Copy path(s) */}
                  <button
                    onClick={handleCopyPaths}
                    className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
                  >
                    <Copy className="w-4 h-4" />
                    {selectedEntries.size === 1 ? 'Copy Path' : `Copy ${selectedEntries.size} Paths`}
                  </button>

                  {/* Open a single file in the workspace tab strip. */}
                  {selectedEntries.size === 1 && selectedEntry && !selectedEntry.isDirectory && (
                    <button
                      onClick={() => handleOpenFile()}
                      className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
                    >
                      <Eye className="w-4 h-4" />
                      Open in Tab
                    </button>
                  )}

                  {/* Download selected files */}
                  {pathTransferEnabled && selectedFiles.length > 0 && (
                    <button
                      onClick={() => {
                        setContextMenu({ visible: false, x: 0, y: 0 });
                        handleDownload();
                      }}
                      className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
                    >
                      <Download className="w-4 h-4" />
                      {selectedFiles.length > 1 ? `Download ${selectedFiles.length} files` : 'Download'}
                    </button>
                  )}

                  {/* Rename (single selection only) */}
                  {selectedEntries.size === 1 && selectedEntry && (
                    <button
                      onClick={() => {
                        setContextMenu({ visible: false, x: 0, y: 0 });
                        setRenameEntry(selectedEntry);
                        setNewName(selectedEntry.name);
                      }}
                      className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
                    >
                      <Edit3 className="w-4 h-4" />
                      Rename
                    </button>
                  )}

                  {/* Compress */}
                  <button
                    onClick={handleOpenCompressDialog}
                    className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
                  >
                    <Archive className="w-4 h-4" />
                    Compress ({selectedEntries.size} item{selectedEntries.size > 1 ? 's' : ''})
                  </button>

                  {/* Extract (archives only) */}
                  {hasSelectedArchives && (
                    <button
                      onClick={handleExtract}
                      className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
                    >
                      <FolderOpen className="w-4 h-4" />
                      Extract here
                    </button>
                  )}

                  <div className="h-px bg-tokyo-bg-hl my-1" />

                  {/* Delete */}
                  <button
                    onClick={() => {
                      setContextMenu({ visible: false, x: 0, y: 0 });
                      handleDelete();
                    }}
                    className="w-full px-3 py-1.5 text-left text-sm text-tokyo-red hover:bg-tokyo-bg-hl flex items-center gap-2"
                  >
                    <Trash2 className="w-4 h-4" />
                    Delete ({selectedEntries.size} item{selectedEntries.size > 1 ? 's' : ''})
                  </button>

                  <div className="h-px bg-tokyo-bg-hl my-1" />
                </>
              )}

              {selectedEntries.size === 0 && (
                <button
                  onClick={handleCopyPaths}
                  className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
                >
                  <Copy className="w-4 h-4" />
                  Copy Current Path
                </button>
              )}

              {/* Select All */}
              <button
                onClick={handleSelectAll}
                className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
              >
                <CheckSquare className="w-4 h-4" />
                Select All
              </button>

              {/* Clear Selection */}
              {selectedEntries.size > 0 && (
                <button
                  onClick={handleClearSelection}
                  className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
                >
                  Clear Selection
                </button>
              )}

              <div className="h-px bg-tokyo-bg-hl my-1" />

              {/* New Folder */}
              <button
                onClick={() => {
                  setContextMenu({ visible: false, x: 0, y: 0 });
                  setShowNewFolderDialog(true);
                }}
                className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
              >
                <FolderPlus className="w-4 h-4" />
                New Folder
              </button>

              {/* Upload */}
              {pathTransferEnabled && (
                <button
                  onClick={() => {
                    setContextMenu({ visible: false, x: 0, y: 0 });
                    handleUpload();
                  }}
                  className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
                >
                  <Upload className="w-4 h-4" />
                  Upload Files
                </button>
              )}

              {/* Refresh */}
              <button
                onClick={() => {
                  setContextMenu({ visible: false, x: 0, y: 0 });
                  handleRefresh();
                }}
                className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
              >
                <RefreshCw className="w-4 h-4" />
                Refresh
              </button>
            </div>,
            document.body
          )}
        </div>
      )}

    </div>
  );
});

export type { SftpPanelProps };
