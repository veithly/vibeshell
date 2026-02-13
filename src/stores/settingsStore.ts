import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

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
export type ThemeName = 'tokyo-night' | 'dracula' | 'one-dark' | 'nord';

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
 * All application settings combined.
 */
export interface AppSettings {
  terminal: TerminalSettings;
  appearance: AppearanceSettings;
  sshDefaults: SshDefaultSettings;
  serverStatus: ServerStatusSettings;
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
    theme: 'tokyo-night',
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

  // App Settings state
  /** Current application settings */
  settings: AppSettings;

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
  /** Reset all settings to defaults */
  resetSettings: () => Promise<void>;
}

// ============================================================================
// Persistence Helpers
// ============================================================================

const SETTINGS_KEY = 'vibeshell_settings';

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
        appearance: { ...defaultSettings.appearance, ...parsed.appearance },
        sshDefaults: { ...defaultSettings.sshDefaults, ...parsed.sshDefaults },
        serverStatus: { ...defaultSettings.serverStatus, ...parsed.serverStatus },
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
    const settings = await invoke<AppSettings | null>('load_settings');
    if (settings) {
      // Merge with defaults to handle new settings added in updates
      return {
        terminal: { ...defaultSettings.terminal, ...settings.terminal },
        appearance: { ...defaultSettings.appearance, ...settings.appearance },
        sshDefaults: { ...defaultSettings.sshDefaults, ...settings.sshDefaults },
        serverStatus: { ...defaultSettings.serverStatus, ...settings.serverStatus },
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
    await invoke('save_settings', { settings });
  } catch (error) {
    console.warn('Failed to save settings to Tauri backend:', error);
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
  settings: { ...defaultSettings },
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
      const tools = await invoke<AiTool[]>('detect_ai_tools');
      set({ aiTools: tools, loading: false });
    } catch (error) {
      set({
        error: error instanceof Error ? error.message : String(error),
        loading: false,
      });
    }
  },

  installTo: async (toolId: string) => {
    set({ loading: true, loadingToolId: toolId, error: null });
    try {
      await invoke('install_to_tool', { toolId });
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
      await invoke('uninstall_from_tool', { toolId });
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
      const settings = await loadSettings();
      set({ settings, initialized: true });
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

  resetSettings: async () => {
    set({ settings: { ...defaultSettings } });
    await saveSettings(defaultSettings);
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
  };
}

export const themes: ThemeDefinition[] = [
  {
    name: 'tokyo-night',
    displayName: 'Tokyo Night',
    colors: {
      bg: '#1a1b26',
      bgDark: '#16161e',
      bgHl: '#292e42',
      fg: '#a9b1d6',
      fgDark: '#565f89',
      accent: '#7aa2f7',
    },
  },
  {
    name: 'dracula',
    displayName: 'Dracula',
    colors: {
      bg: '#282a36',
      bgDark: '#21222c',
      bgHl: '#44475a',
      fg: '#f8f8f2',
      fgDark: '#6272a4',
      accent: '#bd93f9',
    },
  },
  {
    name: 'one-dark',
    displayName: 'One Dark',
    colors: {
      bg: '#282c34',
      bgDark: '#21252b',
      bgHl: '#3e4451',
      fg: '#abb2bf',
      fgDark: '#5c6370',
      accent: '#61afef',
    },
  },
  {
    name: 'nord',
    displayName: 'Nord',
    colors: {
      bg: '#2e3440',
      bgDark: '#242933',
      bgHl: '#3b4252',
      fg: '#eceff4',
      fgDark: '#4c566a',
      accent: '#88c0d0',
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
