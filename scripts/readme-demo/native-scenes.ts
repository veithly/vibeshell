import { getCurrentWindow, getAllWindows, availableMonitors, LogicalPosition, LogicalSize } from '@tauri-apps/api/window';
import { useFileWorkspaceStore } from '../../src/stores/fileWorkspaceStore';
import { useNavigationStore } from '../../src/stores/navigationStore';
import { usePluginWorkspaceStore } from '../../src/stores/pluginWorkspaceStore';

const report = (value: unknown) => import.meta.hot?.send('vibeshell:readme-report', value);
const scene = new URLSearchParams(location.search).get('scene') ?? 'workspace';
const visible = (s: string) => [...document.querySelectorAll<HTMLElement>(s)].filter(e => e.getBoundingClientRect().width > 0);
async function until(check: () => unknown, name: string) {
  for (let n = 0; n < 150; n++) { if (check()) return; await new Promise(r => setTimeout(r, 100)); }
  throw new Error('Missing scene element: ' + name);
}
function click(label: string) {
  const el = visible('button').find(e => e.getAttribute('aria-label') === label || e.textContent?.trim() === label);
  if (!el) throw new Error('Missing button ' + label);
  el.click();
}
const openFile = (name: string) => useFileWorkspaceStore.getState().openFile({ sessionId: 'demo-session-api', path: '/srv/northstar/' + name, name, size: 512 });

let navigating = false;
async function changeScene({ scene: next, ticket }: { scene: string; ticket: number }) {
  if (navigating) return;
  navigating = true;
  for (const win of await getAllWindows()) if (win.label.startsWith('detach-')) await win.destroy();
  location.href = '/scripts/readme-demo/index.html?scene=' + encodeURIComponent(next) + '&capture=' + ticket;
}
import.meta.hot?.on('vibeshell:readme-scene', changeScene);

