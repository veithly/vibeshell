// Used only by vite.capture.config.ts; production imports the original API.
export * from '../../node_modules/@tauri-apps/api/core.js';
import { invoke as originalInvoke, type InvokeArgs, type InvokeOptions } from '../../node_modules/@tauri-apps/api/core.js';
export async function invoke<T>(cmd: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T> {
  const fixture = (window as any).__VIBESHELL_README_INVOKE__;
  return fixture ? fixture(cmd, args) : originalInvoke<T>(cmd, args, options);
}
