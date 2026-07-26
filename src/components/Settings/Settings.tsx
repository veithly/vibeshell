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
} from '../../stores/settingsStore';
import {
  getDefaultAiBaseUrl,
  getDefaultAiModel,
  type AiPredictionProvider,
} from '../../lib/aiCommandPrediction';
import { useFingerprintStore } from '../../stores/fingerprintStore';
import { useUpdateStore } from '../../stores/updateStore';
import { useRuntimeCapabilitiesStore } from '../../stores/runtimeCapabilitiesStore';
import {
  useCloudSyncStore,
  type CloudSyncProvider,
  type CreateCloudSyncVaultInput,
} from '../../stores/cloudSyncStore';
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
  Video,
  Trash2,
  FolderOpen,
  Sparkles,
  Eye,
  EyeOff,
  Bot,
  RefreshCw,
  Cloud,
  Copy,
  LockKeyhole,
  FileDown,
  FileUp,
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
    <section className="mb-7 min-w-0 sm:mb-8">
      <div className="mb-2 flex min-w-0 items-center gap-2 sm:gap-3">
        <span className="flex-shrink-0 text-tokyo-blue">{icon}</span>
        <h2 className="min-w-0 break-words text-lg font-semibold text-tokyo-fg sm:text-xl">{title}</h2>
      </div>
      {description && <p className="mb-4 break-words text-sm text-tokyo-comment sm:ml-8 sm:text-base">{description}</p>}
      <div className="min-w-0 space-y-4 sm:ml-8">{children}</div>
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
    <div className="flex min-w-0 flex-col items-stretch gap-3 border-b border-tokyo-bg-hl py-3 last:border-b-0 sm:flex-row sm:items-center sm:justify-between sm:gap-0">
      <div className="min-w-0 flex-1 sm:mr-4">
        <label className="text-tokyo-fg font-medium">{label}</label>
        {description && <p className="mt-0.5 break-words text-sm text-tokyo-comment">{description}</p>}
      </div>
      <div className="settings-control min-w-0 w-full sm:w-auto sm:flex-shrink-0">{children}</div>
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
    <div className="flex w-full min-w-0 items-center gap-3 sm:w-auto">
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="h-2 min-w-0 flex-1 appearance-none rounded-lg bg-tokyo-bg-hl cursor-pointer sm:w-32 sm:flex-none
                   [&::-webkit-slider-thumb]:appearance-none
                   [&::-webkit-slider-thumb]:w-4
                   [&::-webkit-slider-thumb]:h-4
                   [&::-webkit-slider-thumb]:rounded-full
                   [&::-webkit-slider-thumb]:bg-tokyo-blue
                   [&::-webkit-slider-thumb]:cursor-pointer
                   [&::-webkit-slider-thumb]:hover:bg-tokyo-cyan
                   [&::-webkit-slider-thumb]:transition-colors"
      />
      <span className="w-16 flex-shrink-0 text-right font-mono text-sm text-tokyo-fg">{displayValue}</span>
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
      className="w-full max-w-full px-3 py-2 rounded-md bg-tokyo-bg-hl border border-tokyo-bg-hl sm:w-auto
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
    <div className="flex w-full min-w-0 items-center gap-2 sm:w-auto">
      <input
        type="number"
        value={value}
        min={min}
        max={max}
        onChange={(e) => onChange(Number(e.target.value))}
        placeholder={placeholder}
        className="min-w-0 flex-1 px-3 py-2 rounded-md bg-tokyo-bg-hl border border-tokyo-bg-hl sm:w-24 sm:flex-none
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
  type?: 'text' | 'password' | 'url';
  className?: string;
}

/**
 * A styled text input.
 */
function TextInput({ value, onChange, placeholder, type = 'text', className = 'sm:w-48' }: TextInputProps) {
  return (
    <input
      type={type}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className={`w-full max-w-full ${className} px-3 py-2 rounded-md bg-tokyo-bg-hl border border-tokyo-bg-hl
                 text-tokyo-fg text-sm
                 focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue
                 hover:border-tokyo-comment transition-colors
                 placeholder:text-tokyo-comment`}
    />
  );
}

