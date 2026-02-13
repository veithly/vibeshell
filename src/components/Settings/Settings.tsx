import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  useSettingsStore,
  themes,
  fontFamilies,
  cursorStyles,
  type FontFamily,
  type CursorStyle,
  type ThemeName,
  type ServerStatusRefreshInterval,
} from '../../stores/settingsStore';
import { useFingerprintStore } from '../../stores/fingerprintStore';
import { IntegrationCard } from './IntegrationCard';
import {
  Monitor,
  Palette,
  Terminal,
  Plug,
  Info,
  RotateCcw,
  ExternalLink,
  Github,
  Shield,
  Key,
  Activity,
  Video,
  Trash2,
  FolderOpen,
} from 'lucide-react';
import { useRecordingStore } from '../../stores/recordingStore';
import type { Recording } from '../../types/tunnel';

// ============================================================================
// Reusable Form Components
// ============================================================================

interface SettingsSectionProps {
  icon: React.ReactNode;
  title: string;
  description?: string;
  children: React.ReactNode;
}

/**
 * A collapsible section container for settings groups.
 */
function SettingsSection({ icon, title, description, children }: SettingsSectionProps) {
  return (
    <section className="mb-8">
      <div className="flex items-center gap-3 mb-2">
        <span className="text-tokyo-blue">{icon}</span>
        <h2 className="text-xl font-semibold text-white">{title}</h2>
      </div>
      {description && <p className="text-tokyo-comment mb-4 ml-9">{description}</p>}
      <div className="ml-9 space-y-4">{children}</div>
    </section>
  );
}

interface SettingRowProps {
  label: string;
  description?: string;
  children: React.ReactNode;
}

/**
 * A single setting row with label and control.
 */
function SettingRow({ label, description, children }: SettingRowProps) {
  return (
    <div className="flex items-center justify-between py-3 border-b border-tokyo-bg-hl last:border-b-0">
      <div className="flex-1 mr-4">
        <label className="text-tokyo-fg font-medium">{label}</label>
        {description && <p className="text-tokyo-comment text-sm mt-0.5">{description}</p>}
      </div>
      <div className="flex-shrink-0">{children}</div>
    </div>
  );
}

interface SliderProps {
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
  formatValue?: (value: number) => string;
}

/**
 * A styled range slider with value display.
 */
function Slider({ value, min, max, step = 1, onChange, formatValue }: SliderProps) {
  const displayValue = formatValue ? formatValue(value) : String(value);

  return (
    <div className="flex items-center gap-3">
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-32 h-2 bg-tokyo-bg-hl rounded-lg appearance-none cursor-pointer
                   [&::-webkit-slider-thumb]:appearance-none
                   [&::-webkit-slider-thumb]:w-4
                   [&::-webkit-slider-thumb]:h-4
                   [&::-webkit-slider-thumb]:rounded-full
                   [&::-webkit-slider-thumb]:bg-tokyo-blue
                   [&::-webkit-slider-thumb]:cursor-pointer
                   [&::-webkit-slider-thumb]:hover:bg-tokyo-cyan
                   [&::-webkit-slider-thumb]:transition-colors"
      />
      <span className="text-tokyo-fg text-sm w-16 text-right font-mono">{displayValue}</span>
    </div>
  );
}

interface SelectProps<T extends string> {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}

/**
 * A styled select dropdown.
 */
function Select<T extends string>({ value, options, onChange }: SelectProps<T>) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value as T)}
      className="px-3 py-2 rounded-md bg-tokyo-bg-hl border border-tokyo-bg-hl
                 text-tokyo-fg text-sm cursor-pointer
                 focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue
                 hover:border-tokyo-comment transition-colors"
    >
      {options.map((option) => (
        <option key={option.value} value={option.value}>
          {option.label}
        </option>
      ))}
    </select>
  );
}

interface ToggleProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
}

/**
 * A styled toggle switch.
 */
function Toggle({ checked, onChange }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors
                  focus:outline-none focus:ring-2 focus:ring-tokyo-blue focus:ring-offset-2 focus:ring-offset-tokyo-bg
                  ${checked ? 'bg-tokyo-blue' : 'bg-tokyo-bg-hl'}`}
    >
      <span
        className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform
                    ${checked ? 'translate-x-6' : 'translate-x-1'}`}
      />
    </button>
  );
}

