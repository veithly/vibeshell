import { create } from 'zustand';
import { safeInvoke } from '../lib/tauri';

export type RuntimePlatform = 'macos' | 'windows' | 'linux' | 'ios' | 'android' | 'unknown';

export interface RuntimeCapabilities {
  readonly platform: RuntimePlatform;
  readonly isMobile: boolean;
  readonly windowControls: boolean;
  readonly localShell: boolean;
  readonly agentGateway: boolean;
  readonly desktopUpdater: boolean;
  readonly cliIpc: boolean;
  readonly directoryTransfer: boolean;
  readonly backgroundTunnels: boolean;
}

export const WEB_RUNTIME_CAPABILITIES: RuntimeCapabilities = Object.freeze({
  platform: 'unknown',
  isMobile: false,
  windowControls: false,
  localShell: false,
  agentGateway: false,
  desktopUpdater: false,
  cliIpc: false,
  directoryTransfer: false,
  backgroundTunnels: false,
});

export async function fetchRuntimeCapabilities(): Promise<RuntimeCapabilities> {
  const result = await safeInvoke<RuntimeCapabilities>('get_runtime_capabilities');
  return result.success ? result.data : WEB_RUNTIME_CAPABILITIES;
}

type RuntimeCapabilitiesStatus = 'idle' | 'loading' | 'ready';

interface RuntimeCapabilitiesStore {
  capabilities: RuntimeCapabilities;
  status: RuntimeCapabilitiesStatus;
  load: () => Promise<RuntimeCapabilities>;
}

let pendingLoad: Promise<RuntimeCapabilities> | null = null;

export const useRuntimeCapabilitiesStore = create<RuntimeCapabilitiesStore>((set, get) => ({
  capabilities: WEB_RUNTIME_CAPABILITIES,
  status: 'idle',

  load: () => {
    const state = get();
    if (state.status === 'ready') {
      return Promise.resolve(state.capabilities);
    }
    if (pendingLoad) {
      return pendingLoad;
    }

    set({ status: 'loading' });
    pendingLoad = fetchRuntimeCapabilities()
      .then((capabilities) => {
        set({ capabilities, status: 'ready' });
        return capabilities;
      })
      .finally(() => {
        pendingLoad = null;
      });

    return pendingLoad;
  },
}));
