import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import {
  X,
  ZoomIn,
  ZoomOut,
  RotateCw,
  Download,
  Maximize2,
  Minimize2,
  AlertCircle,
  Loader2,
  Copy,
  Check,
  FileText,
  FileImage,
  Edit3,
  Save,
} from 'lucide-react';
import { cn } from '../../lib/utils';
import { safeInvoke } from '../../lib/tauri';
import { FileIcon, getSyntaxLanguage, isTextPreviewable, isImagePreviewable } from './FileIcon';

/**
 * File content response from backend
 */
interface SftpFileContent {
  content: string;
  isBinary: boolean;
  size: number;
  truncated: boolean;
  mimeType: string;
}

interface PreviewModalProps {
  /** Whether the modal is open */
  isOpen: boolean;
  /** File path to preview */
  filePath: string;
  /** File name for display */
  fileName: string;
  /** File size in bytes */
  fileSize: number;
  /** Session ID for SFTP operations */
  sessionId: string;
  /** Callback when modal is closed */
  onClose: () => void;
  /** Callback to download the file */
  onDownload?: () => void;
  /** Callback to save edited content */
  onSave?: (content: string) => Promise<void>;
}

/**
 * Format file size in human readable format
 */
function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`;
}

/**
 * Simple syntax highlighting for code
 * This is a lightweight implementation - for production, consider using highlight.js or prism
 */
function highlightCode(code: string, language: string): string {
  // Keywords for various languages
  const keywords: Record<string, string[]> = {
    javascript: ['const', 'let', 'var', 'function', 'return', 'if', 'else', 'for', 'while', 'class', 'import', 'export', 'from', 'async', 'await', 'try', 'catch', 'throw', 'new', 'this', 'null', 'undefined', 'true', 'false'],
    typescript: ['const', 'let', 'var', 'function', 'return', 'if', 'else', 'for', 'while', 'class', 'import', 'export', 'from', 'async', 'await', 'try', 'catch', 'throw', 'new', 'this', 'null', 'undefined', 'true', 'false', 'interface', 'type', 'enum', 'implements', 'extends', 'public', 'private', 'protected'],
    python: ['def', 'class', 'return', 'if', 'elif', 'else', 'for', 'while', 'import', 'from', 'as', 'try', 'except', 'finally', 'with', 'lambda', 'yield', 'async', 'await', 'True', 'False', 'None', 'and', 'or', 'not', 'in', 'is', 'pass', 'break', 'continue'],
    rust: ['fn', 'let', 'mut', 'const', 'struct', 'enum', 'impl', 'trait', 'pub', 'use', 'mod', 'if', 'else', 'match', 'for', 'while', 'loop', 'return', 'async', 'await', 'self', 'Self', 'true', 'false', 'Some', 'None', 'Ok', 'Err'],
    go: ['func', 'var', 'const', 'type', 'struct', 'interface', 'package', 'import', 'return', 'if', 'else', 'for', 'range', 'switch', 'case', 'default', 'go', 'chan', 'select', 'defer', 'make', 'new', 'nil', 'true', 'false'],
    json: [],
    bash: ['if', 'then', 'else', 'fi', 'for', 'do', 'done', 'while', 'case', 'esac', 'function', 'return', 'exit', 'echo', 'export', 'local', 'readonly', 'shift', 'true', 'false'],
  };

  const langKeywords = keywords[language] || keywords['javascript'] || [];

  // Escape HTML
  let escaped = code
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');

  // Highlight strings (single and double quotes)
  escaped = escaped.replace(
    /(["'`])(?:(?!\1)[^\\]|\\.)*?\1/g,
    '<span class="text-tokyo-green">$&</span>'
  );

  // Highlight comments (// and #)
  escaped = escaped.replace(
    /(\/\/.*$|#.*$)/gm,
    '<span class="text-tokyo-comment">$1</span>'
  );

  // Highlight numbers
  escaped = escaped.replace(
    /\b(\d+\.?\d*)\b/g,
    '<span class="text-tokyo-orange">$1</span>'
  );

  // Highlight keywords
  if (langKeywords.length > 0) {
    const keywordRegex = new RegExp(`\\b(${langKeywords.join('|')})\\b`, 'g');
    escaped = escaped.replace(
      keywordRegex,
      '<span class="text-tokyo-magenta">$1</span>'
    );
  }

  return escaped;
}

