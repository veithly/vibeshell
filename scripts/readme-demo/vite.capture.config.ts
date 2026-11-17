import { defineConfig, mergeConfig, type Plugin } from 'vite';
import base from '../../vite.config';
import { fileURLToPath } from 'node:url';

// Opt-in loopback-only scene driver; accepts a fixed scene name, never code.
const scenes = new Set(['workspace', 'panes', 'panes-day', 'completion', 'sftp', 'extract', 'files', 'plugins', 'settings', 'coding', 'approval']);
const driver: Plugin = {
  name: 'readme-native-capture',
  apply: 'serve',
  configureServer(server) {
    let status: unknown = { stage: 'waiting' };
    let requested = { scene: 'workspace', ticket: 0 };
    server.ws.on('vibeshell:readme-report', (report) => {
      status = report;
      console.log('[README_CAPTURE]', JSON.stringify(report));
    });
    server.middlewares.use('/__readme/status', (_req, res) => {
      res.setHeader('Content-Type', 'application/json');
      res.end(JSON.stringify(status));
    });
    server.middlewares.use('/__readme/requested', (_req, res) => {
      res.setHeader('Content-Type', 'application/json');
      res.setHeader('Cache-Control', 'no-store');
      res.end(JSON.stringify(requested));
    });
    server.middlewares.use('/__readme/scene', (req, res) => {
      const scene = new URL(req.url ?? '', 'http://127.0.0.1').searchParams.get('name');
      if (req.method !== 'POST' || !scene || !scenes.has(scene)) { res.statusCode = 400; res.end('Unknown scene'); return; }
      status = { stage: 'loading', scene };
      requested = { scene, ticket: Date.now() };
      server.ws.send('vibeshell:readme-scene', requested);
      res.end('Scene requested');
    });
  },
};
export default defineConfig(mergeConfig(base, {
  resolve: { alias: [
    { find: /^@tauri-apps\/api\/core$/, replacement: fileURLToPath(new URL('./fixture-core.ts', import.meta.url)) },
    { find: /^@tauri-apps\/api\/webviewWindow$/, replacement: fileURLToPath(new URL('./fixture-webview.ts', import.meta.url)) },
  ] },
  plugins: process.env.VIBESHELL_README_CAPTURE === 'run' ? [driver] : [],
  server: { host: '127.0.0.1', port: 1421, strictPort: true },
}));
