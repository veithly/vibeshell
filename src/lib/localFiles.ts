import i18n from 'i18next';
import { safeInvoke } from './tauri';
import { getFileViewerKind } from './fileWorkspace';
import { LOCAL_FILE_ORIGIN, useFileWorkspaceStore } from '../stores/fileWorkspaceStore';
import { usePluginWorkspaceStore } from '../stores/pluginWorkspaceStore';
import { useNavigationStore } from '../stores/navigationStore';
import { useNotificationStore } from '../stores/notificationStore';
import { openDetachedWindow, useDetachedOwnership } from './detach';
import { filePaneId } from './paneIds';

interface LocalFileInfo { path: string; name: string; size: number }
let picking = false;

/** Explicit paths or a native multi-select picker. Never creates a terminal. */
export async function openLocalFiles(paths?: readonly string[]): Promise<void> {
  const chooser = paths === undefined;
  if (chooser && picking) return;
  if (chooser) picking = true;
  try {
    if (chooser) {
      const selected = await safeInvoke<string[]>('pick_local_files');
      if (!selected.success) throw new Error(selected.error.message);
      paths = selected.data;
    }
    for (const path of [...new Set(paths ?? [])].slice(0, 128)) {
      try {
        const result = await safeInvoke<LocalFileInfo>('local_file_stat', { path });
        if (!result.success) throw new Error(result.error.message);
        const file = result.data;
        const id = `${LOCAL_FILE_ORIGIN}\u0000${file.path}`;
        if (useDetachedOwnership.getState().owners[filePaneId(id)]) {
          await openDetachedWindow({ kind: 'file', source: 'local', sessionId: LOCAL_FILE_ORIGIN, ...file });
          continue;
        }
        const kind = getFileViewerKind(file.name);
        useFileWorkspaceStore.getState().openFile({ ...file, source: 'local', sessionId: LOCAL_FILE_ORIGIN,
          // Unknown extensions get a bounded UTF-8 read. The backend rejects
          // binary/unsupported encodings rather than corrupting them on save.
          viewerKind: kind === 'unsupported' ? 'text' : kind });
        usePluginWorkspaceStore.getState().activateTab(null);
        useNavigationStore.getState().goToMain();
      } catch (error) {
        useNotificationStore.getState().error(i18n.t('localFiles.openFailed'), `${path}: ${String(error)}`);
      }
    }
  } catch (error) {
    useNotificationStore.getState().error(i18n.t('localFiles.openFailed'), String(error));
  } finally { if (chooser) picking = false; }
}

/** Listener-first + drain handles both cold launch and a second application invocation. */
export async function listenForLocalFileRequests(): Promise<() => void> {
  if (!('__TAURI_INTERNALS__' in window)) return () => {};
  const { listen } = await import('@tauri-apps/api/event');
  let disposed = false;
  let draining = false;
  let requested = false;
  const drain = async () => {
    requested = true;
    if (draining) return;
    draining = true;
    try {
      while (requested && !disposed) {
        requested = false;
        const result = await safeInvoke<string[]>('take_pending_open_files');
        if (!result.success) throw new Error(result.error.message);
        await openLocalFiles(result.data);
      }
    } catch (error) { console.error('[Local files]', error); }
    finally { draining = false; }
  };
  const stops = await Promise.all([
    listen('vibeshell://open-files-pending', () => { void drain(); }),
    listen('vibeshell://choose-local-files', () => { void openLocalFiles(); }),
  ]);
  void drain();
  return () => { disposed = true; stops.forEach(stop => stop()); };
}
