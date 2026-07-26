import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  AlertTriangle,
  Loader2,
  Play,
  RefreshCw,
  ShieldCheck,
  Store,
  X,
} from 'lucide-react';
import { cn } from '../../lib/utils';
import { isPluginCompatible, parsePluginTable } from '../../plugins/pluginUtils';
import type {
  PluginAction,
  PluginExecutionResult,
  PluginInputValues,
  PluginRecord,
  PluginSessionType,
} from '../../plugins/types';
import { usePluginStore } from '../../stores/pluginStore';
import { PluginIcon } from '../PluginIcon';
import {
  refreshIntervalOptions,
  ServerStatus,
  type RefreshInterval,
} from '../ServerStatus';

interface SessionPluginDockProps {
  sessionId: string;
  sessionType: PluginSessionType;
  open: boolean;
  onClose: () => void;
  onOpenMarketplace: () => void;
}

function localizedPluginName(
  t: ReturnType<typeof useTranslation>['t'],
  plugin: PluginRecord
): string {
  return t(`plugins.catalog.${plugin.manifest.id}.name`, {
    defaultValue: plugin.manifest.name,
  });
}

function settingRefreshInterval(plugin: PluginRecord): RefreshInterval {
  const interval = plugin.settings.refreshInterval;
  return typeof interval === 'string'
    && refreshIntervalOptions.some((option) => option.value === interval)
    ? interval as RefreshInterval
    : '30s';
}

function actionHasMissingInputs(action: PluginAction, values: PluginInputValues): boolean {
  return action.inputs.some((input) => {
    if (!input.required) return false;
    const value = values[input.id];
    return value === undefined || value === '';
  });
}

