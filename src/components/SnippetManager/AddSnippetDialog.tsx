import { useState } from 'react';
import { X } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useSnippetStore } from '../../stores/snippetStore';
import type { CommandSnippet } from '../../types/tunnel';

interface AddSnippetDialogProps {
  snippet: CommandSnippet | null;
  onClose: () => void;
}

export default function AddSnippetDialog({ snippet, onClose }: AddSnippetDialogProps) {
  const { addSnippet, updateSnippet } = useSnippetStore();
  const isEditing = !!snippet;

  const [name, setName] = useState(snippet?.name || '');
  const [command, setCommand] = useState(snippet?.command || '');
  const [category, setCategory] = useState(snippet?.category || '');
  const [description, setDescription] = useState(snippet?.description || '');
  const [tagsStr, setTagsStr] = useState(snippet?.tags.join(', ') || '');

  const handleSave = async () => {
    if (!name.trim() || !command.trim()) return;

    const tags = tagsStr.split(',').map(t => t.trim()).filter(Boolean);

    try {
      if (isEditing && snippet) {
        await updateSnippet(snippet.id, { name, command, category, description, tags });
      } else {
        await addSnippet({ name, command, category, description, tags });
      }
      onClose();
    } catch { /* error handled in store */ }
  };

  const inputClass = cn(
    'w-full bg-tokyo-bg border border-tokyo-bg-hl rounded-md px-3 py-2 text-sm text-tokyo-fg',
    'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue',
    'placeholder:text-tokyo-comment transition-colors'
  );

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/60" onClick={onClose}>
      <div
        className="bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg w-[480px] max-w-[90vw] shadow-2xl"
        onClick={e => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-tokyo-bg-hl">
          <h3 className="text-sm font-semibold text-white">
            {isEditing ? 'Edit Snippet' : 'Add Snippet'}
          </h3>
          <button
            onClick={onClose}
            className="p-1 rounded-md hover:bg-tokyo-bg-hl text-tokyo-comment hover:text-white transition-colors cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Form */}
        <form onSubmit={(e) => { e.preventDefault(); handleSave(); }} className="p-4 space-y-3">
          <div>
            <label className="block text-xs text-tokyo-comment mb-1.5 font-medium">Name *</label>
            <input
              type="text"
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder="e.g. Restart Nginx"
              className={inputClass}
              autoFocus
            />
          </div>

          <div>
            <label className="block text-xs text-tokyo-comment mb-1.5 font-medium">Command *</label>
            <textarea
              value={command}
              onChange={e => setCommand(e.target.value)}
              placeholder="sudo systemctl restart nginx"
              rows={3}
              className={cn(inputClass, 'font-mono resize-none')}
            />
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs text-tokyo-comment mb-1.5 font-medium">Category</label>
              <input
                type="text"
                value={category}
                onChange={e => setCategory(e.target.value)}
                placeholder="e.g. Server, Docker"
                className={inputClass}
              />
            </div>
            <div>
              <label className="block text-xs text-tokyo-comment mb-1.5 font-medium">Tags (comma-separated)</label>
              <input
                type="text"
                value={tagsStr}
                onChange={e => setTagsStr(e.target.value)}
                placeholder="nginx, restart"
                className={inputClass}
              />
            </div>
          </div>

          <div>
            <label className="block text-xs text-tokyo-comment mb-1.5 font-medium">Description</label>
            <input
              type="text"
              value={description}
              onChange={e => setDescription(e.target.value)}
              placeholder="Brief description of what this command does"
              className={inputClass}
            />
          </div>

          {/* Actions */}
          <div className="flex justify-end gap-2 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-sm rounded-md bg-tokyo-bg-hl text-tokyo-fg
                         hover:bg-tokyo-selection hover:text-white transition-colors cursor-pointer"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!name.trim() || !command.trim()}
              className="px-4 py-2 text-sm rounded-md bg-tokyo-blue text-white
                         hover:bg-tokyo-blue/80 disabled:opacity-40 disabled:cursor-not-allowed
                         transition-colors cursor-pointer"
            >
              {isEditing ? 'Update' : 'Save'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
