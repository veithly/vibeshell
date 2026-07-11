import { create } from 'zustand';
import { invokeOrThrow } from '../lib/tauri';
import {
  defaultAiPredictionSettings,
  normalizeAiPredictionSettings,
  type AiPredictionSettings,
} from '../lib/aiCommandPrediction';

// ============================================================================
// Types
// ============================================================================

/**
 * Represents an AI coding tool that can be configured with VibeShell MCP.
 */
export interface AiTool {
  /** Unique identifier for the tool (e.g., "claude-code", "cursor") */
  id: string;
  /** Human-readable name of the tool */
  name: string;
  /** Path to the tool's MCP configuration file */
  configPath: string;
  /** Whether the AI tool is installed on the system */
  installed: boolean;
  /** Whether VibeShell is configured in the tool */
  vibeshellInstalled: boolean;
}

/** Status of the Agent Gateway hosted by the VibeShell GUI. */
export interface AgentGatewayStatus {
  running: boolean;
  endpoint: string | null;
  manifestPath: string;
  pid: number | null;
  protocolVersion: string;
}

/**
 * Available font families for the terminal.
 */
export type FontFamily = 'JetBrains Mono' | 'Fira Code' | 'Consolas' | 'Monaco';

/**
 * Available cursor styles for the terminal.
 */
export type CursorStyle = 'block' | 'underline' | 'bar';

/**
 * Available color themes.
 */
export type ThemeName = 'paper-white' | 'warm-ivory' | 'ink-black' | 'violet-black' | 'cyan-black';

/**
 * Terminal-related settings.
 */
export interface TerminalSettings {
  /** Font size in pixels (10-24) */
  fontSize: number;
  /** Font family for the terminal */
  fontFamily: FontFamily;
  /** Cursor style */
  cursorStyle: CursorStyle;
  /** Whether the cursor should blink */
  cursorBlink: boolean;
  /** Number of lines to keep in scrollback buffer (1000-100000) */
  scrollbackLines: number;
}

/**
 * Appearance-related settings.
 */
export interface AppearanceSettings {
  /** Color theme name */
  theme: ThemeName;
  /** Window opacity (0.5-1.0, Windows only) */
  windowOpacity: number;
}

/**
 * SSH connection default settings.
 */
export interface SshDefaultSettings {
  /** Default SSH port */
  defaultPort: number;
  /** Connection timeout in seconds */
  connectionTimeout: number;
  /** Keepalive interval in seconds (0 to disable) */
  keepaliveInterval: number;
  /** Default username for new connections */
  defaultUsername: string;
}

/**
 * Server status monitoring refresh interval options.
 */
export type ServerStatusRefreshInterval = '5s' | '10s' | '30s' | '1m' | '5m' | 'manual';

/**
 * Server status monitoring settings.
 */
export interface ServerStatusSettings {
  /** Refresh interval for server status monitoring */
  refreshInterval: ServerStatusRefreshInterval;
  /** Whether the status panel is expanded by default */
  defaultExpanded: boolean;
  /** Whether to show network transfer rates */
  showNetworkRates: boolean;
}

/**
 * Global ignore rules for recursive SFTP uploads and syncs.
 */
export interface UploadIgnoreConfig {
  /** Exclude patterns, one per line in the settings UI */
  excludedPaths: string[];
  /** Whether .gitignore files under the upload root should be honored */
  respectGitignore: boolean;
}

/**
 * All application settings combined.
 */
export interface AppSettings {
  terminal: TerminalSettings;
  appearance: AppearanceSettings;
  sshDefaults: SshDefaultSettings;
  serverStatus: ServerStatusSettings;
  aiPrediction: AiPredictionSettings;
}

/**
 * Default settings values.
 */
export const defaultSettings: AppSettings = {
  terminal: {
    fontSize: 14,
    fontFamily: 'JetBrains Mono',
    cursorStyle: 'block',
    cursorBlink: true,
    scrollbackLines: 10000,
  },
  appearance: {
    theme: 'paper-white',
    windowOpacity: 1.0,
  },
  sshDefaults: {
    defaultPort: 22,
    connectionTimeout: 30,
    keepaliveInterval: 60,
    defaultUsername: '',
  },
  serverStatus: {
    refreshInterval: '30s',
    defaultExpanded: false,
    showNetworkRates: true,
  },
  aiPrediction: { ...defaultAiPredictionSettings },
};

