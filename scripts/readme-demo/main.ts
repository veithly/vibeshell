/// <reference types="vite/client" />
// Documentation-only entry. Nothing in src/ imports this fixture.
// Real components + in-memory IPC; never delegate to a native backend.
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import type { PluginManifest, PluginRecord } from '../../src/plugins/types';
import type { SessionInfo, LocalShellSessionInfo } from '../../src/stores/sessionStore';

if (!import.meta.env.DEV || !['127.0.0.1', 'localhost'].includes(location.hostname) || location.port !== '1421') {
  throw new Error('Documentation demo requires the isolated loopback development origin on port 1421.');
}
const scenario = new URLSearchParams(location.search).get('scene') ?? 'collaboration';
const lang = new URLSearchParams(location.search).get('lang') === 'zh' ? 'zh' : 'en';
const light = scenario === 'theme' || scenario === 'files';
const time = Date.UTC(2026, 8, 18, 9, 42, 0);
const seconds = Math.floor(time / 1000);
const root = '/srv/northstar';
const workspace = '/workspace/northstar';
const blocked: string[] = [];
const calls: string[] = [];
const errors: string[] = [];
window.addEventListener('error', event => errors.push(event.message));
window.addEventListener('unhandledrejection', event => errors.push(String(event.reason)));

const servers = [
  { id: 'demo-api', name: 'API · staging', host: '192.0.2.10', group_id: 'demo-staging', tags: ['api', 'preview'] },
  { id: 'demo-worker', name: 'Workers · staging', host: '192.0.2.11', group_id: 'demo-staging', tags: ['jobs', 'docker'] },
  { id: 'demo-db', name: 'Postgres · private', host: '192.0.2.20', group_id: 'demo-data', tags: ['postgres', 'via bastion'], jump_host_id: 'demo-bastion' },
  { id: 'demo-cache', name: 'Redis · private', host: '192.0.2.21', group_id: 'demo-data', tags: ['redis', 'cache'], jump_host_id: 'demo-bastion' },
  { id: 'demo-bastion', name: 'Bastion · gateway', host: '192.0.2.30', group_id: 'demo-platform', tags: ['gateway'] },
  { id: 'demo-web', name: 'Web · preview', host: '192.0.2.40', group_id: 'demo-platform', tags: ['frontend', 'preview'] },
].map(server => ({ port: 22, username: 'demo', auth_type: 'key_with_passphrase', credential_id: null,
  created_at: seconds, updated_at: seconds, ...server }));
const ssh: SessionInfo[] = [
  { id: 'demo-session-api', server_id: 'demo-api', server_name: 'API · staging', state: 'connected', created_at: seconds, clients: 2 },
  { id: 'demo-session-worker', server_id: 'demo-worker', server_name: 'Workers · staging', state: 'connected', created_at: seconds, clients: 1 },
];
const local: LocalShellSessionInfo[] = [
  { id: 'demo-session-agent', shellId: 'zsh', shellName: 'Codex · northstar', cwd: workspace, agentId: 'codex', state: 'running', createdAt: seconds, clients: 1 },
];
const manifestModules = import.meta.glob('../../plugins/builtin/*/plugin.json', { eager: true, import: 'default' });
const plugins: PluginRecord[] = Object.values(manifestModules).map(value => {
  const manifest = value as PluginManifest;
  return { manifest, source: 'builtin', installed: true, enabled: true,
    grantedPermissions: manifest.permissions, settings: manifest.defaultSettings ?? {}, installedAt: seconds };
});
const markdown = `# Northstar deployment notes\n\nA small runbook, kept next to the terminal.\n\n## Before you deploy\n\n- [x] Review the retry change\n- [x] Check the staging health endpoint\n- [ ] Ask a human before restarting the service\n\n## Service map\n\n| Service | Port | Health |\n| --- | --- | --- |\n| API | 8080 | Ready |\n| Worker | 9090 | Ready |\n| Postgres | 5432 | Private network |\n\n## Verify the rollout\n\n\`\`\`sh\ncurl -fsS http://127.0.0.1:8080/health\ndocker compose logs --tail 30 api\n\`\`\`\n\n> Demo runbook. All hostnames and results in this gallery are synthetic.\n`;
const code = `import { setTimeout as delay } from 'node:timers/promises';\n\nexport async function fetchHealth(url: string) {\n  for (let attempt = 0; attempt < 3; attempt++) {\n    try {\n      const response = await fetch(url, {\n        signal: AbortSignal.timeout(5000),\n      });\n      if (!response.ok) throw new Error(\`HTTP \${response.status}\`);\n      return await response.json();\n    } catch (error) {\n      if (attempt === 2) throw error;\n      await delay(250 * 2 ** attempt);\n    }\n  }\n}\n`;
const files: Record<string, string> = {
  [`${root}/RUNBOOK.md`]: markdown,
  [`${root}/src/health.ts`]: code,
  [`${root}/compose.yaml`]: 'services:\n  api:\n    image: northstar/api:preview\n    ports: ["127.0.0.1:8080:8080"]\n    restart: unless-stopped\n  worker:\n    image: northstar/worker:preview\n    environment:\n      QUEUE: jobs-preview\n',
};
const activity = [
  ['mcp:session_exec', 'docker compose ps', 'demo-session-api'],
  ['mcp:sftp_read_file', `Read ${root}/src/health.ts`, 'demo-session-api'],
  ['cli:session_exec', 'curl -fsS http://127.0.0.1:8080/health', 'demo-session-api'],
  ['mcp:plugin_execute', 'docker-containers · logs\ncontainer=api · tail=30', 'demo-session-api'],
  ['cli:session_create', 'Opened a separate worker session', 'demo-session-worker'],
  ['mcp:session_exec', 'git diff --stat\n# Inspect only; leave the shared prompt alone', 'demo-session-api'],
].map(([tool, summary, sessionId], index) => ({ id: `demo-operation-${index}`, sequence: index + 1,
  tool, summary, sessionId, status: 'succeeded' as const, timestamp: time - (6 - index) * 60000 }));

