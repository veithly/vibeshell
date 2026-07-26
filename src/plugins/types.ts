export type PluginPermission = 'remote_exec' | 'local_system_read' | 'local_exec';
export type PluginSessionType = 'ssh' | 'local';
export type PluginSource = 'builtin' | 'external';
export type PluginInputKind = 'text' | 'integer' | 'boolean' | 'select';
export type PluginOutputKind = 'text' | 'table';

export interface PluginInput {
  id: string;
  label: string;
  description: string;
  placeholder: string;
  required: boolean;
  kind: PluginInputKind;
  options: string[];
}

export interface PluginOutput {
  kind: PluginOutputKind;
  columns: string[];
  delimiter: string;
}

export interface PluginAction {
  id: string;
  name: string;
  description: string;
  program: string;
  args: string[];
  inputs: PluginInput[];
  requiresConfirmation: boolean;
  /**
   * When true the command runs under `sudo` so privileged operations work
   * without a dedicated interactive shell. Always implies that the user is
   * prompted for confirmation; a sudo password may be supplied at runtime.
  */
  elevate: boolean;
  /** Allows an explicit sudo retry while keeping normal execution unprivileged. */
  allowSudo: boolean;
  output: PluginOutput;
}

export type PluginEntry =
  | { type: 'native'; view: 'server-status' }
  | { type: 'commands'; actions: PluginAction[] };

export interface PluginManifest {
  schemaVersion: number;
  id: string;
  name: string;
  description: string;
  version: string;
  author: string;
  category: string;
  icon: string;
  permissions: PluginPermission[];
  sessionTypes: PluginSessionType[];
  defaultSettings: Record<string, unknown>;
  entry: PluginEntry;
}

export interface PluginRecord {
  manifest: PluginManifest;
  source: PluginSource;
  installed: boolean;
  enabled: boolean;
  grantedPermissions: PluginPermission[];
  settings: Record<string, unknown>;
  installedAt: number | null;
}

export interface PluginExecutionResult {
  pluginId: string;
  actionId: string;
  output: string;
  durationMs: number;
  truncated: boolean;
}

export type PluginInputValues = Record<string, string | number | boolean>;