export function SessionPluginDock({
  sessionId,
  sessionType,
  open,
  onClose,
  onOpenMarketplace,
}: SessionPluginDockProps) {
  const { t } = useTranslation();
  const {
    plugins,
    operationId,
    error,
    executePluginAction,
    updatePluginSettings,
    clearError,
  } = usePluginStore();
  const compatiblePlugins = useMemo(
    () => plugins.filter((plugin) => isPluginCompatible(plugin, sessionType)),
    [plugins, sessionType]
  );
  const [activePluginId, setActivePluginId] = useState<string | null>(null);
  const [activeActionId, setActiveActionId] = useState<string | null>(null);
  const [inputValues, setInputValues] = useState<PluginInputValues>({});
  const [result, setResult] = useState<PluginExecutionResult | null>(null);
  // When an elevated action's Run button is clicked, capture it here and open
  // the sudo password prompt instead of executing directly.
  const [pendingSudoAction, setPendingSudoAction] = useState<{
    pluginId: string;
    actionId: string;
    trySudo: boolean;
  } | null>(null);
  const sudoInputRef = useRef<HTMLInputElement>(null);
  const requestSequenceRef = useRef(0);

  const activePlugin = compatiblePlugins.find(
    (plugin) => plugin.manifest.id === activePluginId
  ) ?? compatiblePlugins[0] ?? null;
  const actions = activePlugin?.manifest.entry.type === 'commands'
    ? activePlugin.manifest.entry.actions
    : [];
  const activeAction = actions.find((action) => action.id === activeActionId) ?? actions[0] ?? null;
  const executionContext = `${sessionId}:${activePlugin?.manifest.id ?? ''}:${activeAction?.id ?? ''}`;
  const executionContextRef = useRef(executionContext);
  executionContextRef.current = executionContext;
  const parsedTable = activeAction?.output.kind === 'table' && result
    ? parsePluginTable(activeAction, result.output)
    : null;

  useEffect(() => {
    if (!activePlugin || activePlugin.manifest.id === activePluginId) return;
    setActivePluginId(activePlugin.manifest.id);
  }, [activePlugin, activePluginId]);

  useEffect(() => {
    requestSequenceRef.current += 1;
    setActiveActionId(actions[0]?.id ?? null);
    setInputValues({});
    setResult(null);
    setPendingSudoAction(null);
    clearError();
  }, [activePlugin?.manifest.id, sessionId]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (pendingSudoAction) {
      // Belt-and-suspenders focus: autoFocus may miss if the input mounts
      // during a React flush, and we want the password field focused so
      // Enter immediately submits and Escape can be wired next.
      sudoInputRef.current?.focus();
    }
  }, [pendingSudoAction]);

  if (!open) return null;

  const runAction = async (
    pluginId: string,
    actionId: string,
    sudoPassword: string | null,
    trySudo: boolean
  ) => {
    const requestSequence = ++requestSequenceRef.current;
    const requestContext = executionContext;
    const nextResult = await executePluginAction(
      pluginId,
      actionId,
      sessionId,
      inputValues,
      sudoPassword,
      trySudo
    );
    if (
      requestSequence === requestSequenceRef.current
      && requestContext === executionContextRef.current
    ) {
      setResult(nextResult);
    }
  };

  const handleRun = async () => {
    if (!activePlugin || !activeAction) return;
    if (
      activeAction.requiresConfirmation
      && !window.confirm(t('plugins.actionConfirm', { name: activeAction.name }))
    ) {
      return;
    }
    if (activeAction.elevate) {
      // Open the sudo prompt instead of running — the prompt's Submit handler
      // completes the execution so the password never lingers in component
      // state longer than necessary.
      setPendingSudoAction({
        pluginId: activePlugin.manifest.id,
        actionId: activeAction.id,
        trySudo: false,
      });
      return;
    }
    await runAction(activePlugin.manifest.id, activeAction.id, null, false);
  };

  const handleTrySudo = () => {
    if (!activePlugin || !activeAction?.allowSudo) return;
    if (
      activeAction.requiresConfirmation
      && !window.confirm(t('plugins.actionConfirm', { name: activeAction.name }))
    ) {
      return;
    }
    setPendingSudoAction({
      pluginId: activePlugin.manifest.id,
      actionId: activeAction.id,
      trySudo: true,
    });
  };

  const handleSudoSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!pendingSudoAction) return;
    const formData = new FormData(event.currentTarget);
    const password = String(formData.get('sudoPassword') ?? '');
    const target = pendingSudoAction;
    // Clear immediately so the password stays in this form-data scratch space
    // for the minimum amount of time.
    setPendingSudoAction(null);
    await runAction(
      target.pluginId,
      target.actionId,
      password.length > 0 ? password : null,
      target.trySudo
    );
  };

  const handleClose = () => {
    requestSequenceRef.current += 1;
    clearError();
    onClose();
  };

  return (
    <section className="flex-shrink-0 border-t border-tokyo-bg-hl bg-tokyo-bg-dark">
      <div className="flex h-10 items-center border-b border-tokyo-bg-hl px-2">
        <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
          {compatiblePlugins.map((plugin) => (
            <button
              key={plugin.manifest.id}
              className={cn(
                'flex h-8 flex-shrink-0 items-center gap-2 rounded-md px-2.5 text-xs text-tokyo-comment transition-colors hover:bg-tokyo-bg-hl hover:text-tokyo-fg',
                activePlugin?.manifest.id === plugin.manifest.id && 'bg-tokyo-bg-hl text-tokyo-fg'
              )}
              onClick={() => {
                requestSequenceRef.current += 1;
                clearError();
                setActivePluginId(plugin.manifest.id);
              }}
            >
              <PluginIcon name={plugin.manifest.icon} className="h-4 w-4" />
              <span>{localizedPluginName(t, plugin)}</span>
            </button>
          ))}
        </div>
        <div className="ml-2 flex flex-shrink-0 items-center gap-1 border-l border-tokyo-bg-hl pl-2">
          <button
            className="icon-button tooltip-button h-7 w-7"
            data-tooltip={t('plugins.marketplace')}
            aria-label={t('plugins.marketplace')}
            onClick={onOpenMarketplace}
          >
            <Store className="h-4 w-4" />
          </button>
          <button
            className="icon-button tooltip-button h-7 w-7"
            data-tooltip={t('common.close')}
            aria-label={t('common.close')}
            onClick={handleClose}
          >
            <X className="h-4 w-4" />
          </button>
        </div>
      </div>

      {!activePlugin ? (
        <div className="flex h-40 flex-col items-center justify-center gap-3 text-tokyo-comment">
          <p className="text-sm">{t('plugins.noCompatiblePlugins')}</p>
          <button
            className="flex h-8 items-center gap-2 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 text-xs text-tokyo-fg hover:border-tokyo-cyan"
            onClick={onOpenMarketplace}
          >
            <Store className="h-4 w-4" />
            {t('plugins.openMarketplace')}
          </button>
        </div>
      ) : activePlugin.manifest.entry.type === 'native' ? (
        <ServerStatus
          key={`${sessionId}:${activePlugin.manifest.id}`}
          sessionId={sessionId}
          defaultCollapsed={activePlugin.settings.defaultExpanded === false}
          defaultRefreshInterval={settingRefreshInterval(activePlugin)}
          defaultHeight={260}
          minHeight={180}
          maxHeight={420}
          onToggle={(collapsed) => {
            void updatePluginSettings(activePlugin.manifest.id, {
              ...activePlugin.settings,
              defaultExpanded: !collapsed,
            });
          }}
          onRefreshIntervalChange={(refreshInterval) => {
            void updatePluginSettings(activePlugin.manifest.id, {
              ...activePlugin.settings,
              refreshInterval,
            });
          }}
        />
      ) : (
        <div className="flex h-[300px] min-h-0">
          <aside className="w-56 flex-shrink-0 overflow-y-auto border-r border-tokyo-bg-hl p-2">
            {actions.map((action) => (
              <button
                key={action.id}
                className={cn(
                  'mb-1 w-full rounded-md px-3 py-2 text-left transition-colors',
                  activeAction?.id === action.id
                    ? 'bg-tokyo-bg-hl text-tokyo-fg'
                    : 'text-tokyo-comment hover:bg-tokyo-bg-hl/60 hover:text-tokyo-fg'
                )}
                onClick={() => {
                  requestSequenceRef.current += 1;
                  setActiveActionId(action.id);
                  setInputValues({});
                  setResult(null);
                  clearError();
                }}
              >
                <span className="flex items-center gap-1.5 text-xs font-medium">
                  <span className="min-w-0 flex-1 truncate">{action.name}</span>
                  {action.elevate && (
                    <span
                      className="flex items-center gap-0.5 rounded bg-tokyo-orange/20 px-1 text-[9px] font-semibold uppercase text-tokyo-orange"
                      title={t('plugins.elevate')}
                      aria-label={t('plugins.elevate')}
                    >
                      <ShieldCheck className="h-2.5 w-2.5" aria-hidden="true" />
                      {t('plugins.elevateBadge')}
                    </span>
                  )}
                </span>
                <span className="mt-0.5 block overflow-hidden text-[10px] leading-4 opacity-75">
                  {action.description}
                </span>
              </button>
            ))}
          </aside>

          <div className="flex min-w-0 flex-1 flex-col">
            {activeAction && (
              <>
                <div className="flex min-h-[62px] flex-wrap items-center gap-3 border-b border-tokyo-bg-hl px-4 py-2">
                  <div className="mr-auto min-w-[180px]">
                    <div className="flex items-center gap-2">
                      <h3 className="text-sm font-semibold text-tokyo-fg">{activeAction.name}</h3>
                      {activeAction.elevate && (
                        <span
                          className="flex items-center gap-1 rounded bg-tokyo-orange/20 px-1.5 py-0.5 text-[10px] font-semibold uppercase text-tokyo-orange"
                          title={t('plugins.elevate')}
                        >
                          <ShieldCheck className="h-3 w-3" aria-hidden="true" />
                          {t('plugins.elevateBadge')}
                        </span>
                      )}
                    </div>
                    <p className="mt-0.5 text-xs text-tokyo-comment">{activeAction.description}</p>
                  </div>

                  {activeAction.inputs.map((input) => (
                    <label key={input.id} className="min-w-[150px] max-w-[220px] flex-1">
                      <span className="mb-1 block text-[10px] font-medium text-tokyo-comment">
                        {input.label}{input.required ? ' *' : ''}
                      </span>
                      {input.kind === 'select' ? (
                        <select
                          value={String(inputValues[input.id] ?? '')}
                          onChange={(event) => setInputValues((current) => ({
                            ...current,
                            [input.id]: event.target.value,
                          }))}
                          className="h-8 w-full rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2 text-xs text-tokyo-fg outline-none focus:border-tokyo-cyan"
                        >
                          <option value="">{input.placeholder || input.label}</option>
                          {input.options.map((option) => <option key={option}>{option}</option>)}
                        </select>
                      ) : input.kind === 'boolean' ? (
                        <input
                          type="checkbox"
                          checked={Boolean(inputValues[input.id])}
                          onChange={(event) => setInputValues((current) => ({
                            ...current,
                            [input.id]: event.target.checked,
                          }))}
                          className="plugin-toggle-input mt-1"
                        />
                      ) : (
                        <input
                          type={input.kind === 'integer' ? 'number' : 'text'}
                          value={String(inputValues[input.id] ?? '')}
                          placeholder={input.placeholder}
                          onChange={(event) => setInputValues((current) => ({
                            ...current,
                            [input.id]: event.target.value,
                          }))}
                          className="h-8 w-full rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2 text-xs text-tokyo-fg outline-none focus:border-tokyo-cyan"
                        />
                      )}
                    </label>
                  ))}

                  <button
                    className="mt-4 flex h-8 items-center gap-2 rounded-md bg-tokyo-blue px-3 text-xs font-medium text-tokyo-on-accent hover:opacity-90 disabled:opacity-50"
                    onClick={() => void handleRun()}
                    disabled={
                      operationId === `${activePlugin.manifest.id}:${activeAction.id}`
                      || actionHasMissingInputs(activeAction, inputValues)
                    }
                  >
                    {operationId === `${activePlugin.manifest.id}:${activeAction.id}`
                      ? <Loader2 className="h-4 w-4 animate-spin" />
                      : result?.actionId === activeAction.id
                        ? <RefreshCw className="h-4 w-4" />
                        : <Play className="h-4 w-4" />}
                    {result?.actionId === activeAction.id ? t('plugins.runAgain') : t('plugins.run')}
                  </button>
                  {activeAction.allowSudo && (
                    <button
                      className="mt-4 flex h-8 items-center gap-2 rounded-md border border-tokyo-orange/60 bg-tokyo-orange/10 px-3 text-xs font-medium text-tokyo-orange hover:bg-tokyo-orange/20 disabled:opacity-50"
                      onClick={handleTrySudo}
                      disabled={
                        operationId === `${activePlugin.manifest.id}:${activeAction.id}`
                        || actionHasMissingInputs(activeAction, inputValues)
                      }
                    >
                      <ShieldCheck className="h-4 w-4" aria-hidden="true" />
                      {t('plugins.trySudo')}
                    </button>
                  )}
                </div>

                {error && (
                  <div className="mx-4 mt-3 flex items-center gap-2 rounded-md border border-tokyo-red/30 bg-tokyo-red/10 px-3 py-2 text-xs text-tokyo-red">
                    <AlertTriangle className="h-4 w-4 flex-shrink-0" />
                    <span className="min-w-0 flex-1 truncate">{error}</span>
                    <button onClick={clearError}>{t('common.close')}</button>
                  </div>
                )}

                <div className="min-h-0 flex-1 overflow-auto p-3">
                  {!result ? (
                    <div className="flex h-full items-center justify-center text-xs text-tokyo-comment">
                      {t('plugins.readyToRun')}
                    </div>
                  ) : activeAction.output.kind === 'table' ? (
                    <table className="w-full border-separate border-spacing-0 text-left text-xs">
                      <thead className="sticky top-0 bg-tokyo-bg-dark text-tokyo-comment">
                        <tr>
                          {activeAction.output.columns.map((column) => (
                            <th key={column} className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">
                              {column}
                            </th>
                          ))}
                        </tr>
                      </thead>
                      <tbody className="font-mono text-tokyo-fg">
                        {parsedTable?.rows.map((row, rowIndex) => (
                          <tr key={`${result.actionId}:${rowIndex}`} className="hover:bg-tokyo-bg-hl/40">
                            {row.map((cell, cellIndex) => (
                              <td key={cellIndex} className="max-w-[360px] border-b border-tokyo-bg-hl/60 px-3 py-2 align-top">
                                <span className="block truncate" title={cell}>{cell || '-'}</span>
                              </td>
                            ))}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  ) : (
                    <pre className="min-h-full whitespace-pre-wrap break-words rounded-md border border-tokyo-bg-hl bg-tokyo-bg p-3 font-mono text-xs leading-5 text-tokyo-fg">
                      {result.output || t('common.noData')}
                    </pre>
                  )}
                </div>

                {result && (
                  <div className="flex h-7 items-center justify-end gap-3 border-t border-tokyo-bg-hl px-3 text-[10px] text-tokyo-comment">
                    {result.truncated && <span className="text-tokyo-yellow">{t('plugins.outputTruncated')}</span>}
                    {parsedTable?.truncated && <span className="text-tokyo-yellow">{t('plugins.tableRowsTruncated')}</span>}
                    <span>{result.durationMs} ms</span>
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      )}

      {pendingSudoAction && (
        <div
          className="responsive-dialog-layer fixed inset-0 z-[120] flex items-center justify-center bg-tokyo-bg-dark/60 px-3"
          role="dialog"
          aria-modal="true"
          aria-label={t('plugins.elevate')}
        >
          <form
            onSubmit={handleSudoSubmit}
            className="w-full max-w-sm rounded-lg border border-tokyo-bg-hl bg-tokyo-bg-dark p-5 shadow-xl"
          >
            <div className="flex items-center gap-2 text-tokyo-orange">
              <ShieldCheck className="h-5 w-5" aria-hidden="true" />
              <h4 className="text-sm font-semibold">{t('plugins.elevate')}</h4>
            </div>
            <p className="mt-2 text-xs text-tokyo-comment">
              {t('plugins.sudoPasswordOptional')}
            </p>
            <label className="mt-4 block">
              <span className="mb-1 block text-[10px] font-medium text-tokyo-comment">
                {t('plugins.sudoPassword')}
              </span>
              <input
                ref={sudoInputRef}
                name="sudoPassword"
                type="password"
                autoComplete="off"
                autoFocus
                className="h-9 w-full rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 text-sm text-tokyo-fg outline-none focus:border-tokyo-cyan"
              />
            </label>
            <div className="mt-5 flex items-center justify-end gap-2">
              <button
                type="button"
                className="flex h-8 items-center rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 text-xs text-tokyo-fg hover:border-tokyo-comment"
                onClick={() => setPendingSudoAction(null)}
              >
                {t('plugins.sudoCancel')}
              </button>
              <button
                type="submit"
                className="flex h-8 items-center gap-1.5 rounded-md bg-tokyo-orange px-3 text-xs font-medium text-tokyo-bg hover:opacity-90"
              >
                <ShieldCheck className="h-3.5 w-3.5" aria-hidden="true" />
                {t('plugins.sudoSubmit')}
              </button>
            </div>
          </form>
        </div>
      )}
    </section>
  );
}
