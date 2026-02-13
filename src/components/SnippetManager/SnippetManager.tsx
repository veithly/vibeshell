import { useState, useEffect, useMemo } from 'react';
import { Terminal, Play, Edit3, Trash2, Copy, Plus, Search, Tag, X } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useSnippetStore } from '../../stores/snippetStore';
import { useSessionStore } from '../../stores/sessionStore';
import AddSnippetDialog from './AddSnippetDialog';

export default function SnippetManager() {
  const { snippets, fetchSnippets, deleteSnippet, searchSnippets } = useSnippetStore();
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedCategory, setSelectedCategory] = useState<string | null>(null);
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [editSnippet, setEditSnippet] = useState<typeof snippets[0] | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  useEffect(() => {
    fetchSnippets();
  }, []);

  const categories = useMemo(() => {
    const cats = new Set(snippets.map(s => s.category).filter(Boolean));
    return Array.from(cats).sort();
  }, [snippets]);

  const filteredSnippets = useMemo(() => {
    let result = snippets;
    if (selectedCategory) {
      result = result.filter(s => s.category === selectedCategory);
    }
    return result;
  }, [snippets, selectedCategory]);

  const handleSearch = () => {
    if (searchQuery.trim()) {
      searchSnippets(searchQuery);
    } else {
      fetchSnippets();
    }
  };

  const handleExecute = (command: string) => {
    const sessionStore = useSessionStore.getState();
    const activeId = sessionStore.activeSessionId;
    if (activeId) {
      sessionStore.sendInput(activeId, command + '\n');
    }
  };

  const handleCopy = async (id: string, command: string) => {
    try {
      await navigator.clipboard.writeText(command);
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 1500);
    } catch { /* fallback */ }
  };

  return (
    <div className="flex flex-col h-full bg-tokyo-bg text-tokyo-fg">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-tokyo-bg-hl">
        <div className="flex items-center gap-2">
          <Terminal className="w-4 h-4 text-tokyo-magenta" />
          <span className="text-sm font-semibold text-white">Command Snippets</span>
          <span className="text-xs text-tokyo-comment px-1.5 py-0.5 rounded-full bg-tokyo-bg-hl">
            {snippets.length}
          </span>
        </div>
        <button
          onClick={() => { setEditSnippet(null); setShowAddDialog(true); }}
          className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs rounded-md bg-tokyo-green/10 text-tokyo-green
                     hover:bg-tokyo-green/20 transition-colors cursor-pointer font-medium"
        >
          <Plus className="w-3.5 h-3.5" />
          Add
        </button>
      </div>

      {/* Search */}
      <div className="px-3 py-2.5 border-b border-tokyo-bg-hl">
        <div className="flex items-center gap-2">
          <div className="flex-1 flex items-center bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-md px-2.5
                          focus-within:ring-1 focus-within:ring-tokyo-blue focus-within:border-tokyo-blue transition-colors">
            <Search className="w-3.5 h-3.5 text-tokyo-comment flex-shrink-0" />
            <input
              type="text"
              placeholder="Search snippets..."
              value={searchQuery}
              onChange={e => setSearchQuery(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleSearch()}
              className="flex-1 bg-transparent px-2 py-1.5 text-xs text-tokyo-fg placeholder:text-tokyo-comment outline-none"
            />
            {searchQuery && (
              <button
                onClick={() => { setSearchQuery(''); fetchSnippets(); }}
                className="text-tokyo-comment hover:text-tokyo-fg transition-colors cursor-pointer p-0.5"
              >
                <X className="w-3 h-3" />
              </button>
            )}
          </div>
        </div>

        {/* Category pills */}
        {categories.length > 0 && (
          <div className="flex gap-1.5 mt-2 flex-wrap">
            <button
              onClick={() => setSelectedCategory(null)}
              className={cn(
                'px-2.5 py-1 text-xs rounded-full transition-colors cursor-pointer font-medium',
                !selectedCategory
                  ? 'bg-tokyo-blue/15 text-tokyo-blue'
                  : 'bg-tokyo-bg-hl text-tokyo-comment hover:text-tokyo-fg'
              )}
            >
              All
            </button>
            {categories.map(cat => (
              <button
                key={cat}
                onClick={() => setSelectedCategory(selectedCategory === cat ? null : cat)}
                className={cn(
                  'flex items-center gap-1 px-2.5 py-1 text-xs rounded-full transition-colors cursor-pointer',
                  selectedCategory === cat
                    ? 'bg-tokyo-magenta/15 text-tokyo-magenta'
                    : 'bg-tokyo-bg-hl text-tokyo-comment hover:text-tokyo-fg'
                )}
              >
                <Tag className="w-2.5 h-2.5" />
                {cat}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Snippets List */}
      <div className="flex-1 overflow-y-auto p-3 space-y-2">
        {filteredSnippets.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-tokyo-comment py-12">
            <Terminal className="w-10 h-10 mb-3 opacity-40" />
            <p className="text-sm font-medium">No snippets yet</p>
            <p className="text-xs mt-1">Save frequently used commands for quick access</p>
          </div>
        ) : (
          filteredSnippets.map(snippet => (
            <div
              key={snippet.id}
              className="bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg p-3 hover:border-tokyo-selection transition-all duration-200 group"
            >
              <div className="flex items-start justify-between">
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium text-white">{snippet.name}</span>
                    {snippet.category && (
                      <span className="px-1.5 py-0.5 text-[10px] rounded-md bg-tokyo-magenta/10 text-tokyo-magenta font-medium">
                        {snippet.category}
                      </span>
                    )}
                  </div>
                  {snippet.description && (
                    <p className="text-xs text-tokyo-comment mt-0.5 line-clamp-1">{snippet.description}</p>
                  )}
                </div>
                <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity duration-200 ml-2">
                  <button
                    onClick={() => handleExecute(snippet.command)}
                    className="p-1.5 rounded-md hover:bg-tokyo-green/10 text-tokyo-green transition-colors cursor-pointer"
                    title="Execute in terminal"
                  >
                    <Play className="w-3.5 h-3.5" />
                  </button>
                  <button
                    onClick={() => handleCopy(snippet.id, snippet.command)}
                    className={cn(
                      'p-1.5 rounded-md transition-colors cursor-pointer',
                      copiedId === snippet.id
                        ? 'bg-tokyo-green/10 text-tokyo-green'
                        : 'hover:bg-tokyo-bg-hl text-tokyo-comment hover:text-tokyo-fg'
                    )}
                    title={copiedId === snippet.id ? 'Copied!' : 'Copy to clipboard'}
                  >
                    <Copy className="w-3.5 h-3.5" />
                  </button>
                  <button
                    onClick={() => { setEditSnippet(snippet); setShowAddDialog(true); }}
                    className="p-1.5 rounded-md hover:bg-tokyo-bg-hl text-tokyo-comment hover:text-tokyo-fg transition-colors cursor-pointer"
                    title="Edit"
                  >
                    <Edit3 className="w-3.5 h-3.5" />
                  </button>
                  <button
                    onClick={() => deleteSnippet(snippet.id)}
                    className="p-1.5 rounded-md hover:bg-tokyo-red/10 text-tokyo-comment hover:text-tokyo-red transition-colors cursor-pointer"
                    title="Delete"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
              <div className="mt-2 bg-tokyo-bg rounded-md px-3 py-2 font-mono text-xs text-tokyo-cyan overflow-x-auto border border-tokyo-bg-hl/50">
                {snippet.command}
              </div>
              {snippet.tags.length > 0 && (
                <div className="flex gap-1 mt-2">
                  {snippet.tags.map(tag => (
                    <span key={tag} className="px-1.5 py-0.5 text-[10px] rounded bg-tokyo-bg-hl text-tokyo-comment">
                      #{tag}
                    </span>
                  ))}
                </div>
              )}
            </div>
          ))
        )}
      </div>

      {/* Add/Edit Dialog */}
      {showAddDialog && (
        <AddSnippetDialog
          snippet={editSnippet}
          onClose={() => { setShowAddDialog(false); setEditSnippet(null); }}
        />
      )}
    </div>
  );
}
