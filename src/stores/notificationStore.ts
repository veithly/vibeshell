import { create } from 'zustand';

/**
 * Notification types for different message severities
 */
export type NotificationType = 'info' | 'success' | 'warning' | 'error';

/**
 * A notification/toast message
 */
export interface Notification {
  id: string;
  type: NotificationType;
  title: string;
  message: string;
  /** Auto-dismiss after this many milliseconds (0 = never auto-dismiss) */
  duration: number;
  /** Timestamp when the notification was created */
  createdAt: number;
  /** Whether this notification can be dismissed by the user */
  dismissible: boolean;
}

/**
 * Input for creating a notification (without auto-generated fields)
 */
export interface NotificationInput {
  type: NotificationType;
  title: string;
  message: string;
  /** Auto-dismiss after this many milliseconds (default: 5000, 0 = never) */
  duration?: number;
  /** Whether this notification can be dismissed by the user (default: true) */
  dismissible?: boolean;
}

/**
 * Notification store state and actions
 */
interface NotificationStore {
  /** List of active notifications */
  notifications: Notification[];
  /** Whether Tauri is available (null = not checked yet) */
  tauriAvailable: boolean | null;
  /** Whether to show the Tauri unavailable banner */
  showTauriBanner: boolean;

  /** Add a notification */
  addNotification: (input: NotificationInput) => string;
  /** Remove a notification by ID */
  removeNotification: (id: string) => void;
  /** Clear all notifications */
  clearAll: () => void;

  /** Convenience methods for different notification types */
  info: (title: string, message: string, duration?: number) => string;
  success: (title: string, message: string, duration?: number) => string;
  warning: (title: string, message: string, duration?: number) => string;
  error: (title: string, message: string, duration?: number) => string;

  /** Set Tauri availability status */
  setTauriAvailable: (available: boolean) => void;
  /** Dismiss the Tauri unavailable banner */
  dismissTauriBanner: () => void;
}

/**
 * Generate a unique notification ID
 */
function generateId(): string {
  return `notif-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
}

/**
 * Default durations by notification type (in milliseconds)
 */
const DEFAULT_DURATIONS: Record<NotificationType, number> = {
  info: 5000,
  success: 3000,
  warning: 8000,
  error: 0, // Errors don't auto-dismiss by default
};

/**
 * Zustand store for managing notifications/toasts
 */
export const useNotificationStore = create<NotificationStore>((set, get) => ({
  notifications: [],
  tauriAvailable: null,
  showTauriBanner: false,

  addNotification: (input: NotificationInput) => {
    const id = generateId();
    const notification: Notification = {
      id,
      type: input.type,
      title: input.title,
      message: input.message,
      duration: input.duration ?? DEFAULT_DURATIONS[input.type],
      createdAt: Date.now(),
      dismissible: input.dismissible ?? true,
    };

    set((state) => ({
      notifications: [...state.notifications, notification],
    }));

    // Set up auto-dismiss if duration > 0
    if (notification.duration > 0) {
      setTimeout(() => {
        get().removeNotification(id);
      }, notification.duration);
    }

    return id;
  },

  removeNotification: (id: string) => {
    set((state) => ({
      notifications: state.notifications.filter((n) => n.id !== id),
    }));
  },

  clearAll: () => {
    set({ notifications: [] });
  },

  // Convenience methods
  info: (title: string, message: string, duration?: number) => {
    return get().addNotification({ type: 'info', title, message, duration });
  },

  success: (title: string, message: string, duration?: number) => {
    return get().addNotification({ type: 'success', title, message, duration });
  },

  warning: (title: string, message: string, duration?: number) => {
    return get().addNotification({ type: 'warning', title, message, duration });
  },

  error: (title: string, message: string, duration?: number) => {
    return get().addNotification({ type: 'error', title, message, duration });
  },

  setTauriAvailable: (available: boolean) => {
    set({
      tauriAvailable: available,
      showTauriBanner: !available,
    });
  },

  dismissTauriBanner: () => {
    set({ showTauriBanner: false });
  },
}));

/**
 * Helper function to show an error notification from a Tauri error
 */
export function notifyTauriError(error: unknown, context?: string): void {
  const { error: showError } = useNotificationStore.getState();

  let message: string;
  if (error instanceof Error) {
    message = error.message;
  } else if (typeof error === 'string') {
    message = error;
  } else {
    message = 'An unknown error occurred';
  }

  const title = context ? `${context} Failed` : 'Operation Failed';
  showError(title, message);
}