export const defaultUploadIgnoreConfig: UploadIgnoreConfig = {
  excludedPaths: [
    'node_modules/',
    '.git/',
    '.svn/',
    '.hg/',
    'target/',
    '.next/',
    '.nuxt/',
    '.turbo/',
    '.cache/',
    'coverage/',
    '__pycache__/',
    '.pytest_cache/',
    '.mypy_cache/',
    '.ruff_cache/',
    '.venv/',
    'venv/',
    'env/',
  ],
  respectGitignore: true,
};

// ============================================================================
// Store Interface
// ============================================================================

/**
 * Settings store state and actions.
 */
interface SettingsStore {
  // AI Tools state
  /** List of detected AI tools */
  aiTools: AiTool[];
  /** Built-in Agent Gateway status */
  gatewayStatus: AgentGatewayStatus | null;
  /** Current application version from the backend package metadata */
  appVersion: string | null;
  /** Whether app version lookup has completed */
  appVersionLoaded: boolean;

  // App Settings state
  /** Current application settings */
  settings: AppSettings;
  /** Global recursive upload ignore config */
  uploadIgnoreConfig: UploadIgnoreConfig;

  // Loading/Error state
  /** Loading state for async operations */
  loading: boolean;
  /** ID of the tool currently being installed/uninstalled (null if none) */
  loadingToolId: string | null;
  /** Error message if any operation fails */
  error: string | null;
  /** Whether settings have been loaded from storage */
  initialized: boolean;

  // AI Tools actions
  /** Fetch all AI tools and their installation status */
  fetchAiTools: () => Promise<void>;
  /** Fetch built-in Agent Gateway status */
  fetchGatewayStatus: () => Promise<void>;
  /** Fetch current app version */
  fetchAppVersion: () => Promise<void>;
  /** Install VibeShell to a specific AI tool */
  installTo: (toolId: string) => Promise<void>;
  /** Uninstall VibeShell from a specific AI tool */
  uninstallFrom: (toolId: string) => Promise<void>;

  // Settings actions
  /** Initialize settings from persistent storage */
  initializeSettings: () => Promise<void>;
  /** Update terminal settings */
  updateTerminalSettings: (settings: Partial<TerminalSettings>) => Promise<void>;
  /** Update appearance settings */
  updateAppearanceSettings: (settings: Partial<AppearanceSettings>) => Promise<void>;
  /** Update SSH default settings */
  updateSshDefaultSettings: (settings: Partial<SshDefaultSettings>) => Promise<void>;
  /** Update server status settings */
  updateServerStatusSettings: (settings: Partial<ServerStatusSettings>) => Promise<void>;
  /** Update AI command prediction settings */
  updateAiPredictionSettings: (settings: Partial<AiPredictionSettings>) => Promise<void>;
  /** Update global recursive upload ignore config */
  updateUploadIgnoreConfig: (config: Partial<UploadIgnoreConfig>) => Promise<void>;
  /** Reset all settings to defaults */
  resetSettings: () => Promise<void>;
}

// ============================================================================
// Persistence Helpers
// ============================================================================

const SETTINGS_KEY = 'vibeshell_settings';

const LEGACY_THEME_MAP: Record<string, ThemeName> = {
  'tokyo-night': 'violet-black',
  dracula: 'violet-black',
  'one-dark': 'ink-black',
  nord: 'cyan-black',
};

const themeNames = new Set<ThemeName>([
  'paper-white',
  'warm-ivory',
  'ink-black',
  'violet-black',
  'cyan-black',
]);