function SecretInput({ value, onChange, placeholder }: TextInputProps) {
  const [visible, setVisible] = useState(false);

  return (
    <div className="relative w-full sm:w-80">
      <input
        type={visible ? 'text' : 'password'}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="w-full pl-3 pr-10 py-2 rounded-md bg-tokyo-bg-hl border border-tokyo-bg-hl
                   text-tokyo-fg text-sm
                   focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue
                   hover:border-tokyo-comment transition-colors
                   placeholder:text-tokyo-comment"
      />
      <button
        type="button"
        onClick={() => setVisible((current) => !current)}
        className="absolute right-2 top-1/2 -translate-y-1/2 p-1 rounded-md
                   text-tokyo-comment hover:text-tokyo-fg hover:bg-tokyo-selection/40
                   transition-colors"
        aria-label={visible ? 'Hide API key' : 'Show API key'}
      >
        {visible ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
      </button>
    </div>
  );
}

interface TextAreaProps {
  value: string;
  onChange: (value: string) => void;
  onBlur?: () => void;
  placeholder?: string;
}

function TextArea({ value, onChange, onBlur, placeholder }: TextAreaProps) {
  return (
    <textarea
      value={value}
      onChange={(e) => onChange(e.target.value)}
      onBlur={onBlur}
      placeholder={placeholder}
      rows={8}
      className="w-full px-3 py-2 rounded-md bg-tokyo-bg-hl border border-tokyo-bg-hl sm:w-80
                 text-tokyo-fg text-sm font-mono resize-y
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
                     hover:bg-tokyo-selection hover:text-tokyo-fg transition-colors"
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
        <div className="flex min-w-0 items-start gap-2 text-sm text-tokyo-comment font-mono sm:items-center">
          <FolderOpen className="w-4 h-4 flex-shrink-0" />
          <span className="break-all">%APPDATA%/vibeshell/recordings</span>
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
                className="flex min-w-0 items-center justify-between gap-2 p-3 rounded-md bg-tokyo-bg-dark border border-tokyo-bg-hl"
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
    gatewayStatus,
    appVersion,
    appVersionLoaded,
    settings,
    uploadIgnoreConfig,
    loading,
    loadingToolId,
    error,
    initialized,
    fetchAiTools,
    fetchGatewayStatus,
    fetchAppVersion,
    initializeSettings,
    installTo,
    uninstallFrom,
    updateTerminalSettings,
    updateAppearanceSettings,
    updateSshDefaultSettings,
    updateAiPredictionSettings,
    updateUploadIgnoreConfig,
    resetSettings,
  } = useSettingsStore();
  const {
    currentVersion: checkedCurrentVersion,
    latestRelease,
    updateAvailable,
    checking: updateChecking,
    installing: updateInstalling,
    installProgress,
    error: updateError,
    lastCheckedAt,
    checkForUpdates,
    installLatestUpdate,
    openLatestRelease,
  } = useUpdateStore();
  const runtimeCapabilities = useRuntimeCapabilitiesStore((state) => state.capabilities);
  const loadRuntimeCapabilities = useRuntimeCapabilitiesStore((state) => state.load);
  const {
    status: cloudSyncStatus,
    pairingInfo: cloudPairingInfo,
    report: cloudSyncReport,
    fileReport: cloudSyncFileReport,
    loading: cloudSyncLoading,
    error: cloudSyncError,
    refreshStatus: refreshCloudSyncStatus,
    createVault: createCloudSyncVault,
    joinVault: joinCloudSyncVault,
    syncNow: syncCloudNow,
    lock: lockCloudSync,
    exportFile: exportCloudSyncFile,
    importFile: importCloudSyncFile,
  } = useCloudSyncStore();

  const [showResetConfirm, setShowResetConfirm] = useState(false);
  const [cloudProvider, setCloudProvider] = useState<CloudSyncProvider>('github_gist');
  const [cloudWebDavEndpoint, setCloudWebDavEndpoint] = useState('');
  const [cloudGistId, setCloudGistId] = useState('');
  const [cloudToken, setCloudToken] = useState('');
  const [cloudWebDavUsername, setCloudWebDavUsername] = useState('');
  const [cloudWebDavPassword, setCloudWebDavPassword] = useState('');
  const [cloudPairingCode, setCloudPairingCode] = useState('');
  const [pairingCodeCopied, setPairingCodeCopied] = useState(false);
  const [uploadExcludeText, setUploadExcludeText] = useState(
    uploadIgnoreConfig.excludedPaths.join('\n')
  );

  // Load only the settings surfaces supported by the native runtime.
  useEffect(() => {
    void initializeSettings();
    void fetchAppVersion();
    void refreshCloudSyncStatus();
    void loadRuntimeCapabilities().then((capabilities) => {
      if (capabilities.agentGateway) {
        void fetchAiTools();
        void fetchGatewayStatus();
      }
    });
  }, [initializeSettings, fetchAiTools, fetchGatewayStatus, fetchAppVersion, loadRuntimeCapabilities, refreshCloudSyncStatus]);

  useEffect(() => {
    setUploadExcludeText(uploadIgnoreConfig.excludedPaths.join('\n'));
  }, [uploadIgnoreConfig.excludedPaths]);

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

  const handleSaveUploadIgnores = () => {
    updateUploadIgnoreConfig({
      excludedPaths: uploadExcludeText
        .split(/\r?\n|,|;/)
        .map((value) => value.trim())
        .filter(Boolean),
    });
  };

  const handleAiProviderChange = (provider: AiPredictionProvider) => {
    updateAiPredictionSettings({
      provider,
      baseUrl: getDefaultAiBaseUrl(provider),
      model: getDefaultAiModel(provider),
    });
  };

  const handleCreateCloudVault = async () => {
    let input: CreateCloudSyncVaultInput;
    if (cloudProvider === 'github_gist') {
      input = {
        provider: cloudProvider,
        gistId: cloudGistId.trim() || undefined,
        token: cloudToken,
      };
    } else {
      input = {
        provider: cloudProvider,
        endpoint: cloudWebDavEndpoint,
        username: cloudWebDavUsername,
        password: cloudWebDavPassword,
      };
    }
    const created = await createCloudSyncVault(input);
    if (created) {
      setCloudToken('');
      setCloudWebDavPassword('');
    }
    setPairingCodeCopied(false);
  };

  const canCreateCloudVault =
    cloudProvider === 'github_gist'
      ? Boolean(cloudToken.trim())
      : Boolean(cloudWebDavEndpoint.trim());

  const handleJoinCloudVault = async () => {
    const joined = await joinCloudSyncVault(cloudPairingCode);
    if (joined) {
      setCloudPairingCode('');
    }
  };

  const handleCopyPairingCode = async () => {
    if (!cloudPairingInfo) return;
    await navigator.clipboard.writeText(cloudPairingInfo.pairingCode);
    setPairingCodeCopied(true);
  };

  const effectiveAppVersion = appVersion ?? checkedCurrentVersion;
  const handleCheckUpdates = () => {
    checkForUpdates({ force: true, currentVersion: effectiveAppVersion });
  };
  const handleUpdateAction = () => {
    if (updateAvailable) {
      installLatestUpdate();
    } else {
      openLatestRelease();
    }
  };

  const lastCheckedLabel = lastCheckedAt
    ? t('settings.updateLastChecked', {
        time: new Date(lastCheckedAt).toLocaleString(i18n.language),
      })
    : t('settings.updateNotChecked');
  const updateStatus = updateError
    ? t('settings.updateCheckFailed', { message: updateError })
    : updateInstalling
      ? installProgress !== null
        ? t('settings.updateInstallingProgress', { progress: installProgress })
        : t('settings.updateInstalling')
    : updateAvailable && latestRelease
      ? t('settings.updateAvailable', { version: latestRelease.version })
      : latestRelease && effectiveAppVersion
        ? t('settings.updateUpToDate')
        : latestRelease
          ? t('settings.latestVersion', { version: latestRelease.version })
          : t('settings.updateNotChecked');
  const updateStatusClass = updateError
    ? 'text-tokyo-red'
    : updateAvailable
      ? 'text-tokyo-green'
      : 'text-tokyo-comment';

  // Show loading state while settings are initializing
  if (!initialized) {
    return (
      <div className="settings-page mx-auto w-full min-w-0 max-w-4xl overflow-x-hidden p-4 sm:p-6">
        <div className="flex items-center justify-center py-12">
          <div className="inline-block animate-spin rounded-full h-8 w-8 border-b-2 border-tokyo-blue"></div>
          <span className="ml-3 text-tokyo-comment">{t('common.loading')}</span>
        </div>
      </div>
    );
  }

  return (
    <div className="settings-page mx-auto w-full min-w-0 max-w-4xl overflow-x-hidden p-4 sm:p-6">
      <h1 className="mb-7 text-2xl font-bold text-tokyo-fg sm:mb-8">{t('settings.title')}</h1>

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
      {/* Cloud Sync Section */}
      {/* ================================================================== */}
      <SettingsSection
        icon={<Cloud className="w-5 h-5" />}
        title={t('settings.cloudSync')}
        description={t('settings.cloudSyncDesc')}
      >
        {cloudSyncError && (
          <div className="border-y border-tokyo-red/30 bg-tokyo-red/10 px-3 py-2 text-sm text-tokyo-red">
            {cloudSyncError}
          </div>
        )}

        {cloudSyncStatus.unlocked ? (
          <>
            <SettingRow label={t('settings.cloudSyncStatus')}>
              <div className="flex min-w-0 items-center gap-2 text-sm text-tokyo-fg">
                <span className="h-2 w-2 flex-shrink-0 rounded-full bg-tokyo-green" />
                <span>{cloudSyncStatus.syncing ? t('settings.cloudSyncing') : t('settings.cloudUnlocked')}</span>
              </div>
            </SettingRow>
            <SettingRow label={t('settings.cloudVault')}>
              <div className="max-w-full text-right text-xs text-tokyo-comment sm:max-w-sm">
                <div className="text-tokyo-fg">
                  {t(`settings.cloudProvider_${cloudSyncStatus.provider ?? 'github_gist'}`)}
                </div>
                <div className="break-all font-mono text-tokyo-fg">{cloudSyncStatus.vaultId}</div>
                <div className="break-all">{cloudSyncStatus.endpoint}</div>
              </div>
            </SettingRow>
            <SettingRow label={t('settings.cloudPending')}>
              <span className="font-mono text-sm text-tokyo-fg">{cloudSyncStatus.pendingChanges}</span>
            </SettingRow>
            <SettingRow label={t('settings.cloudConflicts')}>
              <span className={`font-mono text-sm ${cloudSyncStatus.conflicts > 0 ? 'text-tokyo-yellow' : 'text-tokyo-fg'}`}>
                {cloudSyncStatus.conflicts}
              </span>
            </SettingRow>
            <SettingRow label={t('settings.cloudLastSync')}>
              <span className="text-sm text-tokyo-comment">
                {cloudSyncStatus.lastSuccessAt
                  ? new Date(cloudSyncStatus.lastSuccessAt * 1000).toLocaleString(i18n.language)
                  : t('settings.cloudNeverSynced')}
              </span>
            </SettingRow>

            {cloudPairingInfo && (
              <div className="space-y-2 border-y border-tokyo-yellow/30 bg-tokyo-yellow/10 px-3 py-3">
                <div className="flex items-center justify-between gap-3">
                  <label htmlFor="cloudPairingOutput" className="text-sm font-medium text-tokyo-fg">
                    {t('settings.cloudPairingCode')}
                  </label>
                  <button
                    type="button"
                    className="icon-button tooltip-button"
                    data-tooltip={pairingCodeCopied ? t('settings.cloudCopied') : t('settings.cloudCopyPairing')}
                    aria-label={pairingCodeCopied ? t('settings.cloudCopied') : t('settings.cloudCopyPairing')}
                    onClick={handleCopyPairingCode}
                  >
                    <Copy className="h-4 w-4" />
                  </button>
                </div>
                <textarea
                  id="cloudPairingOutput"
                  readOnly
                  value={cloudPairingInfo.pairingCode}
                  rows={3}
                  spellCheck={false}
                  className="w-full resize-none rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 py-2 font-mono text-xs text-tokyo-fg focus:outline-none focus:ring-1 focus:ring-tokyo-yellow"
                />
                <p className="text-xs text-tokyo-yellow">{t('settings.cloudPairingCodeWarning')}</p>
              </div>
            )}

            {cloudSyncReport && (
              <p className="text-xs text-tokyo-comment">
                {t('settings.cloudSyncReport', {
                  uploaded: cloudSyncReport.uploaded,
                  downloaded: cloudSyncReport.downloaded,
                  conflicts: cloudSyncReport.conflicts,
                })}
              </p>
            )}

            <div className="flex flex-wrap justify-end gap-2">
              <button
                type="button"
                onClick={() => void lockCloudSync()}
                disabled={cloudSyncLoading}
                className="inline-flex min-h-10 items-center gap-2 rounded-md border border-tokyo-bg-hl px-3 py-2 text-sm text-tokyo-fg transition-colors hover:bg-tokyo-bg-hl disabled:opacity-50"
              >
                <LockKeyhole className="h-4 w-4" />
                {t('settings.cloudLock')}
              </button>
              <button
                type="button"
                onClick={() => void syncCloudNow()}
                disabled={cloudSyncLoading || cloudSyncStatus.syncing}
                className="inline-flex min-h-10 items-center gap-2 rounded-md bg-tokyo-blue px-3 py-2 text-sm text-tokyo-on-accent transition-colors hover:bg-tokyo-blue/80 disabled:opacity-50"
              >
                <RefreshCw className={`h-4 w-4 ${cloudSyncLoading || cloudSyncStatus.syncing ? 'animate-spin' : ''}`} />
                {t('settings.cloudSyncNow')}
              </button>
            </div>
          </>
        ) : (
          <>
            <SettingRow label={t('settings.cloudProvider')}>
              <Select
                value={cloudProvider}
                options={[
                  { value: 'github_gist', label: t('settings.cloudProvider_github_gist') },
                  { value: 'webdav', label: t('settings.cloudProvider_webdav') },
                ]}
                onChange={setCloudProvider}
              />
            </SettingRow>

            {cloudProvider === 'github_gist' && (
              <>
                <SettingRow
                  label={t('settings.cloudGistId')}
                  description={t('settings.cloudGistIdDesc')}
                >
                  <TextInput
                    value={cloudGistId}
                    onChange={setCloudGistId}
                    placeholder="https://gist.github.com/user/id"
                    className="sm:w-80"
                  />
                </SettingRow>
                <SettingRow
                  label={t('settings.cloudGithubToken')}
                  description={t('settings.cloudGithubTokenDesc')}
                >
                  <SecretInput
                    value={cloudToken}
                    onChange={setCloudToken}
                    placeholder="github_pat_..."
                  />
                </SettingRow>
              </>
            )}

            {cloudProvider === 'webdav' && (
              <>
                <SettingRow
                  label={t('settings.cloudWebDavEndpoint')}
                  description={t('settings.cloudWebDavEndpointDesc')}
                >
                  <TextInput
                    value={cloudWebDavEndpoint}
                    onChange={setCloudWebDavEndpoint}
                    placeholder="https://dav.example.com/vibeshell-sync.json"
                    className="sm:w-80"
                  />
                </SettingRow>
                <SettingRow label={t('settings.cloudWebDavUsername')}>
                  <TextInput
                    value={cloudWebDavUsername}
                    onChange={setCloudWebDavUsername}
                    placeholder={t('settings.cloudOptional')}
                    className="sm:w-80"
                  />
                </SettingRow>
                <SettingRow label={t('settings.cloudWebDavPassword')}>
                  <SecretInput
                    value={cloudWebDavPassword}
                    onChange={setCloudWebDavPassword}
                    placeholder={t('settings.cloudOptional')}
                  />
                </SettingRow>
              </>
            )}

            <div className="flex justify-end">
              <button
                type="button"
                onClick={() => void handleCreateCloudVault()}
                disabled={cloudSyncLoading || !canCreateCloudVault}
                className="inline-flex min-h-10 items-center gap-2 rounded-md bg-tokyo-blue px-3 py-2 text-sm text-tokyo-on-accent transition-colors hover:bg-tokyo-blue/80 disabled:opacity-50"
              >
                <Cloud className="h-4 w-4" />
                {t('settings.cloudCreateVault')}
              </button>
            </div>
            <SettingRow label={t('settings.cloudPairingCode')}>
              <TextArea
                value={cloudPairingCode}
                onChange={setCloudPairingCode}
                placeholder="vibeshell-sync-v2..."
              />
            </SettingRow>
            <div className="flex justify-end">
              <button
                type="button"
                onClick={() => void handleJoinCloudVault()}
                disabled={cloudSyncLoading || !cloudPairingCode.trim()}
                className="inline-flex min-h-10 items-center gap-2 rounded-md border border-tokyo-blue px-3 py-2 text-sm text-tokyo-blue transition-colors hover:bg-tokyo-blue/10 disabled:opacity-50"
              >
                <Key className="h-4 w-4" />
                {t('settings.cloudJoinVault')}
              </button>
            </div>
          </>
        )}

        {runtimeCapabilities.directoryTransfer && (
          <div className="space-y-3 border-t border-tokyo-bg-hl pt-4">
            <div>
              <h3 className="text-sm font-medium text-tokyo-fg">{t('settings.cloudPortableFile')}</h3>
              <p className="mt-1 text-xs text-tokyo-comment">{t('settings.cloudPortableFileDesc')}</p>
            </div>
            {cloudSyncFileReport && (
              <p className="break-all text-xs text-tokyo-comment">
                {cloudSyncFileReport.operation === 'export'
                  ? t('settings.cloudFileExportReport', {
                      count: cloudSyncFileReport.exported,
                      path: cloudSyncFileReport.path,
                    })
                  : t('settings.cloudFileImportReport', {
                      applied: cloudSyncFileReport.applied,
                      ignored: cloudSyncFileReport.ignored,
                      conflicts: cloudSyncFileReport.conflicts,
                      path: cloudSyncFileReport.path,
                    })}
              </p>
            )}
            <div className="flex flex-wrap justify-end gap-2">
              <button
                type="button"
                onClick={() => void importCloudSyncFile()}
                disabled={cloudSyncLoading}
                className="inline-flex min-h-10 items-center gap-2 rounded-md border border-tokyo-blue px-3 py-2 text-sm text-tokyo-blue transition-colors hover:bg-tokyo-blue/10 disabled:opacity-50"
              >
                <FileUp className="h-4 w-4" />
                {t('settings.cloudImportFile')}
              </button>
              <button
                type="button"
                onClick={() => void exportCloudSyncFile()}
                disabled={cloudSyncLoading}
                className="inline-flex min-h-10 items-center gap-2 rounded-md border border-tokyo-bg-hl px-3 py-2 text-sm text-tokyo-fg transition-colors hover:bg-tokyo-bg-hl disabled:opacity-50"
              >
                <FileDown className="h-4 w-4" />
                {t('settings.cloudExportFile')}
              </button>
            </div>
          </div>
        )}
      </SettingsSection>

      {/* ================================================================== */}
      {/* AI Command Prediction Section */}
      {/* ================================================================== */}
      <SettingsSection
        icon={<Sparkles className="w-5 h-5" />}
        title={t('settings.aiPrediction')}
        description={t('settings.aiPredictionDesc')}
      >
        <SettingRow label={t('settings.aiPredictionEnabled')} description={t('settings.aiPredictionEnabledDesc')}>
          <Toggle
            checked={settings.aiPrediction.enabled}
            onChange={(enabled) => updateAiPredictionSettings({ enabled })}
          />
        </SettingRow>

        <SettingRow label={t('settings.aiPredictionProvider')} description={t('settings.aiPredictionProviderDesc')}>
          <Select<AiPredictionProvider>
            value={settings.aiPrediction.provider}
            options={[
              { value: 'openai', label: t('settings.aiPredictionProviders.openai') },
              { value: 'claude', label: t('settings.aiPredictionProviders.claude') },
            ]}
            onChange={handleAiProviderChange}
          />
        </SettingRow>

        <SettingRow label={t('settings.aiPredictionApiKey')} description={t('settings.aiPredictionApiKeyDesc')}>
          <SecretInput
            value={settings.aiPrediction.apiKey}
            onChange={(apiKey) => updateAiPredictionSettings({ apiKey })}
            placeholder={settings.aiPrediction.provider === 'claude' ? 'sk-ant-...' : 'sk-...'}
          />
        </SettingRow>

        <SettingRow label={t('settings.aiPredictionBaseUrl')} description={t('settings.aiPredictionBaseUrlDesc')}>
          <TextInput
            type="url"
            value={settings.aiPrediction.baseUrl}
            onChange={(baseUrl) => updateAiPredictionSettings({ baseUrl })}
            placeholder={getDefaultAiBaseUrl(settings.aiPrediction.provider)}
            className="sm:w-80"
          />
        </SettingRow>

        <SettingRow label={t('settings.aiPredictionModel')} description={t('settings.aiPredictionModelDesc')}>
          <TextInput
            value={settings.aiPrediction.model}
            onChange={(model) => updateAiPredictionSettings({ model })}
            placeholder={getDefaultAiModel(settings.aiPrediction.provider)}
            className="sm:w-64"
          />
        </SettingRow>

        <SettingRow label={t('settings.aiPredictionMinChars')} description={t('settings.aiPredictionMinCharsDesc')}>
          <NumberInput
            value={settings.aiPrediction.minChars}
            min={1}
            max={20}
            onChange={(minChars) => updateAiPredictionSettings({ minChars })}
          />
        </SettingRow>

        <SettingRow label={t('settings.aiPredictionDebounce')} description={t('settings.aiPredictionDebounceDesc')}>
          <NumberInput
            value={settings.aiPrediction.debounceMs}
            min={150}
            max={2000}
            onChange={(debounceMs) => updateAiPredictionSettings({ debounceMs })}
            suffix="ms"
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
          <div className="flex flex-wrap gap-3">
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
      {/* File Transfer Section */}
      {/* ================================================================== */}
      {runtimeCapabilities.directoryTransfer && (
        <SettingsSection
          icon={<FolderOpen className="w-5 h-5" />}
          title={t('settings.fileTransfer')}
          description={t('settings.fileTransferDesc')}
        >
        <SettingRow
          label={t('settings.respectGitignore')}
          description={t('settings.respectGitignoreDesc')}
        >
          <Toggle
            checked={uploadIgnoreConfig.respectGitignore}
            onChange={(respectGitignore) => updateUploadIgnoreConfig({ respectGitignore })}
          />
        </SettingRow>

        <SettingRow
          label={t('settings.uploadExcludePatterns')}
          description={t('settings.uploadExcludePatternsDesc')}
        >
          <TextArea
            value={uploadExcludeText}
            onChange={setUploadExcludeText}
            onBlur={handleSaveUploadIgnores}
            placeholder="node_modules/"
          />
        </SettingRow>

        <div className="flex justify-end">
          <button
            onClick={handleSaveUploadIgnores}
            className="px-4 py-2 text-sm rounded-lg bg-tokyo-blue text-tokyo-on-accent
                       hover:bg-tokyo-blue/80 transition-colors"
          >
            {t('common.save')}
          </button>
        </div>
        </SettingsSection>
      )}

      {/* ================================================================== */}
      {/* Security Section */}
      {/* ================================================================== */}
      <SecuritySection />

      {/* ================================================================== */}
      {/* Session Recording Section */}
      {/* ================================================================== */}
      <RecordingSection />

      {/* ================================================================== */}
      {/* Agent Gateway Section */}
      {/* ================================================================== */}
      {runtimeCapabilities.agentGateway && (
        <>
        <SettingsSection
        icon={<Bot className="w-5 h-5" />}
        title={t('settings.gateway')}
        description={t('settings.gatewayDesc')}
      >
        <div className="border-y border-tokyo-bg-hl bg-tokyo-bg-dark/50">
          <div className="flex items-center justify-between gap-4 px-4 py-3">
            <div className="flex min-w-0 items-center gap-2">
              <span className={`h-2 w-2 flex-shrink-0 rounded-full ${gatewayStatus?.running ? 'bg-tokyo-green' : 'bg-tokyo-red'}`} />
              <div>
                <div className="text-sm font-medium text-tokyo-fg">
                  {gatewayStatus?.running ? t('settings.gatewayRunning') : t('settings.gatewayStopped')}
                </div>
                <div className="text-xs text-tokyo-comment">{t('settings.gatewaySharedGui')}</div>
              </div>
            </div>
            <button
              className="icon-button tooltip-button"
              data-tooltip={t('settings.gatewayRefresh')}
              onClick={() => fetchGatewayStatus()}
              aria-label={t('settings.gatewayRefresh')}
            >
              <RefreshCw className="h-4 w-4" />
            </button>
          </div>
          <dl className="divide-y divide-tokyo-bg-hl border-t border-tokyo-bg-hl text-sm">
            <div className="grid gap-1 px-4 py-3 sm:grid-cols-[9rem_minmax(0,1fr)]">
              <dt className="text-tokyo-comment">{t('settings.gatewayEndpoint')}</dt>
              <dd className="break-all font-mono text-xs text-tokyo-fg">{gatewayStatus?.endpoint ?? t('common.loading')}</dd>
            </div>
            <div className="grid gap-1 px-4 py-3 sm:grid-cols-[9rem_minmax(0,1fr)]">
              <dt className="text-tokyo-comment">{t('settings.gatewayManifest')}</dt>
              <dd className="break-all font-mono text-xs text-tokyo-fg">{gatewayStatus?.manifestPath ?? t('common.loading')}</dd>
            </div>
            <div className="grid gap-1 px-4 py-3 sm:grid-cols-[9rem_minmax(0,1fr)]">
              <dt className="text-tokyo-comment">{t('settings.gatewayProtocol')}</dt>
              <dd className="font-mono text-xs text-tokyo-fg">{gatewayStatus?.protocolVersion ?? t('common.loading')}</dd>
            </div>
          </dl>
        </div>
        </SettingsSection>

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
                         hover:bg-tokyo-bg-hl hover:text-tokyo-fg transition-colors
                         disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {loading ? t('settings.refreshing') : t('settings.refreshStatus')}
            </button>
          </div>
        )}
        </SettingsSection>
        </>
      )}

      {/* ================================================================== */}
      {/* About Section */}
      {/* ================================================================== */}
      <SettingsSection
        icon={<Info className="w-5 h-5" />}
        title={t('settings.about')}
        description={t('settings.aboutDesc')}
      >
        <div className="rounded-lg border border-tokyo-bg-hl bg-tokyo-bg-dark p-4 sm:p-6">
          <div className="flex min-w-0 flex-col items-start gap-3 sm:flex-row sm:gap-4">
            <div className="flex-shrink-0 text-3xl font-bold text-tokyo-blue sm:text-4xl">{'>'}_</div>
            <div className="min-w-0 flex-1">
              <h3 className="text-xl font-bold text-tokyo-fg">{t('settings.appTitle')}</h3>
              <p className="text-tokyo-comment mt-1">{t('settings.appSubtitle')}</p>

              <div className="mt-4 space-y-2">
                <div className="flex flex-wrap items-center gap-2 text-sm">
                  <span className="text-tokyo-comment">{t('common.version')}:</span>
                  <span className="text-tokyo-fg font-mono">
                    {appVersion ?? (appVersionLoaded ? t('common.unknown') : t('common.loading'))}
                  </span>
                </div>
                <div className="flex flex-wrap items-center gap-2 text-sm">
                  <span className="text-tokyo-comment">{t('common.builtWith')}:</span>
                  <span className="text-tokyo-fg">Tauri + React + TypeScript</span>
                </div>
              </div>

              {runtimeCapabilities.desktopUpdater && (
                <div className="mt-4 pt-4 border-t border-tokyo-bg-hl">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="min-w-0">
                    <div className="text-sm font-medium text-tokyo-fg">{t('settings.updates')}</div>
                    <div className={`text-sm mt-1 ${updateStatusClass}`}>{updateStatus}</div>
                    <div className="text-xs text-tokyo-comment mt-1">{lastCheckedLabel}</div>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <button
                      type="button"
                      onClick={handleCheckUpdates}
                      disabled={updateChecking || updateInstalling}
                      className="inline-flex items-center gap-2 px-3 py-2 rounded-lg
                                 bg-tokyo-bg-hl text-tokyo-fg text-sm
                                 hover:bg-tokyo-selection hover:text-tokyo-fg transition-colors
                                 disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      <RotateCcw className={`w-4 h-4 ${updateChecking ? 'animate-spin' : ''}`} />
                      <span>
                        {updateChecking ? t('settings.updateChecking') : t('settings.updateCheckNow')}
                      </span>
                    </button>
                    <button
                      type="button"
                      onClick={handleUpdateAction}
                      disabled={updateInstalling}
                      className="inline-flex items-center gap-2 px-3 py-2 rounded-lg
                                 bg-tokyo-blue text-tokyo-on-accent text-sm
                                 hover:bg-tokyo-blue/80 transition-colors
                                 disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      <ExternalLink className={`w-4 h-4 ${updateInstalling ? 'animate-pulse' : ''}`} />
                      <span>
                        {updateInstalling
                          ? t('settings.updateInstalling')
                          : updateAvailable
                            ? t('settings.updateInstall')
                            : t('settings.updateOpenLatest')}
                      </span>
                    </button>
                  </div>
                </div>
                </div>
              )}

              <div className="mt-6 flex flex-wrap gap-3">
                <a
                  href="https://github.com/veithly/vibeshell"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-2 px-4 py-2 rounded-lg
                             bg-tokyo-bg-hl text-tokyo-fg text-sm
                             hover:bg-tokyo-selection hover:text-tokyo-fg transition-colors"
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
          <div className="flex flex-col items-stretch gap-4 sm:flex-row sm:items-center sm:justify-between">
            <div className="min-w-0">
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
              <div className="flex flex-wrap items-center gap-2">
                <span className="mr-2 text-sm text-tokyo-comment">{t('settings.resetConfirm')}</span>
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
