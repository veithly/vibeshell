import { X } from 'lucide-react';
import TunnelPanel from './TunnelPanel';

interface TunnelPanelDialogProps {
  isOpen: boolean;
  serverId: string;
  sessionId?: string;
  onClose: () => void;
}

export function TunnelPanelDialog({ isOpen, serverId, sessionId, onClose }: TunnelPanelDialogProps) {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/60" onClick={onClose} />

      {/* Dialog */}
      <div className="relative bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg shadow-xl w-full max-w-2xl mx-4 h-[60vh] flex flex-col overflow-hidden">
        {/* Close button */}
        <button
          className="absolute top-2 right-2 z-10 p-1 rounded-md text-tokyo-comment hover:text-white hover:bg-tokyo-bg-hl transition-colors"
          onClick={onClose}
        >
          <X className="w-5 h-5" />
        </button>

        {/* Content */}
        <div className="flex-1 overflow-hidden">
          <TunnelPanel serverId={serverId} sessionId={sessionId} />
        </div>
      </div>
    </div>
  );
}
