import { create } from 'zustand';
import { safeInvoke } from '../lib/tauri';
import type { Recording } from '../types/tunnel';

interface RecordingStore {
  /** Map of sessionId -> recordingId for active recordings */
  activeRecordings: Record<string, string>;
  /** List of all recordings */
  recordings: Recording[];
  loading: boolean;
  error: string | null;

  // Actions
  startRecording: (sessionId: string, serverId: string) => Promise<string | null>;
  stopRecording: (sessionId: string) => Promise<void>;
  isRecording: (sessionId: string) => boolean;
  fetchRecordings: () => Promise<void>;
  deleteRecording: (recordingId: string) => Promise<void>;
  checkRecordingStatus: (sessionId: string) => Promise<void>;
  clearError: () => void;
}

export const useRecordingStore = create<RecordingStore>((set, get) => ({
  activeRecordings: {},
  recordings: [],
  loading: false,
  error: null,

  startRecording: async (sessionId: string, serverId: string) => {
    try {
      const result = await safeInvoke<string>('start_recording', { sessionId, serverId });
      if (result.success) {
        set((state) => ({
          activeRecordings: { ...state.activeRecordings, [sessionId]: result.data },
          error: null,
        }));
        return result.data;
      } else {
        set({ error: result.error.message || 'Failed to start recording' });
        return null;
      }
    } catch (e) {
      set({ error: e instanceof Error ? e.message : 'Failed to start recording' });
      return null;
    }
  },

  stopRecording: async (sessionId: string) => {
    try {
      const result = await safeInvoke('stop_recording', { sessionId });
      if (result.success) {
        set((state) => {
          const { [sessionId]: _, ...rest } = state.activeRecordings;
          return { activeRecordings: rest, error: null };
        });
      } else {
        set({ error: result.error.message || 'Failed to stop recording' });
      }
    } catch (e) {
      set({ error: e instanceof Error ? e.message : 'Failed to stop recording' });
    }
  },

  isRecording: (sessionId: string) => {
    return !!get().activeRecordings[sessionId];
  },

  fetchRecordings: async () => {
    set({ loading: true });
    try {
      const result = await safeInvoke<Recording[]>('list_recordings');
      if (result.success) {
        set({ recordings: result.data || [], loading: false, error: null });
      } else {
        set({ loading: false, error: result.error.message || 'Failed to fetch recordings' });
      }
    } catch (e) {
      set({ loading: false, error: e instanceof Error ? e.message : 'Failed to fetch recordings' });
    }
  },

  deleteRecording: async (recordingId: string) => {
    try {
      const result = await safeInvoke('delete_recording', { recordingId });
      if (result.success) {
        set((state) => ({
          recordings: state.recordings.filter((r) => r.id !== recordingId),
          error: null,
        }));
      } else {
        set({ error: result.error.message || 'Failed to delete recording' });
      }
    } catch (e) {
      set({ error: e instanceof Error ? e.message : 'Failed to delete recording' });
    }
  },

  checkRecordingStatus: async (sessionId: string) => {
    try {
      const result = await safeInvoke<boolean>('is_session_recording', { sessionId });
      if (result.success && result.data) {
        // Session is recording, get the recording ID
        const idResult = await safeInvoke<string>('get_session_recording_id', { sessionId });
        if (idResult.success && idResult.data) {
          set((state) => ({
            activeRecordings: { ...state.activeRecordings, [sessionId]: idResult.data! },
          }));
        }
      }
    } catch {
      // Silently ignore check errors
    }
  },

  clearError: () => set({ error: null }),
}));