async function run() {
  const win = getCurrentWindow();
  // Native screenshot work requires a separate virtual monitor. If it is not
  // present, fail closed rather than moving this window onto a physical screen.
  const monitors = await availableMonitors();
  const captureMonitor = monitors.find(
    (monitor) => monitor.name?.includes('VibeShell README Capture')
      || (monitor.position.x < 0 && monitor.size.width === 1440 && monitor.size.height === 900)
  );
  if (!captureMonitor) {
    throw new Error('An isolated 1440x900 VibeShell README Capture virtual display is required for native screenshots.');
  }
  if (scene === 'panes' || scene === 'panes-day') {
    // Permission exists only in the capture config, never the shipped app.
    await win.setAlwaysOnTop(true);
  }
  await win.setSize(new LogicalSize(1200, 800));
  const capturePosition = new LogicalPosition(
    captureMonitor.position.x + 110, captureMonitor.position.y + 44
  );
  await win.setPosition(capturePosition);
  await win.show();
  await win.setFocus();
  report({
    stage: 'positioned',
    scene,
    monitors: monitors.map(monitor => ({
      name: monitor.name, x: monitor.position.x, y: monitor.position.y,
      width: monitor.size.width, height: monitor.size.height,
    })),
    requestedPosition: capturePosition,
    actualPosition: await win.outerPosition(),
  });
  await win.setTitle('VibeShell · README Preview');
  await until(() => scene === 'files' ? visible('[data-tab-kind=file]').length : visible('.xterm-screen').length, 'workspace');
  await document.fonts.ready;
  if (scene === 'workspace') { click('Performance monitor'); await until(() => visible('.metric-card').length === 4, 'metrics'); click('Command history'); }
  if (scene === 'completion') {
    await until(() => document.querySelector('.terminal-viewport textarea'), 'terminal input');
    const input = document.querySelector('.terminal-viewport textarea') as HTMLTextAreaElement;
    input.focus();
    input.value = 'docker ';
    input.dispatchEvent(new InputEvent('input', {bubbles:true, data:'docker ', inputType:'insertText'}));
    await new Promise(r => setTimeout(r, 180));
    input.dispatchEvent(new KeyboardEvent('keydown', {key:' ', code:'Space', ctrlKey:true, bubbles:true, cancelable:true}));
    await until(() => visible('[role=listbox]').length, 'command completion');
  }
  if (scene === 'panes' || scene === 'panes-day') {
    // Verify four live product Mosaic tiles IN ONE native window. This is the
    // primary single-window composition feature, not a desktop window collage.
    await until(() => visible('.xterm-screen').length >= 2, 'two terminal panes');
    await until(() => visible('[data-vibe-surface=markdown]').length > 0, 'Markdown preview pane');
    await until(() => visible('.plugin-workspace, .mosaic-window').length >= 4
      && visible('.plugin-workspace, .mosaic-window').some(el => el.textContent?.includes('Docker')), 'plugin pane');
    const labels = (await getAllWindows()).map(w => w.label);
    if (labels.length !== 1 || labels[0] !== 'main') throw new Error('Expected exactly one main native window: ' + labels);
    const leaves = new Set(visible('[data-pane-id]').map(el => el.dataset.paneId));
    for (const id of ['session:demo-session-api', 'session:demo-session-worker',
      'file:' + encodeURIComponent('demo-session-api\u0000/srv/northstar/RUNBOOK.md'),
      'plugin:demo-session-api::docker-containers']) {
      if (!leaves.has(id)) throw new Error('Missing single-window pane: ' + id);
    }
  }
  if (scene === 'sftp' || scene === 'extract') {
    click('SFTP'); await until(() => visible('[data-testid=sftp-toolbar]').length, 'file manager');
    openFile('nginx.conf'); await until(() => visible('.file-code-editor').length, 'remote editor');
    if (scene === 'extract') {
      const row = visible('tr').find(el => el.textContent?.includes('release.zip'));
      if (!row) throw new Error('Archive not in SFTP');
      const r = row.getBoundingClientRect();
      row.dispatchEvent(new MouseEvent('contextmenu', {bubbles:true, clientX:r.x+100, clientY:r.y+20, button:2}));
      await until(() => visible('[role=menuitem]').some(e => e.textContent?.includes('Extract here')), 'extract action');
    }
  }
  if (scene === 'files') {
    openFile('RUNBOOK.md'); await until(() => visible('button').some(e => e.textContent?.trim() === 'Source and preview'), 'Markdown controls');
    click('Source and preview');
  }
  if (scene === 'plugins') {
    usePluginWorkspaceStore.getState().openPluginTab({pluginId:'docker-containers',sessionId:'demo-session-api',sessionType:'ssh',serverName:'API · staging'});
    await until(() => visible('tbody tr').length >= 5, 'Docker rows');
  }
  if (scene === 'settings') { useNavigationStore.getState().goToSettings(); await until(() => visible('.appearance-modes').length, 'appearance'); }
  if (scene === 'coding') {
    click('More actions');
    await until(() => visible('[role=menu]').length, 'workspace menu');
    click('Workspace changes');
    await until(() => visible('.workspace-diff-line').length, 'Git diff');
  }
  if (scene === 'approval') { (window as any).__VIBESHELL_DEMO__.approveDemo(); await until(() => visible('[role=alertdialog]').length, 'approval'); }
  if (scene !== 'completion') (document.activeElement as HTMLElement)?.blur();
  await new Promise(r => setTimeout(r, 650));
  const demo = (window as any).__VIBESHELL_DEMO__;
  report({stage:'ready', scene, native:true, width:innerWidth, height:innerHeight, windows:(await getAllWindows()).map(w=>w.label), blocked:demo.blocked, errors:demo.errors});
}
run().then(() => {
  // WKWebView can briefly reconnect its dev WebSocket after native theme
  // changes. Polling the fixed scene request makes capture resilient to that
  // delivery gap without accepting arbitrary scripts or native operations.
  const ticket = Number(new URLSearchParams(location.search).get('capture') ?? 0);
  setInterval(() => {
    void fetch('/__readme/requested', {cache:'no-store'}).then(r => r.json()).then(next => {
      if (next.ticket > ticket) return changeScene(next);
    }).catch(error => report({stage:'driver-error',scene,error:String(error)}));
  }, 500);
}).catch(e => report({stage:'failed',scene,error:String(e)}));
