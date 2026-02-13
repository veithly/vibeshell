import type { AiTool } from '../../stores/settingsStore';
import { Check, X, AlertCircle } from 'lucide-react';

interface IntegrationCardProps {
  /** The AI tool to display */
  tool: AiTool;
  /** Callback when install button is clicked */
  onInstall: (toolId: string) => void;
  /** Callback when uninstall button is clicked */
  onUninstall: (toolId: string) => void;
  /** Whether an operation is in progress */
  loading?: boolean;
}

/**
 * Displays an AI tool integration card with install/uninstall actions.
 * Styled with Tokyo Night theme colors.
 */
export function IntegrationCard({
  tool,
  onInstall,
  onUninstall,
  loading = false,
}: IntegrationCardProps) {
  const { id, name, installed, vibeshellInstalled } = tool;

  // Determine the status display
  const getStatus = () => {
    if (!installed) {
      return {
        text: 'Not Available',
        color: 'text-tokyo-comment',
        bgColor: 'bg-tokyo-bg-hl',
        icon: <AlertCircle className="w-3 h-3" />,
      };
    }
    if (vibeshellInstalled) {
      return {
        text: 'Skill Installed',
        color: 'text-tokyo-green',
        bgColor: 'bg-tokyo-green/10',
        icon: <Check className="w-3 h-3" />,
      };
    }
    return {
      text: 'Ready to Install',
      color: 'text-tokyo-blue',
      bgColor: 'bg-tokyo-blue/10',
      icon: null,
    };
  };

  const status = getStatus();

  // Determine button state and action
  const renderButton = () => {
    if (!installed) {
      return (
        <button
          disabled
          className="px-4 py-2 text-sm rounded-lg bg-tokyo-bg-hl text-tokyo-comment cursor-not-allowed"
        >
          Not Available
        </button>
      );
    }

    if (vibeshellInstalled) {
      return (
        <button
          onClick={() => onUninstall(id)}
          disabled={loading}
          className="inline-flex items-center gap-2 px-4 py-2 text-sm rounded-lg
                     bg-tokyo-red/10 border border-tokyo-red/30 text-tokyo-red
                     hover:bg-tokyo-red/20 transition-colors
                     disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <X className="w-4 h-4" />
          {loading ? 'Uninstalling...' : 'Uninstall Skill'}
        </button>
      );
    }

    return (
      <button
        onClick={() => onInstall(id)}
        disabled={loading}
        className="px-4 py-2 text-sm rounded-lg
                   bg-tokyo-blue text-white
                   hover:bg-tokyo-blue/80 transition-colors
                   disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {loading ? 'Installing...' : 'Install Skill'}
      </button>
    );
  };

  return (
    <div className="p-4 rounded-lg border border-tokyo-bg-hl bg-tokyo-bg-dark hover:bg-tokyo-bg-hl/50 transition-colors">
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-lg font-semibold text-white">{name}</h3>
        <span
          className={`inline-flex items-center gap-1 px-2 py-1 text-xs rounded-full ${status.bgColor} ${status.color}`}
        >
          {status.icon}
          {status.text}
        </span>
      </div>

      <p className="text-sm text-tokyo-comment mb-4">
        {installed
          ? vibeshellInstalled
            ? 'VibeShell skill is installed and ready to use.'
            : 'Click Install Skill to enable VibeShell in this tool.'
          : 'This AI tool is not detected on your system.'}
      </p>

      <div className="flex justify-end">{renderButton()}</div>
    </div>
  );
}
