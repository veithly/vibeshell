// Keep the real native detach/handoff implementation, but load the safe fixture
// in the receiving WebView instead of the normal application entry.
export * from '../../node_modules/@tauri-apps/api/webviewWindow.js';
import { WebviewWindow as NativeWebviewWindow } from '../../node_modules/@tauri-apps/api/webviewWindow.js';
export class WebviewWindow extends NativeWebviewWindow {
  constructor(label: string, options: ConstructorParameters<typeof NativeWebviewWindow>[1]) {
    if (!(window as any).__VIBESHELL_README_INVOKE__ || !label.startsWith('detach-') || !String(options?.url).startsWith('index.html?detach=')) {
      throw new Error('Documentation capture only permits isolated detached pages');
    }
    super(label, {...options, url:location.origin + '/scripts/readme-demo/' + options!.url + '&scene=panes'});
  }
}