const settings = {
  terminal: { fontSize: 15, fontFamily: 'Monaco', cursorStyle: 'bar', cursorBlink: false, scrollbackLines: 10000 },
  appearance: { theme: light ? 'paper-white' : 'violet-black', themeMode: 'manual', lightTheme: 'paper-white', darkTheme: 'violet-black', windowOpacity: 1 },
  sshDefaults: { defaultPort: 22, connectionTimeout: 30, keepaliveInterval: 60, defaultUsername: 'demo' },
  aiPrediction: { enabled: false, provider: 'openai', apiKey: '', baseUrl: 'https://api.example.test/v1', model: '', debounceMs: 450, maxTokens: 32, minChars: 2 },
};
const capabilities = { platform: 'macos', isMobile: false, windowControls: false, localShell: true,
  agentGateway: true, desktopUpdater: false, cliIpc: true, directoryTransfer: true, backgroundTunnels: true };
const active = scenario === 'coding' ? 'demo-session-agent' : 'demo-session-api';
const fileTab = { id: `demo-session-api\u0000${root}/RUNBOOK.md`, sessionId: 'demo-session-api',
  path: `${root}/RUNBOOK.md`, name: 'RUNBOOK.md', kind: 'text', size: markdown.length, dirty: false };
const pluginTab = { id: 'demo-session-api::server-performance', pluginId: 'server-performance',
  sessionId: 'demo-session-api', sessionType: 'ssh', serverName: 'API · staging' };
