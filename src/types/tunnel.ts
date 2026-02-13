// =============================================================================
// SSH Tunnel Types
// =============================================================================

/** Type of SSH tunnel */
export type TunnelType = 'local' | 'remote' | 'dynamic';

/** Persistent tunnel configuration stored per-server */
export interface TunnelConfig {
  id: string;
  server_id: string;
  tunnel_type: TunnelType;
  local_host: string;
  local_port: number;
  /** Remote host (not used for Dynamic tunnels) */
  remote_host?: string;
  /** Remote port (not used for Dynamic tunnels) */
  remote_port?: number;
  /** Whether to auto-start this tunnel when connecting */
  auto_start: boolean;
  /** Whether this config is enabled */
  enabled: boolean;
}

/** Input for creating/updating a tunnel config */
export type TunnelConfigInput = Omit<TunnelConfig, 'id'>;

/** Runtime status of an active tunnel */
export type TunnelStatus = 'starting' | 'active' | 'stopped' | 'error';

/** Runtime information about an active tunnel */
export interface TunnelInfo {
  id: string;
  config: TunnelConfig;
  sessionId: string;
  status: TunnelStatus;
  bytesIn: number;
  bytesOut: number;
  activeConnections: number;
  errorMessage?: string;
}

// =============================================================================
// Command Snippet Types
// =============================================================================

/** A saved command snippet / template */
export interface CommandSnippet {
  id: string;
  name: string;
  command: string;
  category: string;
  description: string;
  tags: string[];
  created_at: number;
  updated_at: number;
}

/** Input for creating a new snippet */
export type CreateSnippetInput = Omit<CommandSnippet, 'id' | 'created_at' | 'updated_at'>;

/** Input for updating an existing snippet */
export type UpdateSnippetInput = Partial<CreateSnippetInput>;

// =============================================================================
// Recording Types
// =============================================================================

/** Session recording metadata */
export interface Recording {
  id: string;
  sessionId: string;
  serverId: string;
  startedAt: number;
  endedAt?: number;
  filePath: string;
  syncStatus: 'local' | 'syncing' | 'synced';
}