/**
 * Preview modal for text and image files
 */
export function PreviewModal({
  isOpen,
  filePath,
  fileName,
  fileSize,
  sessionId,
  onClose,
  onDownload,
  onSave,
}: PreviewModalProps) {
  const [content, setContent] = useState<SftpFileContent | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [copied, setCopied] = useState(false);

  // Edit mode state
  const [isEditing, setIsEditing] = useState(false);
  const [editContent, setEditContent] = useState('');
  const [isSaving, setIsSaving] = useState(false);

  // Image preview state
  const [zoom, setZoom] = useState(1);
  const [rotation, setRotation] = useState(0);
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });

  const imageRef = useRef<HTMLImageElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const isText = useMemo(() => isTextPreviewable(fileName), [fileName]);
  const isImage = useMemo(() => isImagePreviewable(fileName), [fileName]);
  const syntaxLanguage = useMemo(() => getSyntaxLanguage(fileName), [fileName]);

  // Load file content
  useEffect(() => {
    if (!isOpen || !sessionId || !filePath) return;

    const loadContent = async () => {
      setIsLoading(true);
      setError(null);
      setContent(null);
      setZoom(1);
      setRotation(0);
      setPosition({ x: 0, y: 0 });
      setIsEditing(false);
      setEditContent('');

      try {
        const result = await safeInvoke<SftpFileContent>('sftp_read_file', {
          request: {
            sessionId,
            path: filePath,
            asBinary: isImage,
          },
        });

        if (result.success) {
          setContent(result.data);
        } else {
          throw new Error(result.error.message);
        }
      } catch (err) {
        console.error('[PreviewModal] Failed to load file:', err);
        setError(err instanceof Error ? err.message : 'Failed to load file');
      } finally {
        setIsLoading(false);
      }
    };

    loadContent();
  }, [isOpen, sessionId, filePath, isImage]);

  // Handle keyboard shortcuts
  useEffect(() => {
    if (!isOpen) return;

    const hasUnsavedChanges = isEditing && editContent !== (content?.content ?? '');

    const handleKeyDown = (e: KeyboardEvent) => {
      // Ctrl+S / Cmd+S to save when editing
      if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault();
        if (isEditing && hasUnsavedChanges && !isSaving && onSave) {
          setIsSaving(true);
          onSave(editContent).then(() => {
            setContent((prev) => prev ? { ...prev, content: editContent } : prev);
            setIsEditing(false);
          }).catch((err) => {
            console.error('[PreviewModal] Failed to save file:', err);
          }).finally(() => {
            setIsSaving(false);
          });
        }
        return;
      }

      if (e.key === 'Escape') {
        if (isFullscreen) {
          setIsFullscreen(false);
        } else if (isEditing) {
          if (hasUnsavedChanges) {
            const confirmed = window.confirm('You have unsaved changes. Discard and close?');
            if (!confirmed) return;
          }
          setIsEditing(false);
          onClose();
        } else {
          onClose();
        }
        return;
      }

      // Skip image shortcuts when editing (user is typing in textarea)
      if (isEditing) return;

      if (e.key === '+' || e.key === '=') {
        if (isImage) setZoom((z) => Math.min(z + 0.25, 5));
      } else if (e.key === '-') {
        if (isImage) setZoom((z) => Math.max(z - 0.25, 0.25));
      } else if (e.key === '0') {
        if (isImage) {
          setZoom(1);
          setPosition({ x: 0, y: 0 });
        }
      } else if (e.key === 'r' || e.key === 'R') {
        if (isImage) setRotation((r) => (r + 90) % 360);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, isFullscreen, isImage, isEditing, editContent, isSaving, content, onSave, onClose]);

  // Copy content to clipboard
  const handleCopy = useCallback(async () => {
    if (!content || content.isBinary) return;

    try {
      await navigator.clipboard.writeText(content.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
    }
  }, [content]);

  // Edit mode handlers
  const hasChanges = isEditing && editContent !== (content?.content ?? '');

  const handleClose = useCallback(() => {
    if (isEditing && hasChanges) {
      const confirmed = window.confirm('You have unsaved changes. Discard and close?');
      if (!confirmed) return;
    }
    setIsEditing(false);
    onClose();
  }, [isEditing, hasChanges, onClose]);

  const handleStartEdit = useCallback(() => {
    if (!content || content.isBinary || content.truncated) return;
    setEditContent(content.content);
    setIsEditing(true);
  }, [content]);

  const handleCancelEdit = useCallback(() => {
    if (hasChanges) {
      const confirmed = window.confirm('You have unsaved changes. Discard?');
      if (!confirmed) return;
    }
    setIsEditing(false);
    setEditContent('');
  }, [hasChanges]);

  const handleSave = useCallback(async () => {
    if (!onSave || !hasChanges || isSaving) return;
    setIsSaving(true);
    try {
      await onSave(editContent);
      setContent((prev) => prev ? { ...prev, content: editContent } : prev);
      setIsEditing(false);
    } catch (err) {
      console.error('[PreviewModal] Failed to save file:', err);
    } finally {
      setIsSaving(false);
    }
  }, [onSave, hasChanges, isSaving, editContent]);

  // Image zoom handlers
  const handleZoomIn = useCallback(() => {
    setZoom((z) => Math.min(z + 0.25, 5));
  }, []);

  const handleZoomOut = useCallback(() => {
    setZoom((z) => Math.max(z - 0.25, 0.25));
  }, []);

  const handleResetView = useCallback(() => {
    setZoom(1);
    setRotation(0);
    setPosition({ x: 0, y: 0 });
  }, []);

  const handleRotate = useCallback(() => {
    setRotation((r) => (r + 90) % 360);
  }, []);

  // Image drag handlers
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (zoom <= 1) return;
    setIsDragging(true);
    setDragStart({ x: e.clientX - position.x, y: e.clientY - position.y });
  }, [zoom, position]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!isDragging) return;
    setPosition({
      x: e.clientX - dragStart.x,
      y: e.clientY - dragStart.y,
    });
  }, [isDragging, dragStart]);

  const handleMouseUp = useCallback(() => {
    setIsDragging(false);
  }, []);

  // Mouse wheel zoom
  const handleWheel = useCallback((e: React.WheelEvent) => {
    if (!isImage) return;
    e.preventDefault();
    const delta = e.deltaY > 0 ? -0.1 : 0.1;
    setZoom((z) => Math.max(0.25, Math.min(5, z + delta)));
  }, [isImage]);

  if (!isOpen) return null;

  const imageDataUrl = content?.isBinary
    ? `data:${content.mimeType};base64,${content.content}`
    : null;

  return (
    <div
      className={cn(
        'fixed z-50 flex items-center justify-center',
        isFullscreen ? 'inset-0' : 'inset-0'
      )}
    >
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/80"
        onClick={handleClose}
      />

      {/* Modal */}
      <div
        className={cn(
          'relative bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg shadow-xl',
          'flex flex-col overflow-hidden',
          isFullscreen
            ? 'w-full h-full rounded-none'
            : 'w-[90vw] h-[85vh] max-w-6xl'
        )}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-tokyo-bg-hl bg-tokyo-bg">
          <div className="flex items-center gap-3 min-w-0">
            <FileIcon filename={fileName} isDirectory={false} size="lg" />
            <div className="min-w-0">
              <h2 className="text-lg font-semibold text-white truncate">{fileName}</h2>
              <div className="flex items-center gap-2 text-sm text-tokyo-comment">
                <span>{formatFileSize(fileSize)}</span>
                {content?.truncated && (
                  <span className="text-tokyo-yellow">(truncated)</span>
                )}
                {isText && <FileText className="w-3 h-3" />}
                {isImage && <FileImage className="w-3 h-3" />}
              </div>
            </div>
          </div>

          <div className="flex items-center gap-1">
            {/* Text-specific actions (hidden when editing) */}
            {isText && content && !content.isBinary && !isEditing && (
              <>
                <button
                  onClick={handleCopy}
                  className={cn(
                    'p-2 rounded-md transition-colors',
                    'hover:bg-tokyo-bg-hl text-tokyo-comment hover:text-white'
                  )}
                  title="Copy content"
                >
                  {copied ? (
                    <Check className="w-5 h-5 text-tokyo-green" />
                  ) : (
                    <Copy className="w-5 h-5" />
                  )}
                </button>
                {onSave && !content.truncated && (
                  <button
                    onClick={handleStartEdit}
                    className={cn(
                      'p-2 rounded-md transition-colors',
                      'hover:bg-tokyo-bg-hl text-tokyo-comment hover:text-white'
                    )}
                    title="Edit file"
                  >
                    <Edit3 className="w-5 h-5" />
                  </button>
                )}
              </>
            )}

            {/* Edit mode actions */}
            {isEditing && (
              <>
                <button
                  onClick={handleCancelEdit}
                  className={cn(
                    'px-3 py-1 rounded-md text-sm transition-colors',
                    'hover:bg-tokyo-bg-hl text-tokyo-comment hover:text-white'
                  )}
                >
                  Cancel
                </button>
                <button
                  onClick={handleSave}
                  disabled={!hasChanges || isSaving}
                  className={cn(
                    'bg-tokyo-blue text-white rounded px-3 py-1 text-sm',
                    'flex items-center gap-1.5',
                    'disabled:opacity-50 disabled:cursor-not-allowed',
                    'hover:bg-tokyo-blue/80 transition-colors'
                  )}
                >
                  {isSaving ? (
                    <Loader2 className="w-4 h-4 animate-spin" />
                  ) : (
                    <Save className="w-4 h-4" />
                  )}
                  Save
                </button>
              </>
            )}

            {/* Image-specific actions */}
            {isImage && content && (
              <>
                <button
                  onClick={handleZoomOut}
                  disabled={zoom <= 0.25}
                  className={cn(
                    'p-2 rounded-md transition-colors',
                    'hover:bg-tokyo-bg-hl text-tokyo-comment hover:text-white',
                    'disabled:opacity-50 disabled:cursor-not-allowed'
                  )}
                  title="Zoom out (-)"
                >
                  <ZoomOut className="w-5 h-5" />
                </button>
                <span className="px-2 text-sm text-tokyo-fg min-w-[4rem] text-center">
                  {Math.round(zoom * 100)}%
                </span>
                <button
                  onClick={handleZoomIn}
                  disabled={zoom >= 5}
                  className={cn(
                    'p-2 rounded-md transition-colors',
                    'hover:bg-tokyo-bg-hl text-tokyo-comment hover:text-white',
                    'disabled:opacity-50 disabled:cursor-not-allowed'
                  )}
                  title="Zoom in (+)"
                >
                  <ZoomIn className="w-5 h-5" />
                </button>
                <button
                  onClick={handleRotate}
                  className={cn(
                    'p-2 rounded-md transition-colors',
                    'hover:bg-tokyo-bg-hl text-tokyo-comment hover:text-white'
                  )}
                  title="Rotate (R)"
                >
                  <RotateCw className="w-5 h-5" />
                </button>
                <button
                  onClick={handleResetView}
                  className={cn(
                    'p-2 rounded-md transition-colors',
                    'hover:bg-tokyo-bg-hl text-tokyo-comment hover:text-white'
                  )}
                  title="Reset view (0)"
                >
                  Reset
                </button>
              </>
            )}

            <div className="w-px h-6 bg-tokyo-bg-hl mx-1" />

            {/* Download button */}
            {onDownload && (
              <button
                onClick={onDownload}
                className={cn(
                  'p-2 rounded-md transition-colors',
                  'hover:bg-tokyo-bg-hl text-tokyo-comment hover:text-white'
                )}
                title="Download file"
              >
                <Download className="w-5 h-5" />
              </button>
            )}

            {/* Fullscreen toggle */}
            <button
              onClick={() => setIsFullscreen(!isFullscreen)}
              className={cn(
                'p-2 rounded-md transition-colors',
                'hover:bg-tokyo-bg-hl text-tokyo-comment hover:text-white'
              )}
              title={isFullscreen ? 'Exit fullscreen' : 'Fullscreen'}
            >
              {isFullscreen ? (
                <Minimize2 className="w-5 h-5" />
              ) : (
                <Maximize2 className="w-5 h-5" />
              )}
            </button>

            {/* Close button */}
            <button
              onClick={handleClose}
              className={cn(
                'p-2 rounded-md transition-colors',
                'hover:bg-tokyo-bg-hl text-tokyo-comment hover:text-white'
              )}
              title="Close (Esc)"
            >
              <X className="w-5 h-5" />
            </button>
          </div>
        </div>

        {/* Content */}
        <div
          ref={containerRef}
          className="flex-1 overflow-hidden relative"
          onWheel={handleWheel}
        >
          {isLoading && (
            <div className="absolute inset-0 flex items-center justify-center bg-tokyo-bg-dark">
              <Loader2 className="w-8 h-8 text-tokyo-blue animate-spin" />
            </div>
          )}

          {error && (
            <div className="absolute inset-0 flex flex-col items-center justify-center bg-tokyo-bg-dark text-tokyo-red">
              <AlertCircle className="w-12 h-12 mb-4" />
              <p className="text-lg font-medium">Failed to load preview</p>
              <p className="text-sm text-tokyo-comment mt-2">{error}</p>
            </div>
          )}

          {content && !isLoading && !error && (
            <>
              {/* Text preview / edit */}
              {isText && !content.isBinary && (
                <div className="h-full overflow-hidden bg-tokyo-bg-dark">
                  {isEditing ? (
                    <textarea
                      value={editContent}
                      onChange={(e) => setEditContent(e.target.value)}
                      className="w-full h-full bg-tokyo-bg-dark text-tokyo-fg font-mono text-sm p-4 resize-none outline-none border-none"
                      spellCheck={false}
                    />
                  ) : (
                    <div className="h-full overflow-auto p-4">
                      <pre className="text-sm font-mono text-tokyo-fg whitespace-pre-wrap break-words">
                        {syntaxLanguage !== 'text' ? (
                          <code
                            dangerouslySetInnerHTML={{
                              __html: highlightCode(content.content, syntaxLanguage),
                            }}
                          />
                        ) : (
                          <code>{content.content}</code>
                        )}
                      </pre>
                      {content.truncated && (
                        <div className="mt-4 p-3 bg-tokyo-yellow/10 border border-tokyo-yellow/30 rounded-md text-tokyo-yellow text-sm">
                          File content truncated. Download the file to see the full content.
                        </div>
                      )}
                    </div>
                  )}
                </div>
              )}

              {/* Image preview */}
              {isImage && imageDataUrl && (
                <div
                  className={cn(
                    'h-full flex items-center justify-center bg-[#1a1a2e]',
                    'overflow-hidden',
                    zoom > 1 ? 'cursor-grab' : 'cursor-default',
                    isDragging && 'cursor-grabbing'
                  )}
                  onMouseDown={handleMouseDown}
                  onMouseMove={handleMouseMove}
                  onMouseUp={handleMouseUp}
                  onMouseLeave={handleMouseUp}
                >
                  {/* Checkerboard background for transparent images */}
                  <div
                    className="absolute inset-0 opacity-10"
                    style={{
                      backgroundImage: `
                        linear-gradient(45deg, #333 25%, transparent 25%),
                        linear-gradient(-45deg, #333 25%, transparent 25%),
                        linear-gradient(45deg, transparent 75%, #333 75%),
                        linear-gradient(-45deg, transparent 75%, #333 75%)
                      `,
                      backgroundSize: '20px 20px',
                      backgroundPosition: '0 0, 0 10px, 10px -10px, -10px 0px',
                    }}
                  />
                  <img
                    ref={imageRef}
                    src={imageDataUrl}
                    alt={fileName}
                    className="max-w-none select-none"
                    style={{
                      transform: `translate(${position.x}px, ${position.y}px) scale(${zoom}) rotate(${rotation}deg)`,
                      transition: isDragging ? 'none' : 'transform 0.2s ease-out',
                    }}
                    draggable={false}
                  />
                </div>
              )}

              {/* Unsupported file type */}
              {!isText && !isImage && (
                <div className="h-full flex flex-col items-center justify-center text-tokyo-comment">
                  <FileIcon filename={fileName} isDirectory={false} size="lg" className="w-16 h-16 mb-4" />
                  <p className="text-lg">Preview not available for this file type</p>
                  <p className="text-sm mt-2">Download the file to view it locally</p>
                </div>
              )}
            </>
          )}
        </div>

        {/* Footer with file info */}
        <div className="px-4 py-2 border-t border-tokyo-bg-hl bg-tokyo-bg text-xs text-tokyo-comment">
          <div className="flex items-center justify-between">
            <span className="truncate">{filePath}</span>
            {isImage && content && (
              <span className="ml-4">
                Use scroll to zoom, drag to pan when zoomed
              </span>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

export type { PreviewModalProps };