const sessionPane = `session:${active}`;
const filePane = `file:${fileTab.id}`;
const pluginPane = `plugin:${pluginTab.id}`;
// Dedicated origin only: reset just this demo's workspace keys on navigation.
for (const key of Object.keys(localStorage)) {
  if (key.startsWith('vibeshell.') || key.startsWith('vibeshell-') || key.startsWith('vibeshell_')) localStorage.removeItem(key);
}
localStorage.setItem('vibeshell-lang', lang);
localStorage.setItem('newConnectionTab', 'ssh');
localStorage.setItem('vibeshell-connection-view', 'icons');
localStorage.setItem('vibeshell-coding-workspace', workspace);
localStorage.setItem('vibeshell-coding-agent', 'codex');
localStorage.setItem('vibeshell_command_history:demo-api', JSON.stringify(['docker compose logs --tail 30 api', 'docker compose ps', 'git status --short']));
localStorage.setItem('vibeshell.workspace-layout.v2', JSON.stringify({ version: 2,
  sessions: [...ssh.map(s => ({ id: s.id, serverId: s.server_id, serverName: s.server_name, sessionType: 'ssh' })),
    { id: local[0].id, serverId: 'zsh', serverName: local[0].shellName, sessionType: 'local', purpose: 'coding_agent', cwd: workspace }],
  files: [fileTab], plugins: [pluginTab], activeSessionId: active,
  activeFileId: scenario === 'files' ? fileTab.id : null, activePluginId: null, detached: [],
  tree: scenario === 'files' ? { direction: 'row', first: sessionPane, second: filePane, splitPercentage: 43 }
    : scenario === 'collaboration' ? { direction: 'row', first: sessionPane, second: pluginPane, splitPercentage: 63 } : sessionPane,
  focusedPane: scenario === 'files' ? filePane : sessionPane,
}));
const ansi = { reset: '\x1b[0m', blue: '\x1b[38;5;75m', green: '\x1b[38;5;78m', dim: '\x1b[38;5;245m' };
const prompt = `${ansi.green}demo@staging-api${ansi.reset} ${ansi.blue}${root}${ansi.reset} $ `;
const terminalText = (id: string) => id === 'demo-session-agent'
  ? `${ansi.blue}Codex · Northstar workspace${ansi.reset}\r\n${ansi.dim}Synthetic agent transcript for documentation${ansi.reset}\r\n\r\n› Add a bounded retry to the health check.\r\n  Keep the last error and show me the diff.\r\n\r\n  Read src/health.ts\r\n  Read tests/health.test.ts\r\n\r\n  The request used to retry without a limit.\r\n  I changed it to three attempts with backoff.\r\n\r\n${ansi.green}✓${ansi.reset} Keep a 5-second timeout per request\r\n${ansi.green}✓${ansi.reset} Preserve the final error\r\n${ansi.green}✓${ansi.reset} Add failure and retry tests\r\n\r\n  src/health.ts          +9  -2\r\n  tests/health.test.ts   +8\r\n\r\n  Review the changes in the panel beside me.\r\n  No deployment or service restart was run.\r\n\r\n› `
  : `${ansi.blue}NORTHSTAR / STAGING${ansi.reset}\r\n${ansi.dim}Documentation fixture · no live SSH connection${ansi.reset}\r\n\r\n${prompt}docker compose ps\r\nNAME          IMAGE                         STATUS\r\napi           northstar/api:preview          Up 2 hours\r\nworker        northstar/worker:preview       Up 2 hours\r\npostgres      postgres:16                   Up 6 days\r\nredis         redis:7                       Up 6 days\r\n\r\n${prompt}curl -fsS localhost:8080/health\r\n${ansi.green}{"status":"ready","queue":"ok","database":"ok"}${ansi.reset}\r\n\r\n${prompt}git diff --stat\r\n src/health.ts         | 11 +++++++++--\r\n tests/health.test.ts  |  8 ++++++++\r\n 2 files changed, 17 insertions(+), 2 deletions(-)\r\n\r\n${ansi.dim}The shared prompt stays yours.\r\nIndependent agent commands appear in activity history.${ansi.reset}\r\n\r\n${prompt}`;
