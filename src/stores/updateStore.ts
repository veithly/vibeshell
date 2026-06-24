import { create } from 'zustand';
import { invokeOrThrow } from '../lib/tauri';
import type { DownloadEvent } from '@tauri-apps/plugin-updater';

export const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

const GITHUB_LATEST_RELEASE_API =
  'https://api.github.com/repos/veithly/vibeshell/releases/latest';
const GITHUB_LATEST_RELEASE_PAGE = 'https://github.com/veithly/vibeshell/releases/latest';
const UPDATE_STATE_KEY = 'vibeshell_update_state';
const REQUEST_TIMEOUT_MS = 12_000;

interface GitHubReleaseAsset {
  name: string;
  browser_download_url: string;
  size?: number;
}

interface GitHubReleaseResponse {
  tag_name: string;
  name?: string | null;
  html_url: string;
  published_at?: string | null;
  assets?: GitHubReleaseAsset[];
}

export interface ReleaseAsset {
  name: string;
  downloadUrl: string;
  size?: number;
}

export interface AppRelease {
  version: string;
  tagName: string;
  name: string;
  htmlUrl: string;
  downloadUrl: string;
  publishedAt: string | null;
  assets: ReleaseAsset[];
}

interface PersistedUpdateState {
  lastCheckedAt: number | null;
  lastNotifiedVersion: string | null;
  latestRelease: AppRelease | null;
}

interface CheckOptions {
  force?: boolean;
  currentVersion?: string | null;
}

interface UpdateStore {
  currentVersion: string | null;
  latestRelease: AppRelease | null;
  updateAvailable: boolean;
  checking: boolean;
  installing: boolean;
  installProgress: number | null;
  error: string | null;
  lastCheckedAt: number | null;
  lastNotifiedVersion: string | null;

  checkForUpdates: (options?: CheckOptions) => Promise<AppRelease | null>;
  installLatestUpdate: () => Promise<void>;
  openLatestRelease: () => Promise<void>;
  markVersionNotified: (version: string) => void;
}

function loadPersistedState(): PersistedUpdateState {
  try {
    const raw = localStorage.getItem(UPDATE_STATE_KEY);
    if (!raw) {
      return { lastCheckedAt: null, lastNotifiedVersion: null, latestRelease: null };
    }

    const parsed = JSON.parse(raw) as Partial<PersistedUpdateState>;
    return {
      lastCheckedAt: typeof parsed.lastCheckedAt === 'number' ? parsed.lastCheckedAt : null,
      lastNotifiedVersion:
        typeof parsed.lastNotifiedVersion === 'string' ? parsed.lastNotifiedVersion : null,
      latestRelease: parsed.latestRelease ?? null,
    };
  } catch {
    return { lastCheckedAt: null, lastNotifiedVersion: null, latestRelease: null };
  }
}

function savePersistedState(state: PersistedUpdateState): void {
  try {
    localStorage.setItem(UPDATE_STATE_KEY, JSON.stringify(state));
  } catch {
    // localStorage can be unavailable in restricted browser modes.
  }
}

function normalizeVersion(version: string): string {
  return version.trim().replace(/^v/i, '').split(/[+-]/)[0];
}

function parseVersion(version: string): number[] | null {
  const normalized = normalizeVersion(version);
  if (!/^\d+(?:\.\d+)*$/.test(normalized)) {
    return null;
  }
  return normalized.split('.').map((part) => Number(part));
}

export function compareVersions(left: string, right: string): number {
  const leftParts = parseVersion(left);
  const rightParts = parseVersion(right);
  if (!leftParts || !rightParts) {
    return 0;
  }

  const length = Math.max(leftParts.length, rightParts.length);
  for (let index = 0; index < length; index += 1) {
    const leftPart = leftParts[index] ?? 0;
    const rightPart = rightParts[index] ?? 0;
    if (leftPart > rightPart) return 1;
    if (leftPart < rightPart) return -1;
  }
  return 0;
}

