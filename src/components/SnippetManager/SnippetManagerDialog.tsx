import { X } from 'lucide-react';
import SnippetManager from './SnippetManager';

interface SnippetManagerDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

export function SnippetManagerDialog({ isOpen, onClose }: SnippetManagerDialogProps) {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div className="absolute inset-0 glass-scrim" onClick={onClose} />

      {/* Dialog */}
      <div className="glass-dialog glass-form relative border border-tokyo-bg-hl rounded-lg w-full max-w-2xl mx-4 h-[70vh] flex flex-col overflow-hidden">
        {/* Close button */}
        <button
          className="absolute top-2 right-2 z-10 p-1 rounded-md text-tokyo-comment hover:text-tokyo-fg hover:bg-tokyo-bg-hl transition-colors"
          onClick={onClose}
          aria-label="Close snippets"
        >
          <X className="w-5 h-5" />
        </button>

        {/* Content */}
        <div className="flex-1 overflow-hidden">
          <SnippetManager />
        </div>
      </div>
    </div>
  );
}