const output = async (id: string, text: string) => emit('session-output', {
  session_id: id, data: btoa(String.fromCharCode(...new TextEncoder().encode(text))),
});
const docker: Record<string, string> = {
  version: 'Docker Engine · demo data',
  containers: 'a1b2c3\tapi\tnorthstar/api:preview\tUp 2 hours (healthy)\t127.0.0.1:8080->8080/tcp\nd4e5f6\tworker\tnorthstar/worker:preview\tUp 2 hours\t9090/tcp\na7b8c9\tpostgres\tpostgres:16\tUp 6 days (healthy)\t5432/tcp\nd0e1f2\tredis\tredis:7\tUp 6 days\t6379/tcp\na3b4c5\tmigrations\tnorthstar/api:preview\tExited (0) 2 hours ago\t',
  stats: 'api\t12.4%\t184 MiB / 2 GiB\nworker\t3.1%\t96 MiB / 1 GiB\npostgres\t1.8%\t312 MiB / 4 GiB\nredis\t0.4%\t24 MiB / 512 MiB',
  logs: '09:40:11 INFO  health endpoint ready\n09:40:14 INFO  database pool connected\n09:41:02 INFO  request GET /health 200 8ms\n09:41:36 INFO  request GET /api/jobs 200 14ms\n',
};
mockWindows('main');
mockIPC(async (cmd, payload) => {
  calls.push(cmd);
  const args = (payload ?? {}) as Record<string, any>;
  const request = args.request ?? args;
  switch (cmd) {
    case 'get_runtime_capabilities': return capabilities;
    case 'get_app_version': return '1.1.0';
    case 'get_servers': return servers;
    case 'get_groups': return [{ id: 'demo-staging', name: 'Staging', color: 'blue' }, { id: 'demo-data', name: 'Data', color: 'green' }, { id: 'demo-platform', name: 'Platform', color: 'purple' }];
    case 'load_settings': return settings;
    case 'save_settings': Object.assign(settings, args.settings); return null;
    case 'sftp_get_upload_ignore_config': return { excludedPaths: ['node_modules/', '.git/', 'target/', '.env'], respectGitignore: true };
    case 'session_list': return ssh;
    case 'local_shell_list_sessions': return local;
    case 'session_attach': await output(request.sessionId, '\x1b[2J\x1b[H' + terminalText(request.sessionId)); return ssh.find(s => s.id === request.sessionId);
    case 'local_shell_attach': await output(request.sessionId, '\x1b[2J\x1b[H' + terminalText(request.sessionId)); return null;
    case 'session_send_input': case 'local_shell_send_input': await output(request.sessionId, request.data); return null;
    case 'session_resize': case 'local_shell_resize': case 'session_detach': case 'local_shell_detach': return null;
    case 'take_pending_open_files': return [];
    case 'workspace_save_handler_ready': return null;
    case 'plugin:window|scale_factor': return 1;
    case 'plugin:window|inner_size': case 'plugin:window|outer_size': return { width: innerWidth, height: innerHeight };
    case 'plugin:window|outer_position': case 'plugin:window|inner_position': return { x: 0, y: 0 };
    case 'plugin:window|is_fullscreen': case 'plugin:window|is_maximized': return false;
    case 'plugin:window|get_all_windows': return ['main'];
    case 'plugin:window|available_monitors': return [{ name: 'Demo display', position: { x: 0, y: 0 }, size: { width: 1440, height: 900 }, scaleFactor: 1, workArea: { position: { x: 0, y: 0 }, size: { width: 1440, height: 900 } } }];
    case 'plugin:window|set_position': case 'plugin:window|set_size': return null;
    case 'cloud_sync_status': return { unlocked: false, syncing: false, provider: null, endpoint: null, vaultId: null, pendingChanges: 0, conflicts: 0, lastSuccessAt: null, lastError: null };
    case 'agent_activity_list': return activity.filter(item => (!args.after || item.sequence > args.after) && (!args.before || item.sequence < args.before));
    case 'get_agent_guard_status': return { autoApproveUntil: null, pending: [] };
    case 'resolve_agent_approval': await emit('agent-approval-resolved', { id: request.id }); return null;
    case 'get_agent_guard_config': return { enabled: true, autoApproveHours: 0 };
    case 'get_agent_gateway_status': return { running: true, endpoint: 'http://127.0.0.1:0/demo-only', manifestPath: '/demo/agent-gateway.json', pid: null, protocolVersion: '2024-11-05' };
    case 'local_shell_list_shells': return [{ id: 'zsh', name: 'Zsh', path: '/bin/zsh', available: true }, { id: 'bash', name: 'Bash', path: '/bin/bash', available: true }];
    case 'local_shell_get_default': return { id: 'zsh', name: 'Zsh', path: '/bin/zsh', available: true };
    case 'coding_agent_list': return ['claude', 'codex', 'opencode', 'pi'].map((id, index) => ({ id, name: ['Claude Code', 'Codex', 'OpenCode', 'Pi'][index], installed: true, executablePath: `/demo/bin/${id}`, startModes: ['new', 'continue_last', 'resume_picker'], accessModes: ['default', 'read_only', 'auto_edit'] }));
    case 'pick_workspace_directory': return workspace;
    case 'coding_agent_workspace_status': return { root: workspace, branch: 'fix/health-retry', files: [{ path: 'src/health.ts', oldPath: null, kind: 'modified', staged: false, unstaged: true }, { path: 'tests/health.test.ts', oldPath: null, kind: 'added', staged: true, unstaged: false }] };
    case 'coding_agent_workspace_diff': return { path: request.path, oldPath: null, truncated: false, content: 'diff --git a/src/health.ts b/src/health.ts\n--- a/src/health.ts\n+++ b/src/health.ts\n@@ -1,6 +1,13 @@\n+import { setTimeout as delay } from "node:timers/promises";\n+\n export async function fetchHealth(url: string) {\n-  const response = await fetch(url);\n-  return response.json();\n+  for (let attempt = 0; attempt < 3; attempt++) {\n+    try {\n+      const response = await fetch(url, {\n+        signal: AbortSignal.timeout(5000),\n+      });\n+      if (!response.ok) throw new Error("Health check failed");\n+      return await response.json();\n+    } catch (error) {\n+      if (attempt === 2) throw error;\n+      await delay(250 * 2 ** attempt);\n+    }\n+  }\n }\n' };
    case 'plugin_list': return plugins;
    case 'plugin_execute': {
      const text = request.pluginId === 'docker-containers' ? docker[request.actionId] : undefined;
      if (text === undefined) break;
      return { pluginId: request.pluginId, actionId: request.actionId, output: text, durationMs: 38, truncated: false };
    }
    case 'get_server_status': return { hostname: 'staging-api', uptimeSeconds: 543210,
      cpu: { usagePercent: 18.4, coreCount: 4, loadAverage: [0.72, 0.61, 0.48] },
      memory: { total: 8589934592, used: 3113851289, free: 5476083303, available: 5476083303, usagePercent: 36.25, swapTotal: 0, swapUsed: 0 },
      disks: [{ mountPoint: '/', filesystem: 'ext4', total: 107374182400, used: 39728447488, available: 67645734912, usagePercent: 37 }],
      network: [{ interface: 'eth0', rxBytes: 102400000, txBytes: 51800000, rxPackets: 9032, txPackets: 5013 }], collectedAt: seconds };
    case 'sftp_init': return true;
    case 'sftp_pwd': return root;
    case 'sftp_list_dir': return [
      ...['src', 'public', 'logs'].map(name => ({ name, path: `${root}/${name}`, isDirectory: true, size: 0, modifiedAt: seconds, permissions: 'drwxr-xr-x' })),
      ...Object.entries(files).filter(([path]) => path.startsWith(request.path + '/') && !path.slice(request.path.length + 1).includes('/')).map(([path, content]) => ({ name: path.split('/').at(-1), path, isDirectory: false, size: content.length, modifiedAt: seconds, permissions: '-rw-r--r--' })),
    ];
    case 'sftp_read_file': case 'local_file_read': {
      const content = files[request.path];
      if (content === undefined) break;
      return { content, isBinary: false, size: content.length, truncated: false, mimeType: 'text/plain' };
    }
    case 'sftp_write_file': if (request.path in files) { files[request.path] = request.content; return null; } break;
    case 'get_credential': return null;
    case 'list_fingerprints': case 'list_recordings': case 'tunnel_config_list': case 'tunnel_list_active': return [];
    case 'is_session_recording': return false;
    case 'get_session_recording_id': return null;
    case 'history_list': return [];
  }
  blocked.push(cmd);
  throw new Error(`Documentation fixture does not implement ${cmd}; native calls are never forwarded.`);
}, { shouldMockEvents: true });

