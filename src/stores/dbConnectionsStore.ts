import { create } from 'zustand';
import { safeInvoke } from '../lib/tauri';

export type DbEngineId = 'postgresql' | 'mysql';

export interface DbConnection {
  id: string;
  name: string;
  engine: DbEngineId;
  host: string;
  port: number;
  username: string;
  hasPassword: boolean;
  defaultDatabase: string | null;
  createdAt: number;
  updatedAt: number;
  lastConnectedAt: number | null;
}

export interface DbConnectionInput {
  id?: string;
  name: string;
  engine: DbEngineId;
  host: string;
  port: number;
  username: string;
  /** Empty string keeps the stored password when editing. */
  password?: string;
  defaultDatabase?: string | null;
}

export interface DbTestResult {
  ok: boolean;
  latencyMs: number;
  serverVersion: string | null;
  error: string | null;
}

export interface DbColumnMeta {
  name: string;
  dataType: string;
  nullable: boolean;
  defaultValue: string | null;
}

export interface DbQueryResult {
  columns: string[];
  rows: unknown[][];
  rowsAffected: number | null;
  durationMs: number;
  truncated: boolean;
}

export interface DatabaseSuggestion {
  engine: string;
  port: number;
  source: string;
  detail: string;
}

export type ConnectionStatus = 'idle' | 'testing' | 'ok' | 'fail';

interface DbConnectionsState {
  connections: DbConnection[];
  loading: boolean;
  initialized: boolean;
  statuses: Record<string, { status: ConnectionStatus; result: DbTestResult | null }>;
  databases: Record<string, string[]>;
  tables: Record<string, string[]>;
  fetchConnections: () => Promise<void>;
  saveConnection: (input: DbConnectionInput) => Promise<DbConnection>;
  deleteConnection: (id: string) => Promise<boolean>;
  testConnection: (id: string) => Promise<DbTestResult | null>;
  probeConnection: (input: DbConnectionInput) => Promise<DbTestResult | null>;
  loadDatabases: (id: string) => Promise<string[]>;
  loadTables: (id: string, database: string) => Promise<string[]>;
  loadColumns: (id: string, database: string, table: string) => Promise<DbColumnMeta[] | null>;
  query: (
    id: string,
    database: string | null,
    sql: string,
    maxRows?: number
  ) => Promise<DbQueryResult | null>;
  detectFromSession: (sessionId: string) => Promise<DatabaseSuggestion[] | null>;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function invokeOr<T>(command: string, args: Record<string, unknown>): Promise<T> {
  const result = await safeInvoke<T>(command, args);
  if (result.success) {
    return result.data;
  }
  throw new Error(result.error.message);
}

export const useDbConnectionsStore = create<DbConnectionsState>((set, get) => ({
  connections: [],
  loading: false,
  initialized: false,
  statuses: {},
  databases: {},
  tables: {},

  fetchConnections: async () => {
    set({ loading: true });
    try {
      const connections = await invokeOr<DbConnection[]>('db_connection_list', {});
      set({ connections, loading: false, initialized: true });
    } catch (error) {
      console.error('[DbConnections] Failed to load connections:', errorMessage(error));
      set({ loading: false, initialized: true });
    }
  },

  saveConnection: async (input) => {
    const saved = await invokeOr<DbConnection>('db_connection_save', { input });
    set((state) => {
      const others = state.connections.filter((candidate) => candidate.id !== saved.id);
      return {
        connections: [...others, saved].sort((left, right) =>
          left.name.localeCompare(right.name)
        ),
      };
    });
    return saved;
  },

  deleteConnection: async (id) => {
    try {
      await invokeOr('db_connection_delete', { request: { connectionId: id } });
      set((state) => {
        const { [id]: _status, ...statuses } = state.statuses;
        const databases = { ...state.databases };
        delete databases[id];
        return {
          connections: state.connections.filter((candidate) => candidate.id !== id),
          statuses,
          databases,
        };
      });
      return true;
    } catch (error) {
      console.error('[DbConnections] Failed to delete connection:', errorMessage(error));
      return false;
    }
  },

  testConnection: async (id) => {
    set((state) => ({
      statuses: { ...state.statuses, [id]: { status: 'testing', result: null } },
    }));
    try {
      const result = await invokeOr<DbTestResult>('db_connection_test', {
        request: { connectionId: id },
      });
      set((state) => ({
        statuses: {
          ...state.statuses,
          [id]: { status: result.ok ? 'ok' : 'fail', result },
        },
      }));
      if (result.ok) {
        void get().loadDatabases(id);
      }
      return result;
    } catch (error) {
      set((state) => ({
        statuses: {
          ...state.statuses,
          [id]: { status: 'fail', result: { ok: false, latencyMs: 0, serverVersion: null, error: errorMessage(error) } },
        },
      }));
      return null;
    }
  },

  probeConnection: async (input) => {
    try {
      return await invokeOr<DbTestResult>('db_connection_probe', { input });
    } catch (error) {
      return { ok: false, latencyMs: 0, serverVersion: null, error: errorMessage(error) };
    }
  },

  loadDatabases: async (id) => {
    const databases = await invokeOr<string[]>('db_connection_databases', {
      request: { connectionId: id },
    });
    set((state) => ({ databases: { ...state.databases, [id]: databases } }));
    return databases;
  },

  loadTables: async (id, database) => {
    const tables = await invokeOr<string[]>('db_connection_tables', {
      request: { connectionId: id, database },
    });
    set((state) => ({ tables: { ...state.tables, [`${id}::${database}`]: tables } }));
    return tables;
  },

  loadColumns: async (id, database, table) => {
    try {
      return await invokeOr<DbColumnMeta[]>('db_connection_columns', {
        request: { connectionId: id, database, table },
      });
    } catch (error) {
      console.error('[DbConnections] Failed to load columns:', errorMessage(error));
      return null;
    }
  },

  query: async (id, database, sql, maxRows = 500) => {
    return invokeOr<DbQueryResult>('db_connection_query', {
      request: { connectionId: id, database, sql, maxRows },
    });
  },

  detectFromSession: async (sessionId) => {
    try {
      return await invokeOr<DatabaseSuggestion[]>('db_session_detect', {
        request: { sessionId },
      });
    } catch (error) {
      console.error('[DbConnections] Detection failed:', errorMessage(error));
      return null;
    }
  },
}));
