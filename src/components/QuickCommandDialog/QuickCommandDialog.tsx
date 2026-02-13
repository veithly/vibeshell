import { useState, useCallback, useRef, useEffect } from 'react';
import { X, Terminal, Play, Loader2, Copy, Check } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useSessionStore } from '../../stores/sessionStore';
import { safeInvoke } from '../../lib/tauri';

interface QuickCommandDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

/**
 * Dialog for running quick SSH commands on connected sessions
 */
export function QuickCommandDialog({ isOpen, onClose }: QuickCommandDialogProps) {
  const { sessions, activeSessionId } = useSessionStore();
  const [command, setCommand] = useState('');
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [output, setOutput] = useState<string>('');
  const [isRunning, setIsRunning] = useState(false);
  const [copied, setCopied] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const outputRef = useRef<HTMLPreElement>(null);

  // Get connected sessions only
  const connectedSessions = sessions.filter((s) => s.state === 'connected');

  // Initialize selected session to active session or first connected
  useEffect(() => {
    if (isOpen) {
      if (activeSessionId && connectedSessions.find((s) => s.id === activeSessionId)) {
        setSelectedSessionId(activeSessionId);
      } else if (connectedSessions.length > 0) {
        setSelectedSessionId(connectedSessions[0].id);
      } else {
        setSelectedSessionId(null);
      }
      // Focus input when dialog opens
      setTimeout(() => inputRef.current?.focus(), 100);
    }
  }, [isOpen, activeSessionId, connectedSessions.length]);

  // Auto-scroll output to bottom
  useEffect(() => {
    if (outputRef.current) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  }, [output]);

  const handleRunCommand = useCallback(async () => {
    if (!command.trim() || !selectedSessionId || isRunning) return;

    setIsRunning(true);
    setOutput('');

    try {
      // Execute command through Tauri backend
      const result = await safeInvoke<{ output: string; exitCode: number }>('session_exec_command', {
        sessionId: selectedSessionId,
        command: command.trim(),
      });

      if (result.success) {
        setOutput(result.data.output || '(No output)');
        if (result.data.exitCode !== 0) {
          setOutput((prev) => prev + `\n\n[Exit code: ${result.data.exitCode}]`);
        }
      } else {
        // Fallback: Send command directly to terminal input
        const { sendInput } = useSessionStore.getState();
        await sendInput(selectedSessionId, command.trim() + '\n');
        setOutput('Command sent to terminal. Check the terminal for output.');
      }
    } catch (error) {
      console.error('Failed to execute command:', error);
      // Fallback to sending to terminal
      const { sendInput } = useSessionStore.getState();
      await sendInput(selectedSessionId, command.trim() + '\n');
      setOutput('Command sent to terminal. Check the terminal for output.');
    } finally {
      setIsRunning(false);
    }
  }, [command, selectedSessionId, isRunning]);

  const handleCopyOutput = useCallback(() => {
    if (output) {
      navigator.clipboard.writeText(output);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  }, [output]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleRunCommand();
    }
    if (e.key === 'Escape') {
      onClose();
    }
  }, [handleRunCommand, onClose]);

  const handleClear = useCallback(() => {
    setCommand('');
    setOutput('');
    inputRef.current?.focus();
  }, []);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/60"
        onClick={onClose}
      />

      {/* Dialog */}
      <div className="relative bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg shadow-xl w-full max-w-2xl mx-4 max-h-[80vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-tokyo-bg-hl">
          <div className="flex items-center gap-2">
            <Terminal className="w-5 h-5 text-tokyo-blue" />
            <h2 className="text-lg font-semibold text-white">Quick Command</h2>
          </div>
          <button
            className="p-1 rounded-md text-tokyo-comment hover:text-white hover:bg-tokyo-bg-hl transition-colors"
            onClick={onClose}
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="p-4 space-y-4 flex-1 overflow-hidden flex flex-col">
          {/* No connected sessions warning */}
          {connectedSessions.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-8 text-center">
              <Terminal className="w-12 h-12 text-tokyo-comment/50 mb-4" />
              <p className="text-tokyo-fg mb-2">No connected sessions</p>
              <p className="text-sm text-tokyo-comment">
                Connect to a server first to run quick commands.
              </p>
            </div>
          ) : (
            <>
              {/* Session Selector */}
              <div>
                <label className="block text-sm font-medium text-tokyo-fg mb-1">
                  Target Session
                </label>
                <select
                  value={selectedSessionId || ''}
                  onChange={(e) => setSelectedSessionId(e.target.value)}
                  className={cn(
                    'w-full px-3 py-2 rounded-md',
                    'bg-tokyo-bg border border-tokyo-bg-hl',
                    'text-tokyo-fg',
                    'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue'
                  )}
                >
                  {connectedSessions.map((session) => (
                    <option key={session.id} value={session.id}>
                      {session.serverName}
                    </option>
                  ))}
                </select>
              </div>

              {/* Command Input */}
              <div>
                <label className="block text-sm font-medium text-tokyo-fg mb-1">
                  Command
                </label>
                <div className="flex gap-2">
                  <input
                    ref={inputRef}
                    type="text"
                    value={command}
                    onChange={(e) => setCommand(e.target.value)}
                    onKeyDown={handleKeyDown}
                    placeholder="ls -la, df -h, ps aux, etc."
                    className={cn(
                      'flex-1 px-3 py-2 rounded-md',
                      'bg-tokyo-bg border border-tokyo-bg-hl',
                      'text-tokyo-fg placeholder-tokyo-comment',
                      'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue',
                      'font-mono text-sm'
                    )}
                  />
                  <button
                    onClick={handleRunCommand}
                    disabled={!command.trim() || isRunning}
                    className={cn(
                      'px-4 py-2 rounded-md',
                      'bg-tokyo-green text-white',
                      'hover:bg-tokyo-green/80',
                      'disabled:opacity-50 disabled:cursor-not-allowed',
                      'transition-colors flex items-center gap-2'
                    )}
                  >
                    {isRunning ? (
                      <Loader2 className="w-4 h-4 animate-spin" />
                    ) : (
                      <Play className="w-4 h-4" />
                    )}
                    Run
                  </button>
                </div>
                <p className="mt-1 text-xs text-tokyo-comment">
                  Press Enter to run, Escape to close
                </p>
              </div>

              {/* Output */}
              <div className="flex-1 flex flex-col min-h-0">
                <div className="flex items-center justify-between mb-1">
                  <label className="text-sm font-medium text-tokyo-fg">
                    Output
                  </label>
                  {output && (
                    <button
                      onClick={handleCopyOutput}
                      className={cn(
                        'flex items-center gap-1 px-2 py-1 rounded text-xs',
                        'text-tokyo-comment hover:text-tokyo-fg',
                        'hover:bg-tokyo-bg-hl transition-colors'
                      )}
                    >
                      {copied ? (
                        <>
                          <Check className="w-3 h-3 text-tokyo-green" />
                          Copied
                        </>
                      ) : (
                        <>
                          <Copy className="w-3 h-3" />
                          Copy
                        </>
                      )}
                    </button>
                  )}
                </div>
                <pre
                  ref={outputRef}
                  className={cn(
                    'flex-1 p-3 rounded-md overflow-auto',
                    'bg-tokyo-bg border border-tokyo-bg-hl',
                    'text-sm font-mono text-tokyo-fg',
                    'min-h-[150px] max-h-[300px]',
                    'whitespace-pre-wrap break-all'
                  )}
                >
                  {output || (
                    <span className="text-tokyo-comment">
                      Output will appear here...
                    </span>
                  )}
                </pre>
              </div>
            </>
          )}
        </div>

        {/* Footer */}
        <div className="flex justify-between px-4 py-3 border-t border-tokyo-bg-hl">
          <button
            onClick={handleClear}
            className={cn(
              'px-4 py-2 rounded-md',
              'bg-tokyo-bg-hl text-tokyo-fg',
              'hover:bg-tokyo-bg hover:text-white',
              'transition-colors'
            )}
          >
            Clear
          </button>
          <button
            onClick={onClose}
            className={cn(
              'px-4 py-2 rounded-md',
              'bg-tokyo-bg-hl text-tokyo-fg',
              'hover:bg-tokyo-bg hover:text-white',
              'transition-colors'
            )}
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