function normalizeAppearanceSettings(value: unknown): AppearanceSettings {
  const raw = value && typeof value === 'object' ? value as Partial<AppearanceSettings> : {};
  const storedTheme = typeof raw.theme === 'string' ? raw.theme : '';
  const theme = themeNames.has(storedTheme as ThemeName)
    ? storedTheme as ThemeName
    : LEGACY_THEME_MAP[storedTheme] ?? defaultSettings.appearance.theme;
  const opacity = typeof raw.windowOpacity === 'number' && Number.isFinite(raw.windowOpacity)
    ? Math.min(1, Math.max(0.5, raw.windowOpacity))
    : defaultSettings.appearance.windowOpacity;

  return { theme, windowOpacity: opacity };
}

/**
 * Load settings from localStorage (fallback for when Tauri store is not available).
 */
function loadSettingsFromStorage(): AppSettings {
  try {
    const stored = localStorage.getItem(SETTINGS_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      // Merge with defaults to handle new settings added in updates
      return {
        terminal: { ...defaultSettings.terminal, ...parsed.terminal },
        appearance: normalizeAppearanceSettings(parsed.appearance),
        sshDefaults: { ...defaultSettings.sshDefaults, ...parsed.sshDefaults },
        serverStatus: { ...defaultSettings.serverStatus, ...parsed.serverStatus },
        aiPrediction: normalizeAiPredictionSettings(parsed.aiPrediction),
      };
    }
  } catch (error) {
    console.warn('Failed to load settings from storage:', error);
  }
  return { ...defaultSettings };
}

/**
 * Save settings to localStorage.
 */
function saveSettingsToStorage(settings: AppSettings): void {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  } catch (error) {
    console.warn('Failed to save settings to storage:', error);
  }
}

/**
 * Try to load settings from Tauri backend, fallback to localStorage.
 */
async function loadSettings(): Promise<AppSettings> {
  try {
    const settings = await invokeOrThrow<AppSettings | null>('load_settings');
    if (settings) {
      // Merge with defaults to handle new settings added in updates
      return {
        terminal: { ...defaultSettings.terminal, ...settings.terminal },
        appearance: normalizeAppearanceSettings(settings.appearance),
        sshDefaults: { ...defaultSettings.sshDefaults, ...settings.sshDefaults },
        serverStatus: { ...defaultSettings.serverStatus, ...settings.serverStatus },
        aiPrediction: normalizeAiPredictionSettings(settings.aiPrediction),
      };
    }
  } catch (error) {
    console.warn('Failed to load settings from Tauri, using localStorage:', error);
  }
  return loadSettingsFromStorage();
}

/**
 * Save settings to Tauri backend and localStorage.
 */
async function saveSettings(settings: AppSettings): Promise<void> {
  // Always save to localStorage as backup
  saveSettingsToStorage(settings);

  try {
    await invokeOrThrow('save_settings', { settings });
  } catch (error) {
    console.warn('Failed to save settings to Tauri backend:', error);
  }
}

async function loadUploadIgnoreConfig(): Promise<UploadIgnoreConfig> {
  try {
    const config = await invokeOrThrow<UploadIgnoreConfig | null>('sftp_get_upload_ignore_config');
    if (config) {
      return {
        excludedPaths: config.excludedPaths ?? defaultUploadIgnoreConfig.excludedPaths,
        respectGitignore: config.respectGitignore ?? defaultUploadIgnoreConfig.respectGitignore,
      };
    }
  } catch (error) {
    console.warn('Failed to load upload ignore config:', error);
  }
  return { ...defaultUploadIgnoreConfig };
}

async function saveUploadIgnoreConfig(config: UploadIgnoreConfig): Promise<UploadIgnoreConfig> {
  try {
    return await invokeOrThrow<UploadIgnoreConfig>('sftp_save_upload_ignore_config', { config });
  } catch (error) {
    console.warn('Failed to save upload ignore config:', error);
    throw error;
  }
}

// ============================================================================
// Store Implementation
// ============================================================================

/**
 * Zustand store for settings and AI tool integrations.
 */