await import('../../src/main');
const { useSessionStore } = await import('../../src/stores/sessionStore');
const { usePluginWorkspaceStore } = await import('../../src/stores/pluginWorkspaceStore');
const { useNavigationStore } = await import('../../src/stores/navigationStore');
const { useAgentApprovalStore } = await import('../../src/stores/agentApprovalStore');
const { useSettingsStore } = await import('../../src/stores/settingsStore');
const { useCustomThemeStore } = await import('../../src/lib/customTheme');
Object.assign(window, { __VIBESHELL_DEMO__: {
  scenario, blocked, calls, errors,
  ready: () => document.querySelectorAll('.xterm-screen').length > 0,
  approveDemo: () => useAgentApprovalStore.setState({ queue: [{ id: 'demo-approval', tool: 'mcp:session_exec', command: 'sudo systemctl restart northstar-api', reasons: ['Elevated privileges', 'Service restart can interrupt active requests'], sessionId: 'demo-session-api', timestamp: time }] }),
  newSession: async () => { ssh.push({ id: 'demo-session-agent-new', server_id: 'demo-api', server_name: 'API · agent inspection', state: 'connected', created_at: seconds + 1, clients: 1 }); await useSessionStore.getState().syncRemoteSessions(); },
  plugin: (pluginId: string) => { usePluginWorkspaceStore.getState().openPluginTab({ pluginId, sessionId: 'demo-session-api', sessionType: 'ssh', serverName: 'API · staging' }); },
  settings: () => useNavigationStore.getState().goToSettings(),
  marketplace: () => useNavigationStore.getState().goToPlugins(),
  theme: async () => { await useSettingsStore.getState().updateAppearanceSettings({ theme: 'paper-white' }); useCustomThemeStore.getState().save(':root {\n  --tokyo-blue: #0d766e !important;\n}\n.session-tabbar [role="tab"] {\n  border-radius: 10px;\n}\n', false); },
  replay: async () => { for (const item of [...ssh, ...local]) await output(item.id, '\x1b[2J\x1b[H' + terminalText(item.id)); },
} });
