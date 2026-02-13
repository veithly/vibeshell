import { useState, useCallback, useEffect } from 'react';
import { X, FolderOpen } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useServerStore, type Server, type AuthType } from '../../stores/serverStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { safeInvoke } from '../../lib/tauri';

interface EditServerDialogProps {
  isOpen: boolean;
  server: Server | null;
  onClose: () => void;
}

/**
 * Dialog for editing an existing server configuration
 */
export function EditServerDialog({ isOpen, server, onClose }: EditServerDialogProps) {
  const { updateServer, loading, error, clearError } = useServerStore();
  const { success: notifySuccess } = useNotificationStore();

  const { servers } = useServerStore();

  const [formData, setFormData] = useState({
    name: '',
    host: '',
    port: 22,
    username: 'root',
    authType: 'password' as AuthType,
    privateKeyPath: '',
    jumpHostId: '',
    agentForwarding: false,
    postLoginCommand: '',
  });

  const [localError, setLocalError] = useState<string | null>(null);

  // Initialize form data when server changes
  useEffect(() => {
    if (server) {
      setFormData({
        name: server.name,
        host: server.host,
        port: server.port,
        username: server.username,
        authType: server.auth_type,
        privateKeyPath: '',
        jumpHostId: server.jump_host_id || '',
        agentForwarding: server.agent_forwarding || false,
        postLoginCommand: server.post_login_command || '',
      });
    }
  }, [server]);

  // Reset form when dialog closes
  useEffect(() => {
    if (!isOpen) {
      setLocalError(null);
      clearError();
    }
  }, [isOpen, clearError]);

  const handleSubmit = useCallback(async (e: React.FormEvent) => {
    e.preventDefault();
    setLocalError(null);

    if (!server) return;

    // Validate
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

    try {
      await updateServer(server.id, {
        name: formData.name.trim(),
        host: formData.host.trim(),
        port: formData.port,
        username: formData.username.trim(),
        auth_type: formData.authType,
        jump_host_id: formData.jumpHostId || undefined,
        agent_forwarding: formData.agentForwarding,
        post_login_command: formData.postLoginCommand.trim() || undefined,
      });

      notifySuccess('Server Updated', `${formData.name} has been updated successfully.`);
      onClose();
    } catch (err) {
      console.error('Failed to update server:', err);
    }
  }, [server, formData, updateServer, notifySuccess, onClose]);

  const handleChange = useCallback((field: string, value: string | number) => {
    setFormData(prev => ({ ...prev, [field]: value }));
    setLocalError(null);
    clearError();
  }, [clearError]);

  const handleBrowseKeyFile = useCallback(async () => {
    try {
      const result = await safeInvoke<string | null>('pick_ssh_key_file');
      if (result.success && result.data) {
        handleChange('privateKeyPath', result.data);
      }
    } catch (error) {
      console.error('Failed to open file dialog:', error);
    }
  }, [handleChange]);

  if (!isOpen || !server) return null;

  const displayError = localError || error;

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
          <h2 className="text-lg font-semibold text-white">Edit Server</h2>
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

          {/* Name */}
          <div>
            <label className="block text-sm font-medium text-tokyo-fg mb-1">
              Name
            </label>
            <input
              type="text"
              value={formData.name}
              onChange={(e) => handleChange('name', e.target.value)}
              placeholder="My Server"
              className={cn(
                'w-full px-3 py-2 rounded-md',
                'bg-tokyo-bg border border-tokyo-bg-hl',
                'text-tokyo-fg placeholder-tokyo-comment',
                'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue'
              )}
            />
          </div>

          {/* Host */}
          <div>
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

          {/* Port & Username */}
          <div className="grid grid-cols-2 gap-4">
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

          {/* Auth Type */}
          <div>
            <label className="block text-sm font-medium text-tokyo-fg mb-1">
              Authentication
            </label>
            <select
              value={formData.authType}
              onChange={(e) => handleChange('authType', e.target.value)}
              className={cn(
                'w-full px-3 py-2 rounded-md',
                'bg-tokyo-bg border border-tokyo-bg-hl',
                'text-tokyo-fg',
                'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue'
              )}
            >
              <option value="password">Password</option>
              <option value="key">SSH Key</option>
              <option value="key_with_passphrase">SSH Key with Passphrase</option>
            </select>
          </div>

          {/* Private Key Path (shown only for key-based auth) */}
          {(formData.authType === 'key' || formData.authType === 'key_with_passphrase') && (
            <div>
              <label className="block text-sm font-medium text-tokyo-fg mb-1">
                Private Key Path (optional)
              </label>
              <div className="flex gap-2">
                <input
                  type="text"
                  value={formData.privateKeyPath}
                  onChange={(e) => handleChange('privateKeyPath', e.target.value)}
                  placeholder="~/.ssh/id_rsa"
                  className={cn(
                    'flex-1 px-3 py-2 rounded-md',
                    'bg-tokyo-bg border border-tokyo-bg-hl',
                    'text-tokyo-fg placeholder-tokyo-comment',
                    'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue'
                  )}
                />
                <button
                  type="button"
                  onClick={handleBrowseKeyFile}
                  className={cn(
                    'px-3 py-2 rounded-md',
                    'bg-tokyo-bg-hl text-tokyo-fg',
                    'hover:bg-tokyo-bg hover:text-white',
                    'transition-colors flex items-center gap-1'
                  )}
                >
                  <FolderOpen className="w-4 h-4" />
                  Browse
                </button>
              </div>
              <p className="mt-1 text-xs text-tokyo-comment">
                You can specify the key file path, or enter it when connecting.
              </p>
            </div>
          )}

          {/* Advanced Section */}
          <div className="space-y-3 pt-2 border-t border-tokyo-bg-hl">
            <h3 className="text-sm font-medium text-tokyo-comment uppercase tracking-wider pt-2">Advanced</h3>

            {/* Jump Host */}
            <div>
              <label className="block text-sm font-medium text-tokyo-fg mb-1">Jump Host</label>
              <select
                value={formData.jumpHostId}
                onChange={(e) => handleChange('jumpHostId', e.target.value)}
                className={cn(
                  'w-full px-3 py-2 rounded-md',
                  'bg-tokyo-bg border border-tokyo-bg-hl text-tokyo-fg',
                  'focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
                )}
              >
                <option value="">None (direct)</option>
                {servers
                  .filter(s => s.id !== server?.id)
                  .map(s => (
                    <option key={s.id} value={s.id}>{s.name} ({s.host})</option>
                  ))
                }
              </select>
            </div>

            {/* Agent Forwarding */}
            <div className="flex items-center gap-2">
              <input
                type="checkbox"
                id="editAgentForwarding"
                checked={formData.agentForwarding}
                onChange={(e) => setFormData(prev => ({ ...prev, agentForwarding: e.target.checked }))}
                className="w-4 h-4 rounded"
              />
              <label htmlFor="editAgentForwarding" className="text-sm text-tokyo-fg">
                SSH Agent Forwarding
              </label>
            </div>

            {/* Post-login Command */}
            <div>
              <label className="block text-sm font-medium text-tokyo-fg mb-1">Post-login Command</label>
              <textarea
                value={formData.postLoginCommand}
                onChange={(e) => setFormData(prev => ({ ...prev, postLoginCommand: e.target.value }))}
                placeholder="e.g., cd /app && source .env"
                rows={2}
                className={cn(
                  'w-full px-3 py-2 rounded-md resize-none font-mono text-sm',
                  'bg-tokyo-bg border border-tokyo-bg-hl text-tokyo-fg placeholder-tokyo-comment',
                  'focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
                )}
              />
            </div>
          </div>

          {/* Actions */}
          <div className="flex justify-end gap-3 pt-4">
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
              disabled={loading}
              className={cn(
                'px-4 py-2 rounded-md',
                'bg-tokyo-blue text-white',
                'hover:bg-tokyo-blue/80',
                'disabled:opacity-50 disabled:cursor-not-allowed',
                'transition-colors'
              )}
            >
              {loading ? 'Saving...' : 'Save Changes'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
