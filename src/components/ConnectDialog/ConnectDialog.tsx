import { useState, useCallback, useEffect } from 'react';
import { X, Key, Lock, Eye, EyeOff, FolderOpen, FileKey, ArrowRightLeft } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useSessionStore } from '../../stores/sessionStore';
import { useServerStore, type Server } from '../../stores/serverStore';
import { safeInvoke } from '../../lib/tauri';

interface ConnectDialogProps {
  isOpen: boolean;
  server: Server | null;
  onClose: () => void;
  onConnected: (sessionId: string) => void;
}

function JumpHostBadge({ jumpHostId }: { jumpHostId: string }) {
  const { servers } = useServerStore();
  const jumpHost = servers.find((s) => s.id === jumpHostId);
  if (!jumpHost) return null;
  return (
    <div className="text-xs text-tokyo-yellow mt-1.5 flex items-center gap-1">
      <ArrowRightLeft className="w-3 h-3" />
      Via: {jumpHost.name} ({jumpHost.host}:{jumpHost.port})
    </div>
  );
}

export function ConnectDialog({ isOpen, server, onClose, onConnected }: ConnectDialogProps) {
  const { connectWithCredentials } = useSessionStore();
  const [password, setPassword] = useState('');
  const [showPassword, setShowPassword] = useState(false);
  const [isConnecting, setIsConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // SSH Key file state
  const [keyPath, setKeyPath] = useState<string | null>(null);
  const [keyContent, setKeyContent] = useState<string | null>(null);
  const [isLoadingKey, setIsLoadingKey] = useState(false);
  const [usingSavedCredentials, setUsingSavedCredentials] = useState(false);
  const [isLoadingCredentials, setIsLoadingCredentials] = useState(false);

  // Load saved credentials when dialog opens
  useEffect(() => {
    if (isOpen && server) {
      // Reset state first
      setPassword('');
      setShowPassword(false);
      setIsConnecting(false);
      setError(null);
      setKeyPath(null);
      setKeyContent(null);
      setIsLoadingKey(false);
      setUsingSavedCredentials(false);
      setIsLoadingCredentials(true);

      // Try to load saved credentials
      const loadCredentials = async () => {
        try {
          const result = await safeInvoke<{
            id: string;
            server_name: string;
            auth_type: string;
            credential: string;
            passphrase: string | null;
            key_path: string | null;
            created_at: number;
          } | null>('get_credential', { request: { serverName: server.name } });

          if (result.success && result.data) {
            console.log('[ConnectDialog] Found saved credentials for', server.name);
            const cred = result.data;

            if (cred.auth_type === 'password') {
              // Password auth - set the password
              setPassword(cred.credential);
              setUsingSavedCredentials(true);
            } else if (cred.auth_type === 'key' || cred.auth_type === 'key_with_passphrase') {
              // Key auth - set the key content and path
              setKeyContent(cred.credential);
              setKeyPath(cred.key_path);
              if (cred.passphrase) {
                setPassword(cred.passphrase);
              }
              setUsingSavedCredentials(true);
            }
          } else {
            console.log('[ConnectDialog] No saved credentials for', server.name);
          }
        } catch (err) {
          console.error('[ConnectDialog] Error loading credentials:', err);
        } finally {
          setIsLoadingCredentials(false);
        }
      };

      loadCredentials();
    }
  }, [isOpen, server?.id, server?.name]);

  // Browse for SSH key file
  const handleBrowseKey = useCallback(async () => {
    setIsLoadingKey(true);
    setError(null);

    try {
      const result = await safeInvoke<string | null>('pick_ssh_key_file');

      if (result.success && result.data) {
        setKeyPath(result.data);

        // Read the key file content
        const readResult = await safeInvoke<string>('read_ssh_key_file', { path: result.data });

        if (readResult.success) {
          setKeyContent(readResult.data);
          console.log('[ConnectDialog] SSH key loaded successfully');
        } else {
          setError(`Failed to read key file: ${readResult.error.message}`);
          setKeyPath(null);
        }
      }
    } catch (err) {
      console.error('[ConnectDialog] Error browsing for key:', err);
      setError(err instanceof Error ? err.message : 'Failed to browse for key file');
    } finally {
      setIsLoadingKey(false);
    }
  }, []);

  const handleConnect = useCallback(async () => {
    if (!server) return;

    const isKeyAuth = server.auth_type === 'key' || server.auth_type === 'key_with_passphrase';

    // Validate inputs
    if (isKeyAuth && !keyContent) {
      setError('Please select an SSH private key file');
      return;
    }

    if (!isKeyAuth && !password) {
      setError('Please enter a password');
      return;
    }

    console.log('[ConnectDialog] handleConnect called for server:', server.name);
    setIsConnecting(true);
    setError(null);

    try {
      console.log('[ConnectDialog] Calling connectWithCredentials:', {
        serverName: server.name,
        authType: isKeyAuth ? 'key' : 'password',
        hasPassword: !!password,
        hasKeyContent: !!keyContent,
      });

      // For key auth, pass the key content as credential and password as passphrase
      // For password auth, pass the password as credential
      const credential = isKeyAuth ? (keyContent || '') : password;
      const passphrase = isKeyAuth ? password : undefined;

      const session = await connectWithCredentials(
        server.name,
        isKeyAuth ? 'key' : 'password',
        credential,
        passphrase,
        80,
        24
      );

      console.log('[ConnectDialog] connectWithCredentials result:', session);

      if (session) {
        console.log('[ConnectDialog] Connection successful, session.id:', session.id);
        console.log('[ConnectDialog] Calling onConnected with sessionId:', session.id);
        onConnected(session.id);
        console.log('[ConnectDialog] Closing dialog');
        onClose();
      } else {
        console.error('[ConnectDialog] Connection failed: session is null');
        setError('Failed to connect. Check your credentials and try again.');
      }
    } catch (err) {
      console.error('[ConnectDialog] Connection error:', err);
      setError(err instanceof Error ? err.message : 'Connection failed');
    } finally {
      setIsConnecting(false);
    }
  }, [server, password, keyContent, connectWithCredentials, onConnected, onClose]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    const isKeyAuth = server?.auth_type === 'key' || server?.auth_type === 'key_with_passphrase';
    const canConnect = isKeyAuth ? !!keyContent : !!password;

    if (e.key === 'Enter' && !isConnecting && canConnect) {
      handleConnect();
    }
  }, [handleConnect, isConnecting, password, keyContent, server?.auth_type]);

  // Helper to get filename from path
  const getFileName = (path: string | null): string => {
    if (!path) return '';
    const parts = path.replace(/\\/g, '/').split('/');
    return parts[parts.length - 1] || path;
  };

  if (!isOpen || !server) return null;

  const isKeyAuth = server.auth_type === 'key' || server.auth_type === 'key_with_passphrase';

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/60"
        onClick={onClose}
      />

      {/* Dialog */}
      <div className="relative bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg shadow-xl w-full max-w-md mx-4">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-tokyo-bg-hl">
          <div className="flex items-center gap-3">
            {isKeyAuth ? (
              <Key className="w-5 h-5 text-tokyo-green" />
            ) : (
              <Lock className="w-5 h-5 text-tokyo-blue" />
            )}
            <h2 className="text-lg font-semibold text-white">Connect to Server</h2>
          </div>
          <button
            className="p-1 rounded-md text-tokyo-comment hover:text-white hover:bg-tokyo-bg-hl transition-colors"
            onClick={onClose}
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="p-4 space-y-4">
          {/* Server Info */}
          <div className="p-3 rounded-md bg-tokyo-bg border border-tokyo-bg-hl">
            <div className="text-sm text-tokyo-comment">Connecting to</div>
            <div className="text-white font-medium">{server.name}</div>
            <div className="text-sm text-tokyo-comment mt-1">
              {server.username}@{server.host}:{server.port}
            </div>
            {server.jump_host_id && (
              <JumpHostBadge jumpHostId={server.jump_host_id} />
            )}
            {server.agent_forwarding && (
              <div className="text-xs text-tokyo-cyan mt-1.5 flex items-center gap-1">
                <Key className="w-3 h-3" />
                Agent forwarding enabled
              </div>
            )}
          </div>

          {isLoadingCredentials && (
            <div className="p-3 rounded-md bg-tokyo-blue/10 border border-tokyo-blue/30 text-tokyo-blue text-sm">
              Loading saved credentials...
            </div>
          )}

          {usingSavedCredentials && !isLoadingCredentials && (
            <div className="p-3 rounded-md bg-tokyo-green/10 border border-tokyo-green/30 text-tokyo-green text-sm flex items-center gap-2">
              <Key className="w-4 h-4" />
              Using saved credentials. Click Connect to proceed.
            </div>
          )}

          {error && (
            <div className="p-3 rounded-md bg-red-900/20 border border-red-800/30 text-red-400 text-sm">
              {error}
            </div>
          )}

          {/* SSH Key File Browser (for key auth) */}
          {isKeyAuth && (
            <div>
              <label className="block text-sm font-medium text-tokyo-fg mb-1">
                SSH Private Key File
              </label>
              <div className="flex gap-2">
                <div
                  className={cn(
                    'flex-1 flex items-center gap-2 px-3 py-2 rounded-md',
                    'bg-tokyo-bg border border-tokyo-bg-hl',
                    'text-tokyo-fg',
                    keyPath ? '' : 'text-tokyo-comment'
                  )}
                >
                  <FileKey className="w-4 h-4 flex-shrink-0" />
                  <span className="truncate text-sm">
                    {keyPath ? getFileName(keyPath) : 'No key file selected'}
                  </span>
                </div>
                <button
                  type="button"
                  onClick={handleBrowseKey}
                  disabled={isLoadingKey || isConnecting}
                  className={cn(
                    'flex items-center gap-2 px-3 py-2 rounded-md',
                    'bg-tokyo-bg-hl text-tokyo-fg',
                    'hover:bg-tokyo-blue/20 hover:text-tokyo-blue',
                    'disabled:opacity-50 disabled:cursor-not-allowed',
                    'transition-colors'
                  )}
                >
                  <FolderOpen className="w-4 h-4" />
                  <span className="text-sm">{isLoadingKey ? 'Loading...' : 'Browse'}</span>
                </button>
              </div>
              {keyPath && (
                <p className="mt-1 text-xs text-tokyo-comment truncate" title={keyPath}>
                  {keyPath}
                </p>
              )}
            </div>
          )}

          {/* Password/Passphrase Input */}
          <div>
            <label className="block text-sm font-medium text-tokyo-fg mb-1">
              {isKeyAuth ? 'Key Passphrase' : 'Password'}
              {isKeyAuth && (
                <span className="text-tokyo-comment ml-2">(leave empty if key has no passphrase)</span>
              )}
            </label>
            <div className="relative">
              <input
                type={showPassword ? 'text' : 'password'}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder={isKeyAuth ? 'Enter passphrase (optional)...' : 'Enter password...'}
                autoFocus={!isKeyAuth}
                className={cn(
                  'w-full px-3 py-2 pr-10 rounded-md',
                  'bg-tokyo-bg border border-tokyo-bg-hl',
                  'text-tokyo-fg placeholder-tokyo-comment',
                  'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue'
                )}
              />
              <button
                type="button"
                onClick={() => setShowPassword(!showPassword)}
                className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-tokyo-comment hover:text-white"
              >
                {showPassword ? (
                  <EyeOff className="w-4 h-4" />
                ) : (
                  <Eye className="w-4 h-4" />
                )}
              </button>
            </div>
          </div>

          {/* Actions */}
          <div className="flex justify-end gap-3 pt-2">
            <button
              type="button"
              onClick={onClose}
              disabled={isConnecting}
              className={cn(
                'px-4 py-2 rounded-md',
                'bg-tokyo-bg-hl text-tokyo-fg',
                'hover:bg-tokyo-bg hover:text-white',
                'disabled:opacity-50 disabled:cursor-not-allowed',
                'transition-colors'
              )}
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={handleConnect}
              disabled={isConnecting || isLoadingKey || isLoadingCredentials || (isKeyAuth ? !keyContent : !password)}
              className={cn(
                'px-4 py-2 rounded-md',
                'bg-tokyo-blue text-white',
                'hover:bg-tokyo-blue/80',
                'disabled:opacity-50 disabled:cursor-not-allowed',
                'transition-colors'
              )}
            >
              {isConnecting ? 'Connecting...' : 'Connect'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