export const useSettingsStore = create<SettingsStore>((set, get) => ({
  // Initial state
  aiTools: [],
  gatewayStatus: null,
  appVersion: null,
  appVersionLoaded: false,
  settings: { ...defaultSettings },
  uploadIgnoreConfig: { ...defaultUploadIgnoreConfig },
  loading: false,
  loadingToolId: null,
  error: null,
  initialized: false,

  // -------------------------------------------------------------------------
  // AI Tools Actions
  // -------------------------------------------------------------------------

  fetchAiTools: async () => {
    set({ loading: true, error: null });
    try {
      const tools = await invokeOrThrow<AiTool[]>('detect_ai_tools');
      set({ aiTools: tools, loading: false });
    } catch (error) {
      set({
        error: error instanceof Error ? error.message : String(error),
        loading: false,
      });
    }
  },

  fetchGatewayStatus: async () => {
    try {
      const status = await invokeOrThrow<AgentGatewayStatus>('get_agent_gateway_status');
      set({ gatewayStatus: status });
    } catch (error) {
      set({
        error: error instanceof Error ? error.message : String(error),
      });
    }
  },

  fetchAppVersion: async () => {
    try {
      const appVersion = await invokeOrThrow<string>('get_app_version');
      set({ appVersion, appVersionLoaded: true });
    } catch (error) {
      console.warn('Failed to fetch app version:', error);
      set({ appVersion: null, appVersionLoaded: true });
    }
  },

  installTo: async (toolId: string) => {
    set({ loading: true, loadingToolId: toolId, error: null });
    try {
      await invokeOrThrow('install_to_tool', { toolId });
      // Refresh the tools list after installation
      await get().fetchAiTools();
      set({ loadingToolId: null });
    } catch (error) {
      set({
        error: error instanceof Error ? error.message : String(error),
        loading: false,
        loadingToolId: null,
      });
    }
  },

  uninstallFrom: async (toolId: string) => {
    set({ loading: true, loadingToolId: toolId, error: null });
    try {
      await invokeOrThrow('uninstall_from_tool', { toolId });
      // Refresh the tools list after uninstallation
      await get().fetchAiTools();
      set({ loadingToolId: null });
    } catch (error) {
      set({
        error: error instanceof Error ? error.message : String(error),
        loading: false,
        loadingToolId: null,
      });
    }
  },

  // -------------------------------------------------------------------------
  // Settings Actions
  // -------------------------------------------------------------------------

  initializeSettings: async () => {
    if (get().initialized) return;

    try {
      const [settings, uploadIgnoreConfig] = await Promise.all([
        loadSettings(),
        loadUploadIgnoreConfig(),
      ]);
      set({ settings, uploadIgnoreConfig, initialized: true });
    } catch (error) {
      console.error('Failed to initialize settings:', error);
      set({ initialized: true }); // Mark as initialized even on error to prevent infinite loops
    }
  },

  updateTerminalSettings: async (terminalUpdates: Partial<TerminalSettings>) => {
    const currentSettings = get().settings;
    const newSettings: AppSettings = {
      ...currentSettings,
      terminal: {
        ...currentSettings.terminal,
        ...terminalUpdates,
      },
    };

    set({ settings: newSettings });
    await saveSettings(newSettings);
  },

  updateAppearanceSettings: async (appearanceUpdates: Partial<AppearanceSettings>) => {
    const currentSettings = get().settings;
    const newSettings: AppSettings = {
      ...currentSettings,
      appearance: {
        ...currentSettings.appearance,
        ...appearanceUpdates,
      },
    };

    set({ settings: newSettings });
    await saveSettings(newSettings);
  },

  updateSshDefaultSettings: async (sshUpdates: Partial<SshDefaultSettings>) => {
    const currentSettings = get().settings;
    const newSettings: AppSettings = {
      ...currentSettings,
      sshDefaults: {
        ...currentSettings.sshDefaults,
        ...sshUpdates,
      },
    };

    set({ settings: newSettings });
    await saveSettings(newSettings);
  },

  updateServerStatusSettings: async (serverStatusUpdates: Partial<ServerStatusSettings>) => {
    const currentSettings = get().settings;
    const newSettings: AppSettings = {
      ...currentSettings,
      serverStatus: {
        ...currentSettings.serverStatus,
        ...serverStatusUpdates,
      },
    };

    set({ settings: newSettings });
    await saveSettings(newSettings);
  },

  updateAiPredictionSettings: async (aiPredictionUpdates: Partial<AiPredictionSettings>) => {
    const currentSettings = get().settings;
    const newSettings: AppSettings = {
      ...currentSettings,
      aiPrediction: normalizeAiPredictionSettings({
        ...currentSettings.aiPrediction,
        ...aiPredictionUpdates,
      }),
    };

    set({ settings: newSettings });
    await saveSettings(newSettings);
  },

  updateUploadIgnoreConfig: async (uploadIgnoreUpdates: Partial<UploadIgnoreConfig>) => {
    const currentConfig = get().uploadIgnoreConfig;
    const nextConfig: UploadIgnoreConfig = {
      ...currentConfig,
      ...uploadIgnoreUpdates,
      excludedPaths:
        uploadIgnoreUpdates.excludedPaths?.map((value) => value.trim()).filter(Boolean) ??
        currentConfig.excludedPaths,
    };

    set({ uploadIgnoreConfig: nextConfig });
    const savedConfig = await saveUploadIgnoreConfig(nextConfig);
    set({ uploadIgnoreConfig: savedConfig });
  },

  resetSettings: async () => {
    set({
      settings: { ...defaultSettings },
      uploadIgnoreConfig: { ...defaultUploadIgnoreConfig },
    });
    await saveSettings(defaultSettings);
    await saveUploadIgnoreConfig(defaultUploadIgnoreConfig);
  },
}));