export function isNewerVersion(candidate: string, current: string | null): boolean {
  return current !== null && compareVersions(candidate, current) > 0;
}

function getPlatformAssetPriority(userAgent = navigator.userAgent): string[] {
  const agent = userAgent.toLowerCase();

  if (agent.includes('windows')) {
    return ['.msi', '.exe', '.zip'];
  }
  if (agent.includes('mac os') || agent.includes('macintosh')) {
    return ['.dmg', '.pkg', '.app.tar.gz', '.zip'];
  }
  if (agent.includes('linux') || agent.includes('x11')) {
    return ['.appimage', '.deb', '.rpm', '.tar.gz'];
  }
  return ['.dmg', '.msi', '.appimage', '.deb', '.zip', '.tar.gz'];
}

function isMetadataAsset(assetName: string): boolean {
  const lowerName = assetName.toLowerCase();
  return (
    lowerName.endsWith('.sig') ||
    lowerName.endsWith('.sha256') ||
    lowerName.endsWith('.sha256sum') ||
    lowerName.endsWith('.json')
  );
}

export function selectPreferredAsset(
  assets: ReleaseAsset[],
  userAgent = navigator.userAgent
): ReleaseAsset | null {
  const installableAssets = assets.filter((asset) => !isMetadataAsset(asset.name));
  const priorities = getPlatformAssetPriority(userAgent);

  for (const extension of priorities) {
    const asset = installableAssets.find((candidate) =>
      candidate.name.toLowerCase().endsWith(extension)
    );
    if (asset) {
      return asset;
    }
  }

  return installableAssets[0] ?? null;
}

function mapGitHubRelease(release: GitHubReleaseResponse): AppRelease {
  const assets =
    release.assets?.map((asset) => ({
      name: asset.name,
      downloadUrl: asset.browser_download_url,
      size: asset.size,
    })) ?? [];
  const preferredAsset = selectPreferredAsset(assets);

  return {
    version: normalizeVersion(release.tag_name),
    tagName: release.tag_name,
    name: release.name || release.tag_name,
    htmlUrl: release.html_url,
    downloadUrl: preferredAsset?.downloadUrl ?? release.html_url,
    publishedAt: release.published_at ?? null,
    assets,
  };
}

async function fetchLatestRelease(): Promise<AppRelease> {
  const controller = new AbortController();
  const timeoutId = window.setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);

  try {
    const response = await fetch(GITHUB_LATEST_RELEASE_API, {
      headers: {
        Accept: 'application/vnd.github+json',
      },
      signal: controller.signal,
    });

    if (!response.ok) {
      throw new Error(`GitHub returned ${response.status}`);
    }

    return mapGitHubRelease((await response.json()) as GitHubReleaseResponse);
  } finally {
    window.clearTimeout(timeoutId);
  }
}

async function fetchSignedUpdaterRelease(): Promise<AppRelease | null> {
  const { check } = await import('@tauri-apps/plugin-updater');
  const update = await check({ timeout: REQUEST_TIMEOUT_MS });
  if (!update) {
    return null;
  }

  return {
    version: normalizeVersion(update.version),
    tagName: `v${normalizeVersion(update.version)}`,
    name: `VibeShell ${update.version}`,
    htmlUrl: GITHUB_LATEST_RELEASE_PAGE,
    downloadUrl: GITHUB_LATEST_RELEASE_PAGE,
    publishedAt: update.date ?? null,
    assets: [],
  };
}

async function getCurrentVersion(currentVersion?: string | null): Promise<string | null> {
  if (currentVersion) {
    return currentVersion;
  }

  try {
    return await invokeOrThrow<string>('get_app_version');
  } catch {
    return null;
  }
}

