import { useState, useEffect, useImperativeHandle, forwardRef, useCallback, useRef, useMemo } from 'react';
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
} from 'lucide-react';
import { cn } from '../../lib/utils';
import { safeInvoke } from '../../lib/tauri';
import { useNotificationStore } from '../../stores/notificationStore';
import { FileIcon, isTextPreviewable, isImagePreviewable } from './FileIcon';
import { PreviewModal } from './PreviewModal';

/**
 * Archive format types supported for compression
 */
type ArchiveFormat = 'tar.gz' | 'zip';

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
  /** Callback when expand button is clicked */
  onExpand?: () => void;
  /** Callback when fullscreen state changes */
  onFullscreenChange?: (isFullscreen: boolean) => void;
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

/**
 * Check if a file can be previewed
 */
function canPreviewFile(name: string): boolean {
  return isTextPreviewable(name) || isImagePreviewable(name);
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
    onExpand: _onExpand,
    onFullscreenChange,
  },
  ref
) {
  const [isCollapsed, setIsCollapsed] = useState(defaultCollapsed);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [panelHeight, setPanelHeight] = useState(defaultHeight);
  const [currentPath, setCurrentPath] = useState<string>('~');
  const [entries, setEntries] = useState<SftpEntry[]>([]);
  const [selectedEntries, setSelectedEntries] = useState<Set<string>>(new Set());
  const [lastSelectedIndex, setLastSelectedIndex] = useState<number | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isInitialized, setIsInitialized] = useState(false);

  // Context menu state
  const [contextMenu, setContextMenu] = useState<ContextMenuState>({ visible: false, x: 0, y: 0 });

  // Resize drag state
  const [isDragging, setIsDragging] = useState(false);
  const dragStartY = useRef<number>(0);
  const dragStartHeight = useRef<number>(0);
  const panelRef = useRef<HTMLDivElement>(null);

  // Rename dialog state
  const [renameEntry, setRenameEntry] = useState<SftpEntry | null>(null);
  const [newName, setNewName] = useState('');

  // New folder dialog state
  const [showNewFolderDialog, setShowNewFolderDialog] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');

  // Compress dialog state
  const [showCompressDialog, setShowCompressDialog] = useState(false);
  const [compressFormat, setCompressFormat] = useState<ArchiveFormat>('tar.gz');
  const [archiveName, setArchiveName] = useState('');

  // Preview modal state
  const [previewEntry, setPreviewEntry] = useState<SftpEntry | null>(null);

  // Drag-and-drop state
  const [isDragOver, setIsDragOver] = useState(false);
  const fileListRef = useRef<HTMLDivElement>(null);

  const { success: notifySuccess, error: notifyError } = useNotificationStore();

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

  // Check if any selected entries are archives (for extract option)
  const hasSelectedArchives = useMemo(() => {
    return selectedEntriesArray.some(e => !e.isDirectory && isArchiveFile(e.name));
  }, [selectedEntriesArray]);

  // Check if selected file can be previewed
  const canPreviewSelected = useMemo(() => {
    return selectedEntry && !selectedEntry.isDirectory && canPreviewFile(selectedEntry.name);
  }, [selectedEntry]);

  // Reset to collapsed when session changes
  useEffect(() => {
    if (!sessionId) {
      setIsCollapsed(true);
      setIsFullscreen(false);
      setIsInitialized(false);
      setEntries([]);
      setSelectedEntries(new Set());
      setCurrentPath('~');
    }
  }, [sessionId]);

  // Handle fullscreen change callback
  useEffect(() => {
    onFullscreenChange?.(isFullscreen);
  }, [isFullscreen, onFullscreenChange]);

  // Close context menu on click outside
  useEffect(() => {
    const handleClickOutside = () => {
      if (contextMenu.visible) {
        setContextMenu({ visible: false, x: 0, y: 0 });
      }
    };
    document.addEventListener('click', handleClickOutside);
    return () => document.removeEventListener('click', handleClickOutside);
  }, [contextMenu.visible]);

  // Handle resize dragging
  useEffect(() => {
    if (!isDragging) return;

    const handleMouseMove = (e: MouseEvent) => {
      const deltaY = dragStartY.current - e.clientY;
      const newHeight = Math.min(maxHeight, Math.max(minHeight, dragStartHeight.current + deltaY));
      setPanelHeight(newHeight);
    };

    const handleMouseUp = () => {
      setIsDragging(false);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    document.body.style.cursor = 'ns-resize';
    document.body.style.userSelect = 'none';

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isDragging, minHeight, maxHeight]);

  // Expose methods to parent via ref
  useImperativeHandle(ref, () => ({
    expand: () => setIsCollapsed(false),
    collapse: () => setIsCollapsed(true),
    toggle: () => setIsCollapsed((prev) => !prev),
    isCollapsed: () => isCollapsed,
    enterFullscreen: () => setIsFullscreen(true),
    exitFullscreen: () => setIsFullscreen(false),
    toggleFullscreen: () => setIsFullscreen((prev) => !prev),
    isFullscreen: () => isFullscreen,
  }), [isCollapsed, isFullscreen]);

  const loadDirectory = useCallback(async (path: string) => {
    if (!sessionId) return;

    setIsLoading(true);
    setError(null);
    setSelectedEntries(new Set());
    setLastSelectedIndex(null);

    try {
      const result = await safeInvoke<SftpEntry[]>('sftp_list_dir', {
        request: {
          sessionId: sessionId,
          path: path,
        },
      });

      if (result.success) {
        // Sort: directories first, then files, alphabetically
        const sorted = result.data.sort((a, b) => {
          if (a.isDirectory && !b.isDirectory) return -1;
          if (!a.isDirectory && b.isDirectory) return 1;
          return a.name.localeCompare(b.name);
        });
        setEntries(sorted);
        setCurrentPath(path);
      } else {
        throw new Error(result.error.message);
      }
    } catch (err) {
      console.error('[SftpPanel] Failed to load directory:', err);
      setError(err instanceof Error ? err.message : 'Failed to load directory');
    } finally {
      setIsLoading(false);
    }
  }, [sessionId]);

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
      setError(err instanceof Error ? err.message : 'Failed to initialize SFTP');
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

  const handleRefresh = useCallback(() => {
    loadDirectory(currentPath);
  }, [currentPath, loadDirectory]);

  const handleGoHome = useCallback(() => {
    loadDirectory('~');
  }, [loadDirectory]);

  const handleUpload = useCallback(async () => {
    if (!sessionId) return;

    try {
      // Pick file to upload
      const pickResult = await safeInvoke<string | null>('pick_file_for_upload');

      if (!pickResult.success || !pickResult.data) return;

      const localPath = pickResult.data;
      const fileName = localPath.split(/[/\\]/).pop() || 'file';
      const remotePath = `${currentPath}/${fileName}`;

      setIsLoading(true);

      const uploadResult = await safeInvoke('sftp_upload_file', {
        request: {
          sessionId: sessionId,
          localPath: localPath,
          remotePath: remotePath,
        },
      });

      if (uploadResult.success) {
        notifySuccess('Upload Complete', `${fileName} uploaded successfully`);
        await loadDirectory(currentPath);
      } else {
        throw new Error(uploadResult.error.message);
      }
    } catch (err) {
      console.error('[SftpPanel] Upload failed:', err);
      notifyError('Upload Failed', err instanceof Error ? err.message : 'Failed to upload file');
    } finally {
      setIsLoading(false);
    }
  }, [sessionId, currentPath, loadDirectory, notifySuccess, notifyError]);

  // Handle files dropped via drag-and-drop
  const handleDropFiles = useCallback(async (paths: string[]) => {
    if (!sessionId || paths.length === 0) return;

    setIsLoading(true);
    let successCount = 0;
    let failCount = 0;

    for (const localPath of paths) {
      const fileName = localPath.split(/[/\\]/).pop() || 'file';
      const remotePath = `${currentPath}/${fileName}`;

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
          failCount++;
          console.error(`[SftpPanel] Upload failed for ${fileName}:`, result.error.message);
        }
      } catch (err) {
        failCount++;
        console.error(`[SftpPanel] Upload failed for ${fileName}:`, err);
      }
    }

    if (successCount > 0) {
      notifySuccess('Upload Complete', `${successCount} file(s) uploaded successfully`);
    }
    if (failCount > 0) {
      notifyError('Upload Failed', `${failCount} file(s) failed to upload`);
    }

    await loadDirectory(currentPath);
    setIsLoading(false);
  }, [sessionId, currentPath, loadDirectory, notifySuccess, notifyError]);

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
    if (!sessionId || !selectedEntry || selectedEntry.isDirectory) return;

    try {
      // Pick download directory
      const pickResult = await safeInvoke<string | null>('pick_download_directory');

      if (!pickResult.success || !pickResult.data) return;

      const localPath = `${pickResult.data}/${selectedEntry.name}`;

      setIsLoading(true);

      const downloadResult = await safeInvoke('sftp_download_file', {
        request: {
          sessionId: sessionId,
          remotePath: selectedEntry.path,
          localPath: localPath,
        },
      });

      if (downloadResult.success) {
        notifySuccess('Download Complete', `${selectedEntry.name} downloaded successfully`);
      } else {
        throw new Error(downloadResult.error.message);
      }
    } catch (err) {
      console.error('[SftpPanel] Download failed:', err);
      notifyError('Download Failed', err instanceof Error ? err.message : 'Failed to download file');
    } finally {
      setIsLoading(false);
    }
  }, [sessionId, selectedEntry, notifySuccess, notifyError]);

  const handleDelete = useCallback(async () => {
    if (!sessionId || selectedEntriesArray.length === 0) return;

    const count = selectedEntriesArray.length;
    const confirmed = window.confirm(
      count === 1
        ? `Are you sure you want to delete ${selectedEntriesArray[0].name}?`
        : `Are you sure you want to delete ${count} items?`
    );

    if (!confirmed) return;

    try {
      setIsLoading(true);

      let successCount = 0;
      let failCount = 0;

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
          failCount++;
        }
      }

      if (successCount > 0) {
        notifySuccess('Deleted', `${successCount} item(s) deleted successfully`);
      }
      if (failCount > 0) {
        notifyError('Delete Failed', `Failed to delete ${failCount} item(s)`);
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

  // Open preview for a file
  const handlePreview = useCallback((entry?: SftpEntry) => {
    const targetEntry = entry || selectedEntry;
    if (!targetEntry || targetEntry.isDirectory || !canPreviewFile(targetEntry.name)) return;
    setPreviewEntry(targetEntry);
    setContextMenu({ visible: false, x: 0, y: 0 });
  }, [selectedEntry]);

  // Close preview modal
  const handleClosePreview = useCallback(() => {
    setPreviewEntry(null);
  }, []);

  // Save file content from preview modal editor
  const handleSavePreviewFile = useCallback(async (content: string) => {
    if (!sessionId || !previewEntry) return;

    try {
      const saveResult = await safeInvoke('sftp_write_file', {
        request: {
          sessionId: sessionId,
          path: previewEntry.path,
          content: content,
        },
      });

      if (saveResult.success) {
        notifySuccess('Saved', `${previewEntry.name} saved successfully`);
      } else {
        throw new Error(saveResult.error.message);
      }
    } catch (err) {
      console.error('[SftpPanel] Save failed:', err);
      notifyError('Save Failed', err instanceof Error ? err.message : 'Failed to save file');
      throw err;
    }
  }, [sessionId, previewEntry, notifySuccess, notifyError]);

  // Download file for preview modal
  const handleDownloadPreviewFile = useCallback(async () => {
    if (!sessionId || !previewEntry) return;

    try {
      const pickResult = await safeInvoke<string | null>('pick_download_directory');
      if (!pickResult.success || !pickResult.data) return;

      const localPath = `${pickResult.data}/${previewEntry.name}`;
      setIsLoading(true);

      const downloadResult = await safeInvoke('sftp_download_file', {
        request: {
          sessionId: sessionId,
          remotePath: previewEntry.path,
          localPath: localPath,
        },
      });

      if (downloadResult.success) {
        notifySuccess('Download Complete', `${previewEntry.name} downloaded successfully`);
      } else {
        throw new Error(downloadResult.error.message);
      }
    } catch (err) {
      console.error('[SftpPanel] Download failed:', err);
      notifyError('Download Failed', err instanceof Error ? err.message : 'Failed to download file');
    } finally {
      setIsLoading(false);
    }
  }, [sessionId, previewEntry, notifySuccess, notifyError]);

  // Start resize drag
  const handleResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragStartY.current = e.clientY;
    dragStartHeight.current = panelHeight;
    setIsDragging(true);
  }, [panelHeight]);

  const toggleCollapse = () => {
    setIsCollapsed(!isCollapsed);
  };

  const toggleFullscreen = () => {
    setIsFullscreen(!isFullscreen);
  };

  if (!sessionId) {
    return null;
  }

  // Parse path for breadcrumb navigation
  const pathParts = currentPath.split('/').filter(Boolean);

  // Calculate panel height based on state
  const getPanelHeight = () => {
    if (isFullscreen) return '100%';
    if (isCollapsed) return '40px';
    return `${panelHeight}px`;
  };

  return (
    <div
      ref={panelRef}
      className={cn(
        'relative border-t border-tokyo-bg-hl bg-tokyo-bg-dark transition-all duration-200',
        // Fullscreen: absolute within the <main> container (not fixed over the whole viewport)
        // so it doesn't cover the left sidebar
        isFullscreen && 'absolute inset-0 z-40 border-t-0'
      )}
      style={{ height: getPanelHeight() }}
    >
      {/* Resize Handle (when not collapsed and not fullscreen) */}
      {!isCollapsed && !isFullscreen && (
        <div
          className={cn(
            'absolute top-0 left-0 right-0 h-2 cursor-ns-resize z-10',
            'group flex items-center justify-center',
            'hover:bg-tokyo-blue/30 transition-colors',
            isDragging && 'bg-tokyo-blue/50'
          )}
          onMouseDown={handleResizeStart}
          title="Drag to resize"
        >
          <div className={cn(
            'flex items-center justify-center w-12 h-4 -mt-2 rounded-t',
            'bg-tokyo-bg-hl/80 border border-b-0 border-tokyo-bg-hl',
            'opacity-60 group-hover:opacity-100 transition-opacity',
            isDragging && 'opacity-100 bg-tokyo-blue/30'
          )}>
            <GripHorizontal className="w-4 h-4 text-tokyo-comment" />
          </div>
        </div>
      )}

      {/* Header */}
      <div
        className={cn(
          'flex items-center justify-between px-3 h-10',
          'border-b border-tokyo-bg-hl cursor-pointer',
          'hover:bg-tokyo-bg-hl/30 transition-colors duration-150'
        )}
        onClick={toggleCollapse}
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
        <div className="flex flex-col h-[calc(100%-40px)] relative">
          {/* Toolbar */}
          <div className="flex items-center gap-1 px-2 py-1 border-b border-tokyo-bg-hl bg-tokyo-bg">
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

            <div className="w-px h-4 bg-tokyo-bg-hl mx-1" />

            <button
              onClick={(e) => { e.stopPropagation(); setShowNewFolderDialog(true); }}
              disabled={isLoading}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="New folder"
            >
              <FolderPlus className="w-4 h-4 text-tokyo-comment" />
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); handleUpload(); }}
              disabled={isLoading}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="Upload"
            >
              <Upload className="w-4 h-4 text-tokyo-comment" />
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); handleDownload(); }}
              disabled={isLoading || !selectedEntry || selectedEntry.isDirectory}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="Download"
            >
              <Download className="w-4 h-4 text-tokyo-comment" />
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); handlePreview(); }}
              disabled={isLoading || !canPreviewSelected}
              className="p-1.5 rounded hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
              title="Preview"
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

            {/* Breadcrumb path */}
            <div className="flex-1 flex items-center gap-1 ml-2 text-xs text-tokyo-comment overflow-hidden">
              <span
                className="text-tokyo-fg cursor-pointer hover:text-tokyo-blue hover:underline"
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
          </div>

          {/* Error display */}
          {error && (
            <div className="flex items-center gap-2 px-3 py-2 bg-red-900/20 border-b border-red-800/30 text-red-400 text-sm">
              <AlertCircle className="w-4 h-4 flex-shrink-0" />
              <span className="truncate">{error}</span>
              <button
                onClick={() => setError(null)}
                className="ml-auto text-red-400 hover:text-red-300"
              >
                Dismiss
              </button>
            </div>
          )}

          {/* File list */}
          <div
            ref={fileListRef}
            className="flex-1 overflow-auto relative"
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
            {entries.length === 0 && !isLoading && !error ? (
              <div className="flex items-center justify-center h-full text-tokyo-comment text-sm">
                Empty directory
              </div>
            ) : (
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
                            <Check className="w-3 h-3 text-white" />
                          )}
                          {selectedEntries.size > 0 && selectedEntries.size < entries.length && (
                            <Minus className="w-3 h-3 text-white" />
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
                      onClick={(e) => handleEntryClick(entry, index, e)}
                      onContextMenu={(e) => handleContextMenu(e, entry, index)}
                      onDoubleClick={(e) => {
                        e.stopPropagation();
                        e.preventDefault();
                        if (entry.isDirectory) {
                          navigateToEntry(entry);
                        } else if (canPreviewFile(entry.name)) {
                          handlePreview(entry);
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
                              <Check className="w-3 h-3 text-white" />
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
                    className="px-3 py-1.5 rounded text-sm bg-tokyo-blue text-white hover:bg-tokyo-blue/80 disabled:opacity-50"
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
                    className="px-3 py-1.5 rounded text-sm bg-tokyo-blue text-white hover:bg-tokyo-blue/80 disabled:opacity-50"
                  >
                    Create
                  </button>
                </div>
              </div>
            </div>
          )}

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
                    className="px-3 py-1.5 rounded text-sm bg-tokyo-blue text-white hover:bg-tokyo-blue/80 disabled:opacity-50"
                  >
                    Compress
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* Context Menu */}
          {contextMenu.visible && (
            <div
              className={cn(
                'fixed bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg shadow-lg py-1 z-[60]',
                'min-w-[160px]'
              )}
              style={{ left: contextMenu.x, top: contextMenu.y }}
              onClick={(e) => e.stopPropagation()}
            >
              {selectedEntries.size > 0 && (
                <>
                  {/* Preview (single file only, previewable types) */}
                  {selectedEntries.size === 1 && selectedEntry && !selectedEntry.isDirectory && canPreviewFile(selectedEntry.name) && (
                    <button
                      onClick={() => handlePreview()}
                      className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
                    >
                      <Eye className="w-4 h-4" />
                      Preview
                    </button>
                  )}

                  {/* Download (single file only) */}
                  {selectedEntries.size === 1 && selectedEntry && !selectedEntry.isDirectory && (
                    <button
                      onClick={() => {
                        setContextMenu({ visible: false, x: 0, y: 0 });
                        handleDownload();
                      }}
                      className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
                    >
                      <Download className="w-4 h-4" />
                      Download
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
              <button
                onClick={() => {
                  setContextMenu({ visible: false, x: 0, y: 0 });
                  handleUpload();
                }}
                className="w-full px-3 py-1.5 text-left text-sm text-tokyo-fg hover:bg-tokyo-bg-hl flex items-center gap-2"
              >
                <Upload className="w-4 h-4" />
                Upload File
              </button>

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
            </div>
          )}
        </div>
      )}

      {/* Preview Modal */}
      {previewEntry && sessionId && (
        <PreviewModal
          isOpen={true}
          filePath={previewEntry.path}
          fileName={previewEntry.name}
          fileSize={previewEntry.size}
          sessionId={sessionId}
          onClose={handleClosePreview}
          onDownload={handleDownloadPreviewFile}
          onSave={handleSavePreviewFile}
        />
      )}
    </div>
  );
});

export type { SftpPanelProps };
