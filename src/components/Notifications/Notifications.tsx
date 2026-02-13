import { useEffect } from 'react';
import { X, AlertCircle, CheckCircle, Info, AlertTriangle } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useNotificationStore, type NotificationType } from '../../stores/notificationStore';
import { isTauriAvailable } from '../../lib/tauri';

/**
 * Icon mapping for notification types
 */
const ICONS: Record<NotificationType, typeof AlertCircle> = {
  info: Info,
  success: CheckCircle,
  warning: AlertTriangle,
  error: AlertCircle,
};

/**
 * Color classes for notification types (Tokyo Night theme)
 */
const TYPE_STYLES: Record<NotificationType, { bg: string; border: string; icon: string }> = {
  info: {
    bg: 'bg-tokyo-bg-dark',
    border: 'border-tokyo-blue',
    icon: 'text-tokyo-blue',
  },
  success: {
    bg: 'bg-tokyo-bg-dark',
    border: 'border-tokyo-green',
    icon: 'text-tokyo-green',
  },
  warning: {
    bg: 'bg-tokyo-bg-dark',
    border: 'border-tokyo-orange',
    icon: 'text-tokyo-orange',
  },
  error: {
    bg: 'bg-tokyo-bg-dark',
    border: 'border-tokyo-red',
    icon: 'text-tokyo-red',
  },
};

/**
 * Single toast notification component
 */
interface ToastProps {
  id: string;
  type: NotificationType;
  title: string;
  message: string;
  dismissible: boolean;
  onDismiss: (id: string) => void;
}

function Toast({ id, type, title, message, dismissible, onDismiss }: ToastProps) {
  const styles = TYPE_STYLES[type];
  const Icon = ICONS[type];

  return (
    <div
      className={cn(
        'flex items-start gap-3 p-4 rounded-lg shadow-lg border-l-4',
        'animate-slide-in-right',
        'min-w-[320px] max-w-[420px]',
        styles.bg,
        styles.border
      )}
      role="alert"
    >
      <Icon className={cn('w-5 h-5 flex-shrink-0 mt-0.5', styles.icon)} />
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-white">{title}</p>
        <p className="mt-1 text-sm text-tokyo-fg break-words">{message}</p>
      </div>
      {dismissible && (
        <button
          onClick={() => onDismiss(id)}
          className={cn(
            'flex-shrink-0 p-1 rounded-md',
            'text-tokyo-comment hover:text-white',
            'hover:bg-tokyo-bg-hl transition-colors'
          )}
          aria-label="Dismiss notification"
        >
          <X className="w-4 h-4" />
        </button>
      )}
    </div>
  );
}

/**
 * Tauri unavailable warning banner
 */
function TauriBanner() {
  const { showTauriBanner, dismissTauriBanner } = useNotificationStore();

  if (!showTauriBanner) {
    return null;
  }

  return (
    <div
      className={cn(
        'flex items-center justify-between gap-4 px-4 py-2',
        'bg-tokyo-orange/20 border-b border-tokyo-orange/30'
      )}
      role="alert"
    >
      <div className="flex items-center gap-2">
        <AlertTriangle className="w-4 h-4 text-tokyo-orange flex-shrink-0" />
        <span className="text-sm text-tokyo-orange">
          Running in browser mode. Tauri backend is not available - SSH connections will not work.
        </span>
      </div>
      <button
        onClick={dismissTauriBanner}
        className={cn(
          'p-1 rounded-md flex-shrink-0',
          'text-tokyo-orange hover:text-white',
          'hover:bg-tokyo-orange/20 transition-colors'
        )}
        aria-label="Dismiss banner"
      >
        <X className="w-4 h-4" />
      </button>
    </div>
  );
}

/**
 * Toast container that renders all active notifications
 */
function ToastContainer() {
  const { notifications, removeNotification } = useNotificationStore();

  if (notifications.length === 0) {
    return null;
  }

  return (
    <div
      className={cn(
        'fixed bottom-4 right-4 z-50',
        'flex flex-col gap-2'
      )}
      aria-live="polite"
      aria-label="Notifications"
    >
      {notifications.map((notification) => (
        <Toast
          key={notification.id}
          id={notification.id}
          type={notification.type}
          title={notification.title}
          message={notification.message}
          dismissible={notification.dismissible}
          onDismiss={removeNotification}
        />
      ))}
    </div>
  );
}

/**
 * Main notifications component that includes both the banner and toast container
 * Also handles initial Tauri availability check
 */
export function Notifications() {
  const { setTauriAvailable } = useNotificationStore();

  // Check Tauri availability on mount
  useEffect(() => {
    isTauriAvailable().then(setTauriAvailable);
  }, [setTauriAvailable]);

  return (
    <>
      <TauriBanner />
      <ToastContainer />
    </>
  );
}

/**
 * Export individual components for flexible usage
 */
export { TauriBanner, ToastContainer, Toast };
