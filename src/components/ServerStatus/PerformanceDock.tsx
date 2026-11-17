import { useTranslation } from 'react-i18next';
import { ServerStatus, refreshIntervalOptions, type RefreshInterval } from './index';
import { isPluginCompatible } from '../../plugins/pluginUtils';
import type { PluginRecord } from '../../plugins/types';
import type { Session } from '../../stores/sessionStore';
import { usePluginStore } from '../../stores/pluginStore';

/** A secondary pane, never a replacement for the active terminal. */
export function PerformanceDock({ session, plugin, onClose }: {
  session: Session;
  plugin: PluginRecord | undefined;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const updatePluginSettings = usePluginStore((state) => state.updatePluginSettings);
  if (!plugin || !isPluginCompatible(plugin, session.sessionType)) return null;
  const interval = refreshIntervalOptions.some((item) => item.value === plugin.settings.refreshInterval)
    ? plugin.settings.refreshInterval as RefreshInterval : '30s';

  return (
    <section className="performance-dock glass-panel glass-form" aria-label={t('sidebar.performance')} data-testid="performance-dock">
      <ServerStatus
        sessionId={session.id}
        title={t('sidebar.performance')}
        sessionName={session.serverName}
        defaultCollapsed={false}
        defaultRefreshInterval={interval}
        defaultHeight={230}
        minHeight={150}
        maxHeight={420}
        onClose={onClose}
        onRefreshIntervalChange={(refreshInterval) => {
          void updatePluginSettings(plugin.manifest.id, { ...plugin.settings, refreshInterval });
        }}
      />
    </section>
  );
}
