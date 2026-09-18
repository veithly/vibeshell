import { useState, useCallback, useEffect } from 'react';
import { X } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useServerStore, isKeyAuthType, type Server, type AuthType, type UpdateServerInput } from '../../stores/serverStore';
import { useSshKeyPicker } from '../../hooks/useSshKeyPicker';
import { useNotificationStore } from '../../stores/notificationStore';

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
    jumpHostId: '',
    agentForwarding: false,
    postLoginCommand: '',
  });

  const [localError, setLocalError] = useState<string | null>(null);
  const [newPassword, setNewPassword] = useState('');
  const [changePassphrase, setChangePassphrase] = useState(false);
  const [newPassphrase, setNewPassphrase] = useState('');
  const { keyPath, keyContent, isLoadingKey, browseForSshKey, setKeyContent, reset } = useSshKeyPicker({ onError: setLocalError });
  const isKey = isKeyAuthType(formData.authType);

  // Initialize form data from the persisted server every time the dialog is
  // opened. Re-running on `isOpen` guarantees that unsaved draft edits from a
  // previously cancelled dialog are discarded instead of leaking back in.
  useEffect(() => {
    if (!isOpen) {
      setNewPassword(''); setNewPassphrase(''); setChangePassphrase(false); reset();
      return;
    }
    if (server) {
      setNewPassword('');
      setNewPassphrase('');
      setChangePassphrase(false);
      reset();
      setFormData({
        name: server.name,
        host: server.host,
        port: server.port,
        username: server.username,
        // Standalone 'key' auth was removed; legacy rows fall back to the
        // unified key+passphrase mode (empty passphrase = unencrypted key).
        authType: server.auth_type === 'key' ? 'key_with_passphrase' : server.auth_type,
        jumpHostId: server.jump_host_id || '',
        agentForwarding: server.agent_forwarding || false,
        postLoginCommand: server.post_login_command || '',
      });
    }
  }, [isOpen, server, reset]);

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

    if (!server || isLoadingKey) return;

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

    if (!Number.isInteger(formData.port) || formData.port < 1 || formData.port > 65535) {
      setLocalError('Port must be between 1 and 65535');
      return;
    }
    const authChanged = isKeyAuthType(server.auth_type) !== isKey;
    if (authChanged && (isKey ? !keyContent : !newPassword)) {
      setLocalError(isKey ? 'Select or paste the new private key' : 'Enter the new password');
      return;
    }
    const credentials: NonNullable<UpdateServerInput['credentials']> = {};
    if (isKey) {
      if (keyContent) { credentials.credential = keyContent; credentials.keyPath = keyPath ?? ''; }
      if (changePassphrase) credentials.passphrase = newPassphrase;
    } else if (newPassword) {
      credentials.credential = newPassword;
    }

    try {
      await updateServer(server.id, {
        name: formData.name.trim(),
        host: formData.host.trim(),
        port: formData.port,
        username: formData.username.trim(),
        auth_type: formData.authType,
        jump_host_id: formData.jumpHostId || null,
        agent_forwarding: formData.agentForwarding,
        post_login_command: formData.postLoginCommand.trim() || null,
        ...(Object.keys(credentials).length ? { credentials } : {}),
      });

      notifySuccess('Server Updated', `${formData.name} has been updated successfully.`);
      onClose();
    } catch (err) {
      setLocalError(err instanceof Error ? err.message : 'Failed to update server');
    }
  }, [server, formData, updateServer, notifySuccess, onClose, isKey, newPassword, keyContent, keyPath, changePassphrase, newPassphrase, isLoadingKey]);

  const handleChange = useCallback((field: string, value: string | number) => {
    setFormData(prev => ({ ...prev, [field]: value }));
    setLocalError(null);
    clearError();
  }, [clearError]);

  if (!isOpen || !server) return null;

  const displayError = localError || error;

  return (
    <div className="responsive-dialog-layer fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/60"
        onClick={() => { if (!loading) onClose(); }}
      />

      {/* Dialog */}
      <div className="responsive-dialog-panel relative min-w-0 bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg shadow-xl w-full max-w-md mx-3 sm:mx-4">
        {/* Header */}
        <div className="responsive-dialog-header flex items-center justify-between gap-3 px-4 py-3 border-b border-tokyo-bg-hl">
          <h2 className="text-lg font-semibold text-tokyo-fg">Edit Server</h2>
          <button
            className="p-1 rounded-md text-tokyo-comment hover:text-tokyo-fg hover:bg-tokyo-bg-hl transition-colors"
            onClick={onClose}
            aria-label="Close edit server dialog"
            disabled={loading}
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Form */}
        <form onSubmit={handleSubmit} className="p-4 space-y-4">
          {displayError && (
            <div className="p-3 rounded-md bg-tokyo-red/10 border border-tokyo-red/30 text-tokyo-red text-sm">
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
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <div>
              <label className="block text-sm font-medium text-tokyo-fg mb-1">
                Port
              </label>
              <input
                type="number"
                value={formData.port}
                onChange={(e) => handleChange('port', Number(e.target.value))}
                min={1}
                max={65535}
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
              <option value="key_with_passphrase">SSH Key (passphrase optional)</option>
            </select>
          </div>

          <fieldset disabled={loading || isLoadingKey} className="space-y-3 border-t border-tokyo-bg-hl pt-3">
            <legend className="text-sm font-medium text-tokyo-fg">Saved credentials</legend>
            <p className="text-xs text-tokyo-comment">Unchanged fields keep the saved credentials. This updates VibeShell's saved login, not the password on the server.</p>
            {!isKey ? (
              <div>
                <label htmlFor="edit-server-password" className="block text-sm text-tokyo-fg mb-1">New password</label>
                <input id="edit-server-password" type="password" autoComplete="new-password" value={newPassword}
                  onChange={(event) => setNewPassword(event.target.value)} placeholder="Leave blank to keep saved password"
                  className="w-full rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 py-2 text-tokyo-fg focus:ring-1 focus:ring-tokyo-blue" />
              </div>
            ) : (
              <>
                <button type="button" onClick={browseForSshKey} className="rounded-md bg-tokyo-bg-hl px-3 py-2 text-sm text-tokyo-fg">
                  {isLoadingKey ? 'Loading key…' : 'Choose replacement private key'}
                </button>
                {keyPath && <p className="break-all text-xs text-tokyo-comment">{keyPath}</p>}
                <label htmlFor="edit-server-key" className="block text-sm text-tokyo-fg">Replacement private key (optional)</label>
                <textarea id="edit-server-key" value={keyContent ?? ''} onChange={(event) => setKeyContent(event.target.value || null)}
                  rows={3} spellCheck={false} autoCapitalize="none" autoCorrect="off" placeholder="Leave blank to keep saved key"
                  className="w-full rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 py-2 font-mono text-sm text-tokyo-fg focus:ring-1 focus:ring-tokyo-blue" />
                <label className="flex items-center gap-2 text-sm text-tokyo-fg">
                  <input type="checkbox" checked={changePassphrase} onChange={(event) => setChangePassphrase(event.target.checked)} />
                  Change key passphrase
                </label>
                {changePassphrase && <div>
                  <label htmlFor="edit-server-passphrase" className="block text-sm text-tokyo-fg mb-1">New key passphrase</label>
                  <input id="edit-server-passphrase" type="password" autoComplete="new-password" value={newPassphrase}
                    onChange={(event) => setNewPassphrase(event.target.value)} placeholder="Empty clears the saved passphrase"
                    className="w-full rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 py-2 text-tokyo-fg focus:ring-1 focus:ring-tokyo-blue" />
                </div>}
              </>
            )}
          </fieldset>

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
          <div className="responsive-dialog-actions flex justify-end gap-3 pt-4">
            <button
              type="button"
              onClick={onClose}
              className={cn(
                'px-4 py-2 rounded-md',
                'bg-tokyo-bg-hl text-tokyo-fg',
                'hover:bg-tokyo-bg hover:text-tokyo-fg',
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
                'bg-tokyo-blue text-tokyo-on-accent',
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
