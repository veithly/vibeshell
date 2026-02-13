import { useState, useCallback } from 'react';
import { X, Eye, EyeOff, FolderOpen, FileKey, Server, Key, Lock } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useServerStore } from '../../stores/serverStore';
import { safeInvoke } from '../../lib/tauri';
import { useNotificationStore } from '../../stores/notificationStore';

interface AddServerDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

export function AddServerDialog({ isOpen, onClose }: AddServerDialogProps) {
  const { addServer, loading, error, clearError } = useServerStore();
  const { success: notifySuccess } = useNotificationStore();

  const { servers } = useServerStore();

  const [formData, setFormData] = useState({
    name: '',
    host: '',
    port: 22,
    username: 'root',
    authType: 'password' as 'password' | 'key' | 'key_with_passphrase',
    password: '',
    keyPath: '',
    keyPassphrase: '',
    saveCredentials: true,
    jumpHostId: '',
    agentForwarding: false,
    postLoginCommand: '',
  });

  const [keyContent, setKeyContent] = useState<string | null>(null);
  const [isLoadingKey, setIsLoadingKey] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  const [showPassphrase, setShowPassphrase] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  // Browse for SSH key file
  const handleBrowseKey = useCallback(async () => {
    setIsLoadingKey(true);
    setLocalError(null);

    try {
      const result = await safeInvoke<string | null>('pick_ssh_key_file');

      if (result.success && result.data) {
        setFormData(prev => ({ ...prev, keyPath: result.data! }));

        // Read the key file content
        const readResult = await safeInvoke<string>('read_ssh_key_file', { path: result.data });

        if (readResult.success) {
          setKeyContent(readResult.data);
        } else {
          setLocalError(`Failed to read key file: ${readResult.error.message}`);
          setFormData(prev => ({ ...prev, keyPath: '' }));
        }
      }
    } catch (err) {
      console.error('Error browsing for key:', err);
      setLocalError(err instanceof Error ? err.message : 'Failed to browse for key file');
    } finally {
      setIsLoadingKey(false);
    }
  }, []);

  const handleSubmit = useCallback(async (e: React.FormEvent) => {
    e.preventDefault();
    setLocalError(null);

    // Validate basic fields
    if (!formData.name.trim()) {
      setLocalError('Server name is required');
      return;
    }
    if (!formData.host.trim()) {
      setLocalError('Host is required');
      return;
    }
    if (!formData.username.trim()) {
      setLocalError('Username is required');
      return;
    }

    // Validate auth-specific fields
    const isKeyAuth = formData.authType === 'key' || formData.authType === 'key_with_passphrase';

    if (isKeyAuth && !keyContent) {
      setLocalError('Please select an SSH private key file');
      return;
    }

    if (!isKeyAuth && !formData.password && formData.saveCredentials) {
      setLocalError('Password is required when saving credentials');
      return;
    }

    try {
      // First, save credentials if enabled
      let credentialId: string | undefined;

      if (formData.saveCredentials) {
        const credentialResult = await safeInvoke<string>('save_credential', {
          request: {
            serverName: formData.name.trim(),
            authType: formData.authType,
            credential: isKeyAuth ? keyContent : formData.password,
            passphrase: isKeyAuth && formData.authType === 'key_with_passphrase' ? formData.keyPassphrase : null,
            keyPath: isKeyAuth ? formData.keyPath : null,
          },
        });

        if (credentialResult.success) {
          credentialId = credentialResult.data;
        } else {
          console.warn('Failed to save credentials, will prompt on connect:', credentialResult.error.message);
        }
      }

      // Add the server
      await addServer({
        name: formData.name.trim(),
        host: formData.host.trim(),
        port: formData.port,
        username: formData.username.trim(),
        auth_type: formData.authType,
        credential_id: credentialId,
        group_id: undefined,
        tags: [],
        jump_host_id: formData.jumpHostId || undefined,
        agent_forwarding: formData.agentForwarding,
        post_login_command: formData.postLoginCommand.trim() || undefined,
      });

      notifySuccess('Server Added', `${formData.name} has been added successfully.`);

      // Reset form and close
      setFormData({
        name: '',
        host: '',
        port: 22,
        username: 'root',
        authType: 'password',
        password: '',
        keyPath: '',
        keyPassphrase: '',
        saveCredentials: true,
        jumpHostId: '',
        agentForwarding: false,
        postLoginCommand: '',
      });
      setKeyContent(null);
      onClose();
    } catch (err) {
      console.error('Failed to add server:', err);
    }
  }, [formData, keyContent, addServer, notifySuccess, onClose]);

  const handleChange = useCallback((field: string, value: string | number | boolean) => {
    setFormData(prev => ({ ...prev, [field]: value }));
    setLocalError(null);
    clearError();

    // Reset key content when changing auth type
    if (field === 'authType') {
      setKeyContent(null);
      setFormData(prev => ({ ...prev, keyPath: '', password: '', keyPassphrase: '' }));
    }
  }, [clearError]);

  // Helper to get filename from path
  const getFileName = (path: string): string => {
    if (!path) return '';
    const parts = path.replace(/\\/g, '/').split('/');
    return parts[parts.length - 1] || path;
  };

  if (!isOpen) return null;

  const displayError = localError || error;
  const isKeyAuth = formData.authType === 'key' || formData.authType === 'key_with_passphrase';

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/60"
        onClick={onClose}
      />

      {/* Dialog */}
      <div className="relative bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg shadow-xl w-full max-w-lg mx-4 max-h-[90vh] overflow-y-auto">
        {/* Header */}
        <div className="sticky top-0 bg-tokyo-bg-dark flex items-center justify-between px-4 py-3 border-b border-tokyo-bg-hl">
          <div className="flex items-center gap-2">
            <Server className="w-5 h-5 text-tokyo-blue" />
            <h2 className="text-lg font-semibold text-white">Add Server</h2>
          </div>
          <button
            className="p-1 rounded-md text-tokyo-comment hover:text-white hover:bg-tokyo-bg-hl transition-colors"
            onClick={onClose}
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Form */}
        <form onSubmit={handleSubmit} className="p-4 space-y-4">
          {displayError && (
            <div className="p-3 rounded-md bg-red-900/20 border border-red-800/30 text-red-400 text-sm">
              {displayError}
            </div>
          )}

          {/* Connection Section */}
          <div className="space-y-4">
            <h3 className="text-sm font-medium text-tokyo-comment uppercase tracking-wider">Connection</h3>

            {/* Name */}
            <div>
              <label className="block text-sm font-medium text-tokyo-fg mb-1">
                Display Name
              </label>
              <input
                type="text"
                value={formData.name}
                onChange={(e) => handleChange('name', e.target.value)}
                placeholder="My Server"
                autoFocus
                className={cn(
                  'w-full px-3 py-2 rounded-md',
                  'bg-tokyo-bg border border-tokyo-bg-hl',
                  'text-tokyo-fg placeholder-tokyo-comment',
                  'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue'
                )}
              />
            </div>

            {/* Host & Port */}
            <div className="grid grid-cols-3 gap-4">
              <div className="col-span-2">
                <label className="block text-sm font-medium text-tokyo-fg mb-1">
                  Host
                </label>
                <input
                  type="text"
                  value={formData.host}
                  onChange={(e) => handleChange('host', e.target.value)}
                  placeholder="192.168.1.1 or example.com"
                  className={cn(
                    'w-full px-3 py-2 rounded-md',
                    'bg-tokyo-bg border border-tokyo-bg-hl',
                    'text-tokyo-fg placeholder-tokyo-comment',
                    'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue'
                  )}
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-tokyo-fg mb-1">
                  Port
                </label>
                <input
                  type="number"
                  value={formData.port}
                  onChange={(e) => handleChange('port', parseInt(e.target.value) || 22)}
                  className={cn(
                    'w-full px-3 py-2 rounded-md',
                    'bg-tokyo-bg border border-tokyo-bg-hl',
                    'text-tokyo-fg',
                    'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue'
                  )}
                />
              </div>
            </div>

            {/* Username */}
            <div>
              <label className="block text-sm font-medium text-tokyo-fg mb-1">
                Username
              </label>
              <input
                type="text"
                value={formData.username}
                onChange={(e) => handleChange('username', e.target.value)}
                placeholder="root"
                className={cn(
                  'w-full px-3 py-2 rounded-md',
                  'bg-tokyo-bg border border-tokyo-bg-hl',
                  'text-tokyo-fg placeholder-tokyo-comment',
                  'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue'
                )}
              />
            </div>
          </div>

          {/* Authentication Section */}
          <div className="space-y-4 pt-2 border-t border-tokyo-bg-hl">
            <h3 className="text-sm font-medium text-tokyo-comment uppercase tracking-wider pt-2">Authentication</h3>

            {/* Auth Type */}
            <div>
              <label className="block text-sm font-medium text-tokyo-fg mb-1">
                Method
              </label>
              <div className="grid grid-cols-3 gap-2">
                <button
                  type="button"
                  onClick={() => handleChange('authType', 'password')}
                  className={cn(
                    'flex items-center justify-center gap-2 px-3 py-2 rounded-md border transition-colors',
                    formData.authType === 'password'
                      ? 'bg-tokyo-blue/20 border-tokyo-blue text-tokyo-blue'
                      : 'bg-tokyo-bg border-tokyo-bg-hl text-tokyo-fg hover:border-tokyo-comment'
                  )}
                >
                  <Lock className="w-4 h-4" />
                  <span className="text-sm">Password</span>
                </button>
                <button
                  type="button"
                  onClick={() => handleChange('authType', 'key')}
                  className={cn(
                    'flex items-center justify-center gap-2 px-3 py-2 rounded-md border transition-colors',
                    formData.authType === 'key'
                      ? 'bg-tokyo-green/20 border-tokyo-green text-tokyo-green'
                      : 'bg-tokyo-bg border-tokyo-bg-hl text-tokyo-fg hover:border-tokyo-comment'
                  )}
                >
                  <Key className="w-4 h-4" />
                  <span className="text-sm">SSH Key</span>
                </button>
                <button
                  type="button"
                  onClick={() => handleChange('authType', 'key_with_passphrase')}
                  className={cn(
                    'flex items-center justify-center gap-2 px-3 py-2 rounded-md border transition-colors',
                    formData.authType === 'key_with_passphrase'
                      ? 'bg-tokyo-magenta/20 border-tokyo-magenta text-tokyo-magenta'
                      : 'bg-tokyo-bg border-tokyo-bg-hl text-tokyo-fg hover:border-tokyo-comment'
                  )}
                >
                  <FileKey className="w-4 h-4" />
                  <span className="text-sm">Key+Pass</span>
                </button>
              </div>
            </div>

            {/* Password Auth */}
            {formData.authType === 'password' && (
              <div>
                <label className="block text-sm font-medium text-tokyo-fg mb-1">
                  Password
                </label>
                <div className="relative">
                  <input
                    type={showPassword ? 'text' : 'password'}
                    value={formData.password}
                    onChange={(e) => handleChange('password', e.target.value)}
                    placeholder="Enter password..."
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
                    {showPassword ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                  </button>
                </div>
              </div>
            )}

            {/* Key Auth */}
            {isKeyAuth && (
              <>
                <div>
                  <label className="block text-sm font-medium text-tokyo-fg mb-1">
                    Private Key File
                  </label>
                  <div className="flex gap-2">
                    <div
                      className={cn(
                        'flex-1 flex items-center gap-2 px-3 py-2 rounded-md',
                        'bg-tokyo-bg border border-tokyo-bg-hl',
                        'text-tokyo-fg',
                        formData.keyPath ? '' : 'text-tokyo-comment'
                      )}
                    >
                      <FileKey className="w-4 h-4 flex-shrink-0" />
                      <span className="truncate text-sm">
                        {formData.keyPath ? getFileName(formData.keyPath) : 'No key file selected'}
                      </span>
                    </div>
                    <button
                      type="button"
                      onClick={handleBrowseKey}
                      disabled={isLoadingKey || loading}
                      className={cn(
                        'flex items-center gap-2 px-3 py-2 rounded-md',
                        'bg-tokyo-bg-hl text-tokyo-fg',
                        'hover:bg-tokyo-green/20 hover:text-tokyo-green',
                        'disabled:opacity-50 disabled:cursor-not-allowed',
                        'transition-colors'
                      )}
                    >
                      <FolderOpen className="w-4 h-4" />
                      <span className="text-sm">{isLoadingKey ? 'Loading...' : 'Browse'}</span>
                    </button>
                  </div>
                  {formData.keyPath && (
                    <p className="mt-1 text-xs text-tokyo-comment truncate" title={formData.keyPath}>
                      {formData.keyPath}
                    </p>
                  )}
                </div>

                {formData.authType === 'key_with_passphrase' && (
                  <div>
                    <label className="block text-sm font-medium text-tokyo-fg mb-1">
                      Key Passphrase
                    </label>
                    <div className="relative">
                      <input
                        type={showPassphrase ? 'text' : 'password'}
                        value={formData.keyPassphrase}
                        onChange={(e) => handleChange('keyPassphrase', e.target.value)}
                        placeholder="Enter key passphrase..."
                        className={cn(
                          'w-full px-3 py-2 pr-10 rounded-md',
                          'bg-tokyo-bg border border-tokyo-bg-hl',
                          'text-tokyo-fg placeholder-tokyo-comment',
                          'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue'
                        )}
                      />
                      <button
                        type="button"
                        onClick={() => setShowPassphrase(!showPassphrase)}
                        className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-tokyo-comment hover:text-white"
                      >
                        {showPassphrase ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                      </button>
                    </div>
                  </div>
                )}
              </>
            )}

            {/* Save Credentials Option */}
            <div className="flex items-center gap-2">
              <input
                type="checkbox"
                id="saveCredentials"
                checked={formData.saveCredentials}
                onChange={(e) => handleChange('saveCredentials', e.target.checked)}
                className="w-4 h-4 rounded border-tokyo-bg-hl bg-tokyo-bg text-tokyo-blue focus:ring-tokyo-blue"
              />
              <label htmlFor="saveCredentials" className="text-sm text-tokyo-fg">
                Save credentials for quick connect
              </label>
            </div>
          </div>

          {/* Advanced Section */}
          <div className="space-y-4 pt-2 border-t border-tokyo-bg-hl">
            <h3 className="text-sm font-medium text-tokyo-comment uppercase tracking-wider pt-2">Advanced</h3>

            {/* Jump Host */}
            <div>
              <label className="block text-sm font-medium text-tokyo-fg mb-1">
                Jump Host (ProxyJump)
              </label>
              <select
                value={formData.jumpHostId}
                onChange={(e) => handleChange('jumpHostId', e.target.value)}
                className={cn(
                  'w-full px-3 py-2 rounded-md',
                  'bg-tokyo-bg border border-tokyo-bg-hl',
                  'text-tokyo-fg',
                  'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue'
                )}
              >
                <option value="">None (direct connection)</option>
                {servers
                  .filter(s => s.name !== formData.name)
                  .map(s => (
                    <option key={s.id} value={s.id}>{s.name} ({s.host}:{s.port})</option>
                  ))
                }
              </select>
              <p className="mt-1 text-xs text-tokyo-comment">
                Connect through another server (requires saved credentials on jump host)
              </p>
            </div>

            {/* Agent Forwarding */}
            <div className="flex items-center gap-2">
              <input
                type="checkbox"
                id="agentForwarding"
                checked={formData.agentForwarding}
                onChange={(e) => handleChange('agentForwarding', e.target.checked)}
                className="w-4 h-4 rounded border-tokyo-bg-hl bg-tokyo-bg text-tokyo-blue focus:ring-tokyo-blue"
              />
              <label htmlFor="agentForwarding" className="text-sm text-tokyo-fg">
                Enable SSH Agent Forwarding
              </label>
            </div>

            {/* Post-login Command */}
            <div>
              <label className="block text-sm font-medium text-tokyo-fg mb-1">
                Post-login Command
              </label>
              <textarea
                value={formData.postLoginCommand}
                onChange={(e) => handleChange('postLoginCommand', e.target.value)}
                placeholder="Command to execute after connecting (e.g., cd /app && source .env)"
                rows={2}
                className={cn(
                  'w-full px-3 py-2 rounded-md resize-none',
                  'bg-tokyo-bg border border-tokyo-bg-hl',
                  'text-tokyo-fg placeholder-tokyo-comment font-mono text-sm',
                  'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue'
                )}
              />
            </div>
          </div>

          {/* Actions */}
          <div className="flex justify-end gap-3 pt-4 border-t border-tokyo-bg-hl">
            <button
              type="button"
              onClick={onClose}
              className={cn(
                'px-4 py-2 rounded-md',
                'bg-tokyo-bg-hl text-tokyo-fg',
                'hover:bg-tokyo-bg hover:text-white',
                'transition-colors'
              )}
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={loading || isLoadingKey}
              className={cn(
                'px-4 py-2 rounded-md',
                'bg-tokyo-blue text-white',
                'hover:bg-tokyo-blue/80',
                'disabled:opacity-50 disabled:cursor-not-allowed',
                'transition-colors'
              )}
            >
              {loading ? 'Adding...' : 'Add Server'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