async function openUrl(url: string): Promise<void> {
  try {
    await invokeOrThrow('open_external_url', { url });
  } catch (error) {
    console.warn('Falling back to browser URL open:', error);
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}

const persistedState = loadPersistedState();

export const useUpdateStore = create<UpdateStore>((set, get) => ({
  currentVersion: null,
  latestRelease: persistedState.latestRelease,
  updateAvailable: false,
  checking: false,
  installing: false,
  installProgress: null,
  error: null,
  lastCheckedAt: persistedState.lastCheckedAt,
  lastNotifiedVersion: persistedState.lastNotifiedVersion,

  checkForUpdates: async (options = {}) => {
    const currentVersion = await getCurrentVersion(options.currentVersion);
    const cachedRelease = get().latestRelease;
    const lastCheckedAt = get().lastCheckedAt;
    const now = Date.now();

    if (
      !options.force &&
      cachedRelease &&
      lastCheckedAt &&
      now - lastCheckedAt < UPDATE_CHECK_INTERVAL_MS
    ) {
      const updateAvailable = isNewerVersion(cachedRelease.version, currentVersion);
      set({ currentVersion, updateAvailable, error: null });
      return updateAvailable ? cachedRelease : null;
    }

    set({ checking: true, error: null });

    try {
      let latestRelease = await fetchSignedUpdaterRelease();
      if (!latestRelease) {
        latestRelease = await fetchLatestRelease();
      }
      const checkedAt = Date.now();
      const updateAvailable = isNewerVersion(latestRelease.version, currentVersion);
      const persisted = {
        lastCheckedAt: checkedAt,
        lastNotifiedVersion: get().lastNotifiedVersion,
        latestRelease,
      };

      savePersistedState(persisted);
      set({
        currentVersion,
        latestRelease,
        updateAvailable,
        lastCheckedAt: checkedAt,
        checking: false,
      });

      return updateAvailable ? latestRelease : null;
    } catch (error) {
      try {
        const latestRelease = await fetchLatestRelease();
        const checkedAt = Date.now();
        const updateAvailable = isNewerVersion(latestRelease.version, currentVersion);
        const persisted = {
          lastCheckedAt: checkedAt,
          lastNotifiedVersion: get().lastNotifiedVersion,
          latestRelease,
        };

        savePersistedState(persisted);
        set({
          currentVersion,
          latestRelease,
          updateAvailable,
          lastCheckedAt: checkedAt,
          checking: false,
        });

        return updateAvailable ? latestRelease : null;
      } catch (fallbackError) {
        const message = fallbackError instanceof Error ? fallbackError.message : String(fallbackError);
        set({
          currentVersion,
          error: message,
          checking: false,
        });
        return null;
      }
    }
  },

  installLatestUpdate: async () => {
    set({ installing: true, installProgress: 0, error: null });

    try {
      const [{ check }, { relaunch }] = await Promise.all([
        import('@tauri-apps/plugin-updater'),
        import('@tauri-apps/plugin-process'),
      ]);
      const update = await check({ timeout: REQUEST_TIMEOUT_MS });

      if (!update) {
        throw new Error('No signed update is available for this version.');
      }

      let downloaded = 0;
      let contentLength: number | undefined;

      await update.downloadAndInstall((event: DownloadEvent) => {
        if (event.event === 'Started') {
          contentLength = event.data.contentLength;
          downloaded = 0;
          set({ installProgress: contentLength ? 0 : null });
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength;
          set({
            installProgress: contentLength
              ? Math.min(100, Math.round((downloaded / contentLength) * 100))
              : null,
          });
        } else if (event.event === 'Finished') {
          set({ installProgress: 100 });
        }
      });

      await relaunch();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      set({
        error: message,
        installing: false,
        installProgress: null,
      });
    }
  },

  openLatestRelease: async () => {
    const latestRelease = get().latestRelease;
    await openUrl(latestRelease?.downloadUrl ?? latestRelease?.htmlUrl ?? GITHUB_LATEST_RELEASE_PAGE);
  },

  markVersionNotified: (version: string) => {
    const nextState = {
      lastCheckedAt: get().lastCheckedAt,
      lastNotifiedVersion: version,
      latestRelease: get().latestRelease,
    };

    savePersistedState(nextState);
    set({ lastNotifiedVersion: version });
  },
}));