interface NumberInputProps {
  value: number;
  min?: number;
  max?: number;
  onChange: (value: number) => void;
  placeholder?: string;
  suffix?: string;
}

/**
 * A styled number input.
 */
function NumberInput({ value, min, max, onChange, placeholder, suffix }: NumberInputProps) {
  return (
    <div className="flex items-center gap-2">
      <input
        type="number"
        value={value}
        min={min}
        max={max}
        onChange={(e) => onChange(Number(e.target.value))}
        placeholder={placeholder}
        className="w-24 px-3 py-2 rounded-md bg-tokyo-bg-hl border border-tokyo-bg-hl
                   text-tokyo-fg text-sm font-mono
                   focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue
                   hover:border-tokyo-comment transition-colors"
      />
      {suffix && <span className="text-tokyo-comment text-sm">{suffix}</span>}
    </div>
  );
}

interface TextInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}

/**
 * A styled text input.
 */
function TextInput({ value, onChange, placeholder }: TextInputProps) {
  return (
    <input
      type="text"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className="w-48 px-3 py-2 rounded-md bg-tokyo-bg-hl border border-tokyo-bg-hl
                 text-tokyo-fg text-sm
                 focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue
                 hover:border-tokyo-comment transition-colors
                 placeholder:text-tokyo-comment"
    />
  );
}

// ============================================================================
// Theme Preview Component
// ============================================================================

interface ThemePreviewProps {
  theme: ThemeName;
  selected: boolean;
  onClick: () => void;
}

function ThemePreview({ theme, selected, onClick }: ThemePreviewProps) {
  const themeData = themes.find((t) => t.name === theme);
  if (!themeData) return null;

  return (
    <button
      onClick={onClick}
      className={`relative p-1 rounded-lg transition-all ${
        selected
          ? 'ring-2 ring-tokyo-blue ring-offset-2 ring-offset-tokyo-bg'
          : 'hover:ring-1 hover:ring-tokyo-comment'
      }`}
    >
      <div
        className="w-20 h-14 rounded-md overflow-hidden"
        style={{ backgroundColor: themeData.colors.bgDark }}
      >
        {/* Mini terminal preview */}
        <div className="h-3 flex items-center px-1.5 gap-1" style={{ backgroundColor: themeData.colors.bgDark }}>
          <div className="w-1.5 h-1.5 rounded-full bg-red-500" />
          <div className="w-1.5 h-1.5 rounded-full bg-yellow-500" />
          <div className="w-1.5 h-1.5 rounded-full bg-green-500" />
        </div>
        <div className="p-1.5" style={{ backgroundColor: themeData.colors.bg }}>
          <div className="flex items-center gap-1">
            <span className="text-[6px]" style={{ color: themeData.colors.accent }}>$</span>
            <div className="w-6 h-1 rounded" style={{ backgroundColor: themeData.colors.fg }} />
          </div>
          <div className="mt-0.5 w-10 h-1 rounded" style={{ backgroundColor: themeData.colors.fgDark }} />
        </div>
      </div>
      <span className="block text-xs text-tokyo-fg mt-1 text-center">{themeData.displayName}</span>
    </button>
  );
}

// ============================================================================
// Security Section Component
// ============================================================================

/**
 * Security settings section for managing SSH host keys and other security features.
 */
function SecuritySection() {
  const { t } = useTranslation();
  const { fingerprints, openManager, fetchFingerprints } = useFingerprintStore();

  // Fetch fingerprints on mount to show count
  useEffect(() => {
    fetchFingerprints();
  }, [fetchFingerprints]);

  return (
    <SettingsSection
      icon={<Shield className="w-5 h-5" />}
      title={t('settings.security')}
      description={t('settings.securityDesc')}
    >
      <SettingRow
        label={t('settings.hostKeyFingerprints')}
        description={t('settings.hostKeyFingerprintsDesc', { count: fingerprints.length })}
      >
        <button
          onClick={openManager}
          className="inline-flex items-center gap-2 px-4 py-2 rounded-lg
                     bg-tokyo-bg-hl text-tokyo-fg text-sm
                     hover:bg-tokyo-selection hover:text-white transition-colors"
        >
          <Key className="w-4 h-4" />
          <span>{t('settings.manageHostKeys')}</span>
        </button>
      </SettingRow>
    </SettingsSection>
  );
}

