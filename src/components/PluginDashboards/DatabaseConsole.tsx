import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ChevronDown,
  ChevronRight,
  Database,
  Loader2,
  Pencil,
  Play,
  Plus,
  Radar,
  Table2,
  Trash2,
} from 'lucide-react';
import { cn } from '../../lib/utils';
import {
  useDbConnectionsStore,
  type DbColumnMeta,
  type DbConnection,
  type DbConnectionInput,
  type DbQueryResult,
} from '../../stores/dbConnectionsStore';
import { ConnectionDialog } from './ConnectionDialog';
import { DetectDialog } from './DetectDialog';
import { DashboardHeader, ErrorBanner } from './ui';

const PAGE_SIZE = 100;

interface Selection {
  connectionId: string;
  database: string;
  table: string;
}

type MainTab = 'data' | 'structure' | 'sql';

function JsonGrid({ result, emptyText }: { result: DbQueryResult | null; emptyText: string }) {
  if (!result) {
    return <div className="flex h-full min-h-[120px] items-center justify-center text-xs text-tokyo-comment">{emptyText}</div>;
  }
  if (result.columns.length === 0) {
    return (
      <div className="flex h-full min-h-[120px] items-center justify-center text-xs text-tokyo-comment">
        {result.rowsAffected !== null ? `${result.rowsAffected} rows affected · ${result.durationMs} ms` : emptyText}
      </div>
    );
  }
  return (
    <div className="m-2 overflow-auto rounded-lg border border-tokyo-bg-hl">
      <table className="w-full border-separate border-spacing-0 text-left text-xs">
        <thead className="sticky top-0 bg-tokyo-bg-dark text-tokyo-comment">
          <tr>
            {result.columns.map((column, index) => (
              <th key={`${column}-${index}`} className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">
                {column}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="font-mono text-tokyo-fg">
          {result.rows.map((row, rowIndex) => (
            <tr key={rowIndex} className="hover:bg-tokyo-bg-hl/40">
              {row.map((cell, cellIndex) => (
                <td key={cellIndex} className="max-w-[280px] border-b border-tokyo-bg-hl/60 px-3 py-1.5 align-top">
                  {cell === null || cell === undefined ? (
                    <span className="italic text-tokyo-comment">NULL</span>
                  ) : (
                    <span className="block truncate" title={String(cell)}>{String(cell)}</span>
                  )}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function DatabaseConsole() {
  const { t } = useTranslation();
  const {
    connections,
    initialized,
    fetchConnections,
    statuses,
    databases,
    tables,
    testConnection,
    loadDatabases,
    loadTables,
    loadColumns,
    deleteConnection,
    query,
  } = useDbConnectionsStore();

  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [expandedDatabases, setExpandedDatabases] = useState<Set<string>>(new Set());
  const [selection, setSelection] = useState<Selection | null>(null);
  const [tab, setTab] = useState<MainTab>('data');
  const [page, setPage] = useState(0);
  const [dataResult, setDataResult] = useState<DbQueryResult | null>(null);
  const [dataLoading, setDataLoading] = useState(false);
  const [hasMorePages, setHasMorePages] = useState(false);
  const [structure, setStructure] = useState<DbColumnMeta[] | null>(null);
  const [sqlText, setSqlText] = useState('');
  const [sqlResult, setSqlResult] = useState<DbQueryResult | null>(null);
  const [sqlRunning, setSqlRunning] = useState(false);
  const [sqlError, setSqlError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dialog, setDialog] = useState<{ editing: DbConnection | null; prefill: Partial<DbConnectionInput> | null } | null>(null);
  const [detectOpen, setDetectOpen] = useState(false);

  useEffect(() => {
    if (!initialized) void fetchConnections();
  }, [initialized, fetchConnections]);

  const selectedConnection = useMemo(
    () => connections.find((candidate) => candidate.id === selection?.connectionId) ?? null,
    [connections, selection]
  );

  const quoteIdent = (identifier: string, engine: string) =>
    engine === 'mysql'
      ? `\`${identifier.replace(/`/g, '``')}\``
      : `"${identifier.replace(/"/g, '""')}"`;

  const loadPage = useCallback(
    async (target: Selection, targetPage: number) => {
      const connection = connections.find((candidate) => candidate.id === target.connectionId);
      if (!connection) return;
      setDataLoading(true);
      setError(null);
      try {
        const ident = quoteIdent(target.table, connection.engine);
        const result = await query(
          target.connectionId,
          target.database,
          `SELECT * FROM ${ident} LIMIT ${PAGE_SIZE + 1} OFFSET ${targetPage * PAGE_SIZE}`,
          PAGE_SIZE + 1
        );
        if (result) {
          setHasMorePages(result.rows.length > PAGE_SIZE);
          setDataResult({ ...result, rows: result.rows.slice(0, PAGE_SIZE) });
        }
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      } finally {
        setDataLoading(false);
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [connections, query]
  );

  const loadStructure = useCallback(async (target: Selection) => {
    setStructure(null);
    const columns = await loadColumns(target.connectionId, target.database, target.table);
    setStructure(columns);
  }, [loadColumns]);

  const selectTable = useCallback((target: Selection) => {
    setSelection(target);
    setTab('data');
    setPage(0);
    setDataResult(null);
    void loadPage(target, 0);
    void loadStructure(target);
  }, [loadPage, loadStructure]);

  const toggleConnection = async (connection: DbConnection) => {
    const nextExpanded = new Set(expanded);
    if (nextExpanded.has(connection.id)) {
      nextExpanded.delete(connection.id);
      setExpanded(nextExpanded);
      return;
    }
    nextExpanded.add(connection.id);
    setExpanded(nextExpanded);
    const status = statuses[connection.id]?.status;
    if (status !== 'ok') {
      const result = await testConnection(connection.id);
      if (!result?.ok) setError(result?.error ?? 'Connection failed');
    } else if (!databases[connection.id]) {
      loadDatabases(connection.id).catch((caught) =>
        setError(caught instanceof Error ? caught.message : String(caught))
      );
    }
  };

  const toggleDatabase = async (connectionId: string, database: string) => {
    const key = `${connectionId}::${database}`;
    const next = new Set(expandedDatabases);
    if (next.has(key)) {
      next.delete(key);
      setExpandedDatabases(next);
      return;
    }
    next.add(key);
    setExpandedDatabases(next);
    try {
      await loadTables(connectionId, database);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  };

  const runSql = async () => {
    if (!selection && !selectedConnection) return;
    const connectionId = selection?.connectionId ?? selectedConnection?.id;
    if (!connectionId) return;
    const sql = sqlText.trim();
    if (!sql) return;
    setSqlRunning(true);
    setSqlError(null);
    setSqlResult(null);
    try {
      const result = await query(connectionId, selection?.database ?? null, sql);
      setSqlResult(result);
    } catch (caught) {
      setSqlError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setSqlRunning(false);
    }
  };

  const removeConnection = async (connection: DbConnection) => {
    if (!window.confirm(t('plugins.dbConn.deleteConfirm', { name: connection.name }))) return;
    if (selection?.connectionId === connection.id) {
      setSelection(null);
      setDataResult(null);
    }
    await deleteConnection(connection.id);
  };

  const statusDot = (connectionId: string) => {
    const status = statuses[connectionId]?.status ?? 'idle';
    return (
      <span
        className={cn(
          'h-2 w-2 flex-shrink-0 rounded-full',
          status === 'ok' && 'bg-tokyo-green',
          status === 'fail' && 'bg-tokyo-red',
          status === 'testing' && 'animate-pulse bg-tokyo-yellow',
          status === 'idle' && 'bg-tokyo-comment'
        )}
        title={status}
      />
    );
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <DashboardHeader
        icon={<Database className="h-4 w-4 text-tokyo-cyan" />}
        title={t('plugins.dbConn.title')}
        badge={selection ? `${selection.database} · ${selection.table.split('.').pop()}` : null}
        onRefresh={() => {
          if (selection) {
            void loadPage(selection, page);
            void loadStructure(selection);
          } else {
            void fetchConnections();
          }
        }}
        refreshing={dataLoading}
      />
      <ErrorBanner message={error} onDismiss={() => setError(null)} />

      <div className="flex min-h-0 flex-1">
        {/* Connection tree (Navicat-style) */}
        <aside className="w-56 flex-shrink-0 overflow-y-auto border-r border-tokyo-bg-hl bg-tokyo-bg-dark/40 p-2">
          <div className="mb-2 flex items-center gap-1 px-1">
            <span className="flex-1 text-[10px] font-semibold uppercase tracking-wide text-tokyo-comment">
              {t('plugins.dbConn.connections')} ({connections.length})
            </span>
            <button
              className="icon-button h-6 w-6"
              onClick={() => setDetectOpen(true)}
              aria-label={t('plugins.dbConn.detectTitle')}
              title={t('plugins.dbConn.detectTitle')}
            >
              <Radar className="h-3.5 w-3.5" />
            </button>
            <button
              className="icon-button h-6 w-6"
              onClick={() => setDialog({ editing: null, prefill: null })}
              aria-label={t('plugins.dbConn.addTitle')}
              title={t('plugins.dbConn.addTitle')}
            >
              <Plus className="h-3.5 w-3.5" />
            </button>
          </div>

          {connections.length === 0 && (
            <div className="px-2 py-6 text-center text-[11px] leading-5 text-tokyo-comment">
              {t('plugins.dbConn.emptyHint')}
            </div>
          )}

          {connections.map((connection) => {
            const isExpanded = expanded.has(connection.id);
            const dbList = databases[connection.id];
            return (
              <div key={connection.id} className="group/conn mb-0.5">
                <div
                  className={cn(
                    'flex items-center gap-1.5 rounded-md px-1.5 py-1.5 text-xs transition-colors cursor-pointer',
                    selection?.connectionId === connection.id
                      ? 'bg-tokyo-bg-hl text-tokyo-fg'
                      : 'text-tokyo-fg hover:bg-tokyo-bg-hl/60'
                  )}
                  onClick={() => void toggleConnection(connection)}
                >
                  {isExpanded
                    ? <ChevronDown className="h-3 w-3 flex-shrink-0 text-tokyo-comment" />
                    : <ChevronRight className="h-3 w-3 flex-shrink-0 text-tokyo-comment" />}
                  {statusDot(connection.id)}
                  <Database
                    className={cn(
                      'h-3.5 w-3.5 flex-shrink-0',
                      connection.engine === 'postgresql' ? 'text-tokyo-cyan' : 'text-tokyo-orange'
                    )}
                  />
                  <span className="min-w-0 flex-1 truncate" title={`${connection.host}:${connection.port}`}>
                    {connection.name}
                  </span>
                  <button
                    className="hidden h-5 w-5 items-center justify-center rounded hover:bg-tokyo-bg-hl group-hover/conn:flex"
                    onClick={(event) => {
                      event.stopPropagation();
                      setDialog({ editing: connection, prefill: null });
                    }}
                    aria-label={t('common.edit')}
                  >
                    <Pencil className="h-3 w-3" />
                  </button>
                  <button
                    className="hidden h-5 w-5 items-center justify-center rounded hover:bg-tokyo-bg-hl hover:text-tokyo-red group-hover/conn:flex"
                    onClick={(event) => {
                      event.stopPropagation();
                      void removeConnection(connection);
                    }}
                    aria-label={t('plugins.dbConn.delete')}
                  >
                    <Trash2 className="h-3 w-3" />
                  </button>
                </div>

                {isExpanded && (
                  <div className="ml-5 border-l border-tokyo-bg-hl pl-1">
                    {statuses[connection.id]?.status === 'testing' && (
                      <div className="flex items-center gap-1.5 px-2 py-1 text-[11px] text-tokyo-comment">
                        <Loader2 className="h-3 w-3 animate-spin" />
                        {t('plugins.dbConn.connecting')}
                      </div>
                    )}
                    {statuses[connection.id]?.result?.ok && (
                      <div className="px-2 py-0.5 text-[10px] text-tokyo-comment">
                        {statuses[connection.id]?.result?.serverVersion} · {statuses[connection.id]?.result?.latencyMs} ms
                      </div>
                    )}
                    {(dbList ?? []).map((database) => {
                      const key = `${connection.id}::${database}`;
                      const isDbExpanded = expandedDatabases.has(key);
                      return (
                        <div key={key}>
                          <div
                            className="flex cursor-pointer items-center gap-1.5 rounded-md px-1.5 py-1 text-xs text-tokyo-comment transition-colors hover:bg-tokyo-bg-hl/60 hover:text-tokyo-fg"
                            onClick={() => void toggleDatabase(connection.id, database)}
                          >
                            {isDbExpanded
                              ? <ChevronDown className="h-3 w-3 flex-shrink-0" />
                              : <ChevronRight className="h-3 w-3 flex-shrink-0" />}
                            <Database className="h-3 w-3 flex-shrink-0 opacity-70" />
                            <span className="min-w-0 flex-1 truncate font-mono">{database}</span>
                          </div>
                          {isDbExpanded && (
                            <div className="ml-4 border-l border-tokyo-bg-hl pl-1">
                              {(tables[key] ?? []).map((table) => (
                                <button
                                  key={table}
                                  className={cn(
                                    'flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left text-xs transition-colors',
                                    selection
                                      && selection.connectionId === connection.id
                                      && selection.database === database
                                      && selection.table === table
                                      ? 'bg-tokyo-bg-hl text-tokyo-cyan'
                                      : 'text-tokyo-comment hover:bg-tokyo-bg-hl/60 hover:text-tokyo-fg'
                                  )}
                                  onClick={() => selectTable({ connectionId: connection.id, database, table })}
                                >
                                  <Table2 className="h-3 w-3 flex-shrink-0" />
                                  <span className="min-w-0 flex-1 truncate font-mono">
                                    {table.includes('.') ? table.split('.').slice(1).join('.') : table}
                                  </span>
                                </button>
                              ))}
                              {(tables[key] ?? []).length === 0 && (
                                <div className="px-2 py-1 text-[10px] text-tokyo-comment">
                                  {t('common.noData')}
                                </div>
                              )}
                            </div>
                          )}
                        </div>
                      );
                    })}
                    {dbList === undefined && statuses[connection.id]?.status === 'ok' && (
                      <div className="flex items-center gap-1.5 px-2 py-1 text-[11px] text-tokyo-comment">
                        <Loader2 className="h-3 w-3 animate-spin" />
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </aside>

        {/* Main content */}
        <div className="flex min-w-0 flex-1 flex-col">
          {selection === null ? (
            <div className="flex h-full flex-col items-center justify-center gap-2 text-tokyo-comment">
              <Database className="h-8 w-8 opacity-40" />
              <p className="max-w-[300px] text-center text-xs leading-5">
                {t('plugins.dbConn.mainHint')}
              </p>
            </div>
          ) : (
            <>
              <div className="flex h-9 flex-shrink-0 items-center gap-2 border-b border-tokyo-bg-hl px-3">
                <div className="flex h-7 rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark p-0.5">
                  {([
                    ['data', t('plugins.dbConn.dataTab')],
                    ['structure', t('plugins.dbConn.structureTab')],
                    ['sql', 'SQL'],
                  ] as Array<[MainTab, string]>).map(([id, label]) => (
                    <button
                      key={id}
                      className={cn(
                        'rounded px-2.5 text-xs transition-colors',
                        tab === id ? 'bg-tokyo-bg-hl text-tokyo-fg' : 'text-tokyo-comment hover:text-tokyo-fg'
                      )}
                      onClick={() => setTab(id)}
                    >
                      {label}
                    </button>
                  ))}
                </div>
                <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-tokyo-comment">
                  {selectedConnection?.name} · {selection.database} · {selection.table}
                </span>
                {tab === 'data' && (
                  <div className="flex items-center gap-1">
                    <button
                      className="icon-button h-7 w-7"
                      disabled={page === 0 || dataLoading}
                      onClick={() => {
                        const next = page - 1;
                        setPage(next);
                        void loadPage(selection, next);
                      }}
                      aria-label="previous page"
                    >
                      <ChevronRight className="h-3.5 w-3.5 rotate-180" />
                    </button>
                    <span className="text-[11px] tabular-nums text-tokyo-comment">
                      {(dataResult?.rows.length ?? 0) > 0 ? page * PAGE_SIZE + 1 : 0}–
                      {page * PAGE_SIZE + (dataResult?.rows.length ?? 0)}
                    </span>
                    <button
                      className="icon-button h-7 w-7"
                      disabled={!hasMorePages || dataLoading}
                      onClick={() => {
                        const next = page + 1;
                        setPage(next);
                        void loadPage(selection, next);
                      }}
                      aria-label="next page"
                    >
                      <ChevronRight className="h-3.5 w-3.5" />
                    </button>
                    {dataLoading && <Loader2 className="h-3.5 w-3.5 animate-spin text-tokyo-comment" />}
                  </div>
                )}
              </div>

              {tab === 'data' && (
                <div className="min-h-0 flex-1 overflow-auto">
                  <JsonGrid result={dataResult} emptyText={t('common.noData')} />
                </div>
              )}

              {tab === 'structure' && (
                <div className="min-h-0 flex-1 overflow-auto">
                  {structure === null ? (
                    <div className="flex h-full items-center justify-center text-xs text-tokyo-comment">
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      {t('plugins.dbConn.loading')}
                    </div>
                  ) : structure.length === 0 ? (
                    <div className="flex h-full items-center justify-center text-xs text-tokyo-comment">
                      {t('common.noData')}
                    </div>
                  ) : (
                    <div className="m-2 overflow-hidden rounded-lg border border-tokyo-bg-hl">
                      <table className="w-full border-separate border-spacing-0 text-left text-xs">
                        <thead className="bg-tokyo-bg-dark text-tokyo-comment">
                          <tr>
                            <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.dbConn.column')}</th>
                            <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.dbConn.type')}</th>
                            <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">NULL</th>
                            <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.dbConn.default')}</th>
                          </tr>
                        </thead>
                        <tbody className="font-mono text-tokyo-fg">
                          {structure.map((column) => (
                            <tr key={column.name} className="hover:bg-tokyo-bg-hl/40">
                              <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5 text-tokyo-cyan">{column.name}</td>
                              <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5">{column.dataType}</td>
                              <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5">
                                {column.nullable ? <span className="text-tokyo-comment">NULL</span> : <span className="text-tokyo-yellow">NOT NULL</span>}
                              </td>
                              <td className="max-w-[240px] border-b border-tokyo-bg-hl/60 px-3 py-1.5">
                                <span className="block truncate" title={column.defaultValue ?? ''}>
                                  {column.defaultValue ?? '-'}
                                </span>
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                </div>
              )}

              {tab === 'sql' && (
                <div className="flex min-h-0 flex-1 flex-col">
                  <div className="border-b border-tokyo-bg-hl p-2">
                    <textarea
                      className="h-20 w-full resize-none rounded-lg border border-tokyo-bg-hl bg-tokyo-bg p-2.5 font-mono text-xs text-tokyo-fg outline-none focus:border-tokyo-cyan"
                      placeholder="SELECT * FROM …"
                      value={sqlText}
                      onChange={(event) => setSqlText(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
                          event.preventDefault();
                          void runSql();
                        }
                      }}
                    />
                    <div className="mt-1.5 flex items-center gap-2">
                      <button
                        className="flex h-7 items-center gap-1.5 rounded-md bg-tokyo-blue px-3 text-xs font-medium text-tokyo-on-accent hover:opacity-90 disabled:opacity-50"
                        onClick={() => void runSql()}
                        disabled={sqlRunning || !sqlText.trim()}
                      >
                        {sqlRunning ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Play className="h-3.5 w-3.5" />}
                        {t('plugins.run')}
                      </button>
                      <span className="text-[10px] text-tokyo-comment">⌘/Ctrl + Enter</span>
                      {sqlResult && (
                        <span className="ml-auto text-[10px] text-tokyo-comment">
                          {sqlResult.rows.length} {t('plugins.database.rows')} · {sqlResult.durationMs} ms
                          {sqlResult.rowsAffected !== null && ` · ${sqlResult.rowsAffected} affected`}
                        </span>
                      )}
                    </div>
                  </div>
                  {sqlError && (
                    <div className="m-2 rounded-md border border-tokyo-red/30 bg-tokyo-red/10 px-3 py-2 font-mono text-xs text-tokyo-red">
                      {sqlError}
                    </div>
                  )}
                  <div className="min-h-0 flex-1 overflow-auto">
                    <JsonGrid result={sqlResult} emptyText={t('plugins.dbConn.sqlHint')} />
                  </div>
                </div>
              )}
            </>
          )}
        </div>
      </div>

      {dialog && (
        <ConnectionDialog
          editing={dialog.editing}
          prefill={dialog.prefill}
          onClose={() => setDialog(null)}
          onSaved={(connection) => {
            void toggleConnection(connection);
          }}
        />
      )}
      {detectOpen && (
        <DetectDialog
          onClose={() => setDetectOpen(false)}
          onAdopt={(prefill) => setDialog({ editing: null, prefill })}
        />
      )}
    </div>
  );
}