// ============================================================================
// Theme Definitions (for use in components)
// ============================================================================

export interface ThemeDefinition {
  name: ThemeName;
  displayName: string;
  colors: {
    bg: string;
    bgDark: string;
    bgHl: string;
    fg: string;
    fgDark: string;
    accent: string;
    onAccent: string;
  };
}

export const themes: ThemeDefinition[] = [
  {
    name: 'paper-white',
    displayName: 'Paper White',
    colors: {
      bg: '#ffffff',
      bgDark: '#f7f7f5',
      bgHl: '#e7e7e3',
      fg: '#171717',
      fgDark: '#73736e',
      accent: '#6d4aff',
      onAccent: '#ffffff',
    },
  },
  {
    name: 'warm-ivory',
    displayName: 'Warm Ivory',
    colors: {
      bg: '#f7f3ea',
      bgDark: '#eee8dd',
      bgHl: '#ded5c6',
      fg: '#181714',
      fgDark: '#746e64',
      accent: '#5d3fd3',
      onAccent: '#ffffff',
    },
  },
  {
    name: 'ink-black',
    displayName: 'Ink Black',
    colors: {
      bg: '#0b0b0c',
      bgDark: '#050506',
      bgHl: '#1c1c1f',
      fg: '#f5f5f4',
      fgDark: '#85858c',
      accent: '#a78bfa',
      onAccent: '#0b0b0c',
    },
  },
  {
    name: 'violet-black',
    displayName: 'Violet Black',
    colors: {
      bg: '#100d16',
      bgDark: '#09070d',
      bgHl: '#282032',
      fg: '#f4efff',
      fgDark: '#958aa6',
      accent: '#9b72ff',
      onAccent: '#100d16',
    },
  },
  {
    name: 'cyan-black',
    displayName: 'Cyan Black',
    colors: {
      bg: '#071112',
      bgDark: '#04090a',
      bgHl: '#112729',
      fg: '#e9f8f7',
      fgDark: '#7b9998',
      accent: '#2dd4bf',
      onAccent: '#071112',
    },
  },
];

/**
 * Font family options for the terminal.
 */
export const fontFamilies: { value: FontFamily; label: string }[] = [
  { value: 'JetBrains Mono', label: 'JetBrains Mono' },
  { value: 'Fira Code', label: 'Fira Code' },
  { value: 'Consolas', label: 'Consolas' },
  { value: 'Monaco', label: 'Monaco' },
];

/**
 * Cursor style options for the terminal.
 */
export const cursorStyles: { value: CursorStyle; label: string }[] = [
  { value: 'block', label: 'Block' },
  { value: 'underline', label: 'Underline' },
  { value: 'bar', label: 'Bar' },
];
