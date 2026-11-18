import { Monitor, Moon, Sun } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { isLightTheme, useSettingsStore } from '../../stores/settingsStore';

/** Themes belong to settings, not the workspace's operational menu. */
export function AppearanceModeControl() {
  const { t } = useTranslation();
  const appearance = useSettingsStore((state) => state.settings.appearance);
  const update = useSettingsStore((state) => state.updateAppearanceSettings);
  const selected = appearance.themeMode === 'system' ? 'system' : isLightTheme(appearance.theme) ? 'light' : 'dark';
  return (
    <div className="appearance-modes" role="group" aria-label={t('settings.themeMode')}>
      {([
        { id: 'light', label: 'settings.dayMode', icon: Sun, theme: appearance.lightTheme },
        { id: 'dark', label: 'settings.nightMode', icon: Moon, theme: appearance.darkTheme },
        { id: 'system', label: 'settings.themeModeSystem', icon: Monitor, theme: appearance.theme },
      ] as const).map(({ id, label, icon: Icon, theme }) => (
        <button key={id} type="button" aria-pressed={selected === id} onClick={() => {
          void update(id === 'system' ? { themeMode: 'system' } : { themeMode: 'manual', theme });
        }}>
          <Icon className="h-4 w-4" aria-hidden="true" />
          <span>{t(label)}</span>
        </button>
      ))}
    </div>
  );
}