// ============================================================================
// Session Recording Section Component
// ============================================================================

function RecordingSection() {
  const { t } = useTranslation();
  const { recordings, fetchRecordings, deleteRecording } = useRecordingStore();

  useEffect(() => {
    fetchRecordings();
  }, [fetchRecordings]);

  const formatDate = (ts: number) => {
    return new Date(ts * 1000).toLocaleString();
  };

  const formatDuration = (rec: Recording) => {
    if (!rec.endedAt) return t('settings.inProgress');
    const secs = rec.endedAt - rec.startedAt;
    const mins = Math.floor(secs / 60);
    const hours = Math.floor(mins / 60);
    if (hours > 0) return `${hours}h ${mins % 60}m`;
    if (mins > 0) return `${mins}m ${secs % 60}s`;
    return `${secs}s`;
  };

  return (
    <SettingsSection
      icon={<Video className="w-5 h-5" />}
      title={t('settings.recording')}
      description={t('settings.recordingDesc')}
    >
      <SettingRow
        label={t('settings.storageLocation')}
        description={t('settings.storageLocationDesc')}
      >
        <div className="flex items-center gap-2 text-sm text-tokyo-comment font-mono">
          <FolderOpen className="w-4 h-4" />
          <span>%APPDATA%/vibeshell/recordings</span>
        </div>
      </SettingRow>

      <div className="mt-4">
        <h3 className="text-tokyo-fg font-medium mb-3">
          {t('settings.recordedSessions', { count: recordings.length })}
        </h3>
        {recordings.length === 0 ? (
          <p className="text-tokyo-comment text-sm py-4 text-center">
            {t('settings.noRecordings')}
          </p>
        ) : (
          <div className="space-y-2 max-h-64 overflow-y-auto">
            {recordings.map((rec) => (
              <div
                key={rec.id}
                className="flex items-center justify-between p-3 rounded-md bg-tokyo-bg-dark border border-tokyo-bg-hl"
              >
                <div className="flex-1 min-w-0">
                  <div className="text-sm text-tokyo-fg truncate">{rec.filePath.split(/[/\\]/).pop()}</div>
                  <div className="text-xs text-tokyo-comment mt-0.5">
                    {formatDate(rec.startedAt)} &middot; {formatDuration(rec)}
                  </div>
                </div>
                <button
                  onClick={() => deleteRecording(rec.id)}
                  className="flex-shrink-0 p-1.5 rounded-md text-tokyo-comment hover:text-tokyo-red hover:bg-tokyo-red/10 transition-colors"
                  title={t('settings.deleteRecording')}
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </SettingsSection>
  );
}

// ============================================================================
// Main Settings Component
// ============================================================================

/**
 * Comprehensive Settings page with Terminal, Appearance, SSH, AI Tools, and About sections.
 */
export function Settings() {
  const { t, i18n } = useTranslation();
  const {
    aiTools,
    settings,
    loading,
    loadingToolId,
    error,
    initialized,
    fetchAiTools,
    initializeSettings,
    installTo,
    uninstallFrom,
    updateTerminalSettings,
    updateAppearanceSettings,
    updateSshDefaultSettings,
    updateServerStatusSettings,
    resetSettings,
  } = useSettingsStore();

  const [showResetConfirm, setShowResetConfirm] = useState(false);

  // Initialize settings and fetch AI tools on mount
  useEffect(() => {
    initializeSettings();
    fetchAiTools();
  }, [initializeSettings, fetchAiTools]);

  const handleInstall = (toolId: string) => {
    installTo(toolId);
  };

  const handleUninstall = (toolId: string) => {
    uninstallFrom(toolId);
  };

  const handleResetSettings = () => {
    resetSettings();
    setShowResetConfirm(false);
  };

  // Show loading state while settings are initializing
  if (!initialized) {
    return (
      <div className="p-6 max-w-4xl mx-auto">
        <div className="flex items-center justify-center py-12">
          <div className="inline-block animate-spin rounded-full h-8 w-8 border-b-2 border-tokyo-blue"></div>
          <span className="ml-3 text-tokyo-comment">{t('common.loading')}</span>
        </div>
      </div>
    );
  }

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <h1 className="text-2xl font-bold text-white mb-8">{t('settings.title')}</h1>

      {/* ================================================================== */}
      {/* Terminal Settings Section */}
      {/* ================================================================== */}
      <SettingsSection
        icon={<Terminal className="w-5 h-5" />}
        title={t('settings.terminal')}
        description={t('settings.terminalDesc')}
      >
        <SettingRow label={t('settings.fontSize')} description={t('settings.fontSizeDesc')}>
          <Slider
            value={settings.terminal.fontSize}
            min={10}
            max={24}
            onChange={(fontSize) => updateTerminalSettings({ fontSize })}
            formatValue={(v) => `${v}px`}
          />
        </SettingRow>

        <SettingRow label={t('settings.fontFamily')} description={t('settings.fontFamilyDesc')}>
          <Select<FontFamily>
            value={settings.terminal.fontFamily}
            options={fontFamilies}
            onChange={(fontFamily) => updateTerminalSettings({ fontFamily })}
          />
        </SettingRow>

        <SettingRow label={t('settings.cursorStyle')} description={t('settings.cursorStyleDesc')}>
          <Select<CursorStyle>
            value={settings.terminal.cursorStyle}
            options={cursorStyles}
            onChange={(cursorStyle) => updateTerminalSettings({ cursorStyle })}
          />
        </SettingRow>

        <SettingRow label={t('settings.cursorBlink')} description={t('settings.cursorBlinkDesc')}>
          <Toggle
            checked={settings.terminal.cursorBlink}
            onChange={(cursorBlink) => updateTerminalSettings({ cursorBlink })}
          />
        </SettingRow>

        <SettingRow label={t('settings.scrollbackLines')} description={t('settings.scrollbackLinesDesc')}>
          <Slider
            value={settings.terminal.scrollbackLines}
            min={1000}
            max={100000}
            step={1000}
            onChange={(scrollbackLines) => updateTerminalSettings({ scrollbackLines })}
            formatValue={(v) => v.toLocaleString()}
          />
        </SettingRow>
      </SettingsSection>

      {/* ================================================================== */}
      {/* Appearance Settings Section */}
      {/* ================================================================== */}
      <SettingsSection
        icon={<Palette className="w-5 h-5" />}
        title={t('settings.appearance')}
        description={t('settings.appearanceDesc')}
      >
        <SettingRow label={t('settings.theme')} description={t('settings.themeDesc')}>
          <div className="flex gap-3">
            {themes.map((theme) => (
              <ThemePreview
                key={theme.name}
                theme={theme.name}
                selected={settings.appearance.theme === theme.name}
                onClick={() => updateAppearanceSettings({ theme: theme.name })}
              />
            ))}
          </div>
        </SettingRow>

        <SettingRow label={t('settings.windowOpacity')} description={t('settings.windowOpacityDesc')}>
          <Slider
            value={settings.appearance.windowOpacity}
            min={0.5}
            max={1.0}
            step={0.05}
            onChange={(windowOpacity) => updateAppearanceSettings({ windowOpacity })}
            formatValue={(v) => `${Math.round(v * 100)}%`}
          />
        </SettingRow>

        <SettingRow label={t('settings.language')} description={t('settings.languageDesc')}>
          <Select<string>
            value={i18n.language.startsWith('zh') ? 'zh' : 'en'}
            options={[
              { value: 'en', label: 'English' },
              { value: 'zh', label: '简体中文' },
            ]}
            onChange={(lang) => i18n.changeLanguage(lang)}
          />
        </SettingRow>
      </SettingsSection>

      {/* ================================================================== */}
      {/* SSH Defaults Section */}
      {/* ================================================================== */}
      <SettingsSection
        icon={<Monitor className="w-5 h-5" />}
        title={t('settings.sshDefaults')}
        description={t('settings.sshDefaultsDesc')}
      >
        <SettingRow label={t('settings.defaultPort')} description={t('settings.defaultPortDesc')}>
          <NumberInput
            value={settings.sshDefaults.defaultPort}
            min={1}
            max={65535}
            onChange={(defaultPort) => updateSshDefaultSettings({ defaultPort })}
          />
        </SettingRow>

        <SettingRow label={t('settings.connectionTimeout')} description={t('settings.connectionTimeoutDesc')}>
          <NumberInput
            value={settings.sshDefaults.connectionTimeout}
            min={5}
            max={120}
            onChange={(connectionTimeout) => updateSshDefaultSettings({ connectionTimeout })}
            suffix="seconds"
          />
        </SettingRow>

        <SettingRow label={t('settings.keepaliveInterval')} description={t('settings.keepaliveIntervalDesc')}>
          <NumberInput
            value={settings.sshDefaults.keepaliveInterval}
            min={0}
            max={300}
            onChange={(keepaliveInterval) => updateSshDefaultSettings({ keepaliveInterval })}
            suffix="seconds"
          />
        </SettingRow>

        <SettingRow label={t('settings.defaultUsername')} description={t('settings.defaultUsernameDesc')}>
          <TextInput
            value={settings.sshDefaults.defaultUsername}
            onChange={(defaultUsername) => updateSshDefaultSettings({ defaultUsername })}
            placeholder="e.g., root"
          />
        </SettingRow>
      </SettingsSection>

      {/* ================================================================== */}
      {/* Server Status Monitoring Section */}
      {/* ================================================================== */}
      <SettingsSection
        icon={<Activity className="w-5 h-5" />}
        title={t('settings.serverStatus')}
        description={t('settings.serverStatusDesc')}
      >
        <SettingRow label={t('settings.refreshInterval')} description={t('settings.refreshIntervalDesc')}>
          <Select<ServerStatusRefreshInterval>
            value={settings.serverStatus.refreshInterval}
            options={[
              { value: '5s', label: t('settings.refreshIntervalOptions.5s') },
              { value: '10s', label: t('settings.refreshIntervalOptions.10s') },
              { value: '30s', label: t('settings.refreshIntervalOptions.30s') },
              { value: '1m', label: t('settings.refreshIntervalOptions.1m') },
              { value: '5m', label: t('settings.refreshIntervalOptions.5m') },
              { value: 'manual', label: t('settings.refreshIntervalOptions.manual') },
            ]}
            onChange={(refreshInterval) => updateServerStatusSettings({ refreshInterval })}
          />
        </SettingRow>

        <SettingRow label={t('settings.autoExpandPanel')} description={t('settings.autoExpandPanelDesc')}>
          <Toggle
            checked={settings.serverStatus.defaultExpanded}
            onChange={(defaultExpanded) => updateServerStatusSettings({ defaultExpanded })}
          />
        </SettingRow>

        <SettingRow label={t('settings.showNetworkRates')} description={t('settings.showNetworkRatesDesc')}>
          <Toggle
            checked={settings.serverStatus.showNetworkRates}
            onChange={(showNetworkRates) => updateServerStatusSettings({ showNetworkRates })}
          />
        </SettingRow>
      </SettingsSection>

      {/* ================================================================== */}
      {/* Security Section */}
      {/* ================================================================== */}
      <SecuritySection />

      {/* ================================================================== */}
      {/* Session Recording Section */}
      {/* ================================================================== */}
      <RecordingSection />

      {/* ================================================================== */}
      {/* Skills Section */}
      {/* ================================================================== */}
      <SettingsSection
        icon={<Plug className="w-5 h-5" />}
        title={t('settings.skills')}
        description={t('settings.skillsDesc')}
      >
        {/* Error Display */}
        {error && (
          <div className="mb-4 p-4 rounded-lg bg-tokyo-red/10 border border-tokyo-red/30 text-tokyo-red">
            <p className="font-medium">{t('common.error')}</p>
            <p className="text-sm opacity-80">{error}</p>
          </div>
        )}

        {/* Loading State */}
        {loading && aiTools.length === 0 && (
          <div className="text-center py-8">
            <div className="inline-block animate-spin rounded-full h-8 w-8 border-b-2 border-tokyo-blue"></div>
            <p className="mt-2 text-tokyo-comment">{t('settings.detectingTools')}</p>
          </div>
        )}

        {/* Integration Cards Grid */}
        {aiTools.length > 0 && (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {aiTools.map((tool) => (
              <IntegrationCard
                key={tool.id}
                tool={tool}
                onInstall={handleInstall}
                onUninstall={handleUninstall}
                loading={loadingToolId === tool.id}
              />
            ))}
          </div>
        )}

        {/* Empty State */}
        {!loading && aiTools.length === 0 && !error && (
          <div className="text-center py-8 text-tokyo-comment">
            <p>{t('settings.noToolsDetected')}</p>
            <p className="text-sm mt-2">{t('settings.supportedTools')}</p>
          </div>
        )}

        {/* Refresh Button */}
        {aiTools.length > 0 && (
          <div className="mt-4 flex justify-end">
            <button
              onClick={() => fetchAiTools()}
              disabled={loading}
              className="px-4 py-2 text-sm rounded-lg border border-tokyo-bg-hl text-tokyo-fg
                         hover:bg-tokyo-bg-hl hover:text-white transition-colors
                         disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {loading ? t('settings.refreshing') : t('settings.refreshStatus')}
            </button>
          </div>
        )}
      </SettingsSection>

      {/* ================================================================== */}
      {/* About Section */}
      {/* ================================================================== */}
      <SettingsSection
        icon={<Info className="w-5 h-5" />}
        title={t('settings.about')}
        description={t('settings.aboutDesc')}
      >
        <div className="bg-tokyo-bg-dark rounded-lg p-6 border border-tokyo-bg-hl">
          <div className="flex items-start gap-4">
            <div className="text-4xl text-tokyo-blue font-bold">{'>'}_</div>
            <div className="flex-1">
              <h3 className="text-xl font-bold text-white">{t('settings.appTitle')}</h3>
              <p className="text-tokyo-comment mt-1">{t('settings.appSubtitle')}</p>

              <div className="mt-4 space-y-2">
                <div className="flex items-center gap-2 text-sm">
                  <span className="text-tokyo-comment">{t('common.version')}:</span>
                  <span className="text-tokyo-fg font-mono">0.1.0</span>
                </div>
                <div className="flex items-center gap-2 text-sm">
                  <span className="text-tokyo-comment">{t('common.builtWith')}:</span>
                  <span className="text-tokyo-fg">Tauri + React + TypeScript</span>
                </div>
              </div>

              <div className="mt-6 flex gap-3">
                <a
                  href="https://github.com/veithly/vibeshell"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-2 px-4 py-2 rounded-lg
                             bg-tokyo-bg-hl text-tokyo-fg text-sm
                             hover:bg-tokyo-selection hover:text-white transition-colors"
                >
                  <Github className="w-4 h-4" />
                  <span>GitHub</span>
                  <ExternalLink className="w-3 h-3" />
                </a>
              </div>
            </div>
          </div>
        </div>

        {/* Reset Settings */}
        <div className="mt-6 pt-6 border-t border-tokyo-bg-hl">
          <div className="flex items-center justify-between">
            <div>
              <h4 className="text-tokyo-fg font-medium">{t('settings.resetSettings')}</h4>
              <p className="text-tokyo-comment text-sm mt-0.5">
                {t('settings.resetSettingsDesc')}
              </p>
            </div>
            {!showResetConfirm ? (
              <button
                onClick={() => setShowResetConfirm(true)}
                className="inline-flex items-center gap-2 px-4 py-2 rounded-lg
                           border border-tokyo-red/50 text-tokyo-red text-sm
                           hover:bg-tokyo-red/10 transition-colors"
              >
                <RotateCcw className="w-4 h-4" />
                <span>{t('settings.resetAll')}</span>
              </button>
            ) : (
              <div className="flex items-center gap-2">
                <span className="text-tokyo-comment text-sm mr-2">{t('settings.resetConfirm')}</span>
                <button
                  onClick={handleResetSettings}
                  className="px-3 py-1.5 rounded-md bg-tokyo-red text-white text-sm
                             hover:bg-tokyo-red/80 transition-colors"
                >
                  {t('settings.yesReset')}
                </button>
                <button
                  onClick={() => setShowResetConfirm(false)}
                  className="px-3 py-1.5 rounded-md border border-tokyo-bg-hl text-tokyo-fg text-sm
                             hover:bg-tokyo-bg-hl transition-colors"
                >
                  {t('common.cancel')}
                </button>
              </div>
            )}
          </div>
        </div>
      </SettingsSection>
    </div>
  );
}
