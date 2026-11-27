import type { Plugin } from 'vite';

/** Real WKWebView layout checks. Refuses to run against the installed app's data. */
export function themeUiSmoke(): Plugin {
  const id = '/@vibeshell/theme-ui-smoke';
  return {
    name: 'vibeshell-theme-ui-smoke',
    apply: 'serve',
    resolveId(source) { if (source === id) return id; },
    transformIndexHtml() { return [{ tag: 'script', attrs: { type: 'module', src: id }, injectTo: 'head' }]; },
    configureServer(server) {
      server.ws.on('vibeshell:theme-smoke', (report) => console.log('[THEME_UI_SMOKE]', JSON.stringify(report)));
    },
    load(source) {
      if (source !== id) return;
      return `
import { getIdentifier } from '@tauri-apps/api/app';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
import { useSettingsStore } from '/src/stores/settingsStore.ts';
import { useSessionStore } from '/src/stores/sessionStore.ts';
import { usePluginWorkspaceStore } from '/src/stores/pluginWorkspaceStore.ts';
import { useFileWorkspaceStore } from '/src/stores/fileWorkspaceStore.ts';
import { useNavigationStore } from '/src/stores/navigationStore.ts';
import i18n from '/src/i18n/index.ts';
const report = (stage, value) => import.meta.hot.send('vibeshell:theme-smoke', {stage, value});
window.addEventListener('error', event => report('runtime-error', String(event.error?.stack ?? event.message)));
window.addEventListener('unhandledrejection', event => report('runtime-rejection', String(event.reason?.stack ?? event.reason)));
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const visible = selector => [...document.querySelectorAll(selector)].filter(el => el.getBoundingClientRect().width > 0);
function assert(value, message) { if (!value) throw new Error(message); }
async function until(predicate, name) {
  for (let i=0; i<150; i++) { if (predicate()) return; await pause(100); }
  throw new Error('Timed out: '+name);
}
const button = key => visible('button').find(el => el.getAttribute('aria-label') === i18n.t(key));
function material(element) {
  assert(element, 'Missing material surface');
  const css = getComputedStyle(element);
  return {background:css.backgroundColor, reflections:css.backgroundImage, blur:css.backdropFilter || css.webkitBackdropFilter, opacity:css.opacity};
}
function alpha(color) {
  if (color === 'transparent') return 0;
  if (color.includes('/')) return parseFloat(color.split('/')[1]);
  if (color.startsWith('rgba(')) return parseFloat(color.split(',').at(-1));
  return 1;
}
function translucent(element, name) {
  const value = material(element);
  assert(value.reflections === 'none', name+' contains decorative artwork');
  assert(alpha(value.background) < 1, name+' has an opaque backing');
  assert(value.opacity === '1', name+' fades its text');
  return value;
}
function assertClearAncestors(element) {
  for (let parent = element.parentElement; parent; parent = parent.parentElement) {
    const css = getComputedStyle(parent);
    assert(css.backgroundColor === 'rgba(0, 0, 0, 0)', 'Opaque ancestor blocks native material: '+parent.tagName+'.'+parent.className+' '+css.backgroundColor);
    assert(css.backgroundImage === 'none', 'Artwork on ancestor: '+parent.className);
    assert(css.opacity === '1', 'Parent fades text: '+parent.className);
  }
}
function snapshot() {
  const terminal = visible('.terminal-viewport')[0];
  const bar = visible('.titlebar-shell')[0];
  const tab = visible('[role=tab]')[0];
  const xterm = terminal.querySelector('.xterm-screen');
  const rect = el => { const r = el.getBoundingClientRect(); return { x:r.x, y:r.y, width:r.width, height:r.height }; };
  return {
    theme: document.documentElement.dataset.theme,
    nativeGlass: document.documentElement.dataset.nativeGlass,
    mask: getComputedStyle(document.documentElement).getPropertyValue('--vibe-terminal-mask').trim(),
    background: getComputedStyle(terminal).backgroundColor,
    blur: getComputedStyle(terminal).backdropFilter || getComputedStyle(terminal).webkitBackdropFilter,
    textOpacity: getComputedStyle(xterm).opacity,
    rendererBackground: getComputedStyle(terminal.querySelector('.xterm-viewport')).backgroundColor,
    tabBackground: getComputedStyle(tab).backgroundColor,
    tabReflections: getComputedStyle(tab).backgroundImage,
    chrome: material(bar),
    terminalReflections: getComputedStyle(terminal).backgroundImage,
    bodyBackground: getComputedStyle(document.body).backgroundColor,
    viewport: {width:innerWidth, height:innerHeight}, terminal:rect(terminal), bar:rect(bar), tab:rect(tab),
    toolbars:visible('.mosaic-window-toolbar').length,
    logos:visible('.titlebar-brand, .titlebar-session, .session-rail').length,
  };
}
async function run() {
  if (await getIdentifier() !== 'com.vibeshell.glass-preview') { report('skipped', 'Isolated preview identifier required'); return; }
  await getCurrentWindow().setTitle('VibeShell · Glass Preview');
  await until(() => useSettingsStore.getState().initialized && useSessionStore.getState().sessions.length, 'workspace ready');
  // Remove only legacy performance tabs in this isolated QA workspace. The
  // direct shortcut now opens a bottom pane, so these old test tabs mislead QA.
  usePluginWorkspaceStore.getState().tabs.filter(tab => tab.pluginId === 'server-performance').forEach(tab => usePluginWorkspaceStore.getState().closeTab(tab.id));
  await until(() => visible('[data-tab-kind=session]').length, 'session tab rendered');
  // Restore may select the last plugin tab; use the actual UI after hydration.
  visible('[data-tab-kind=session]')[0].click();
  if (button('sidebar.sftp')?.getAttribute('aria-pressed') === 'true') button('sidebar.sftp').click();
  await until(() => {
    if (visible('.terminal-viewport .xterm-screen').length) return true;
    visible('[data-tab-kind=session]')[0]?.click();
    return false;
  }, 'visible terminal');
  assert(visible('.terminal-viewport').length === 1, 'Expected one preview terminal');
  await useSettingsStore.getState().updateAppearanceSettings({themeMode:'manual', theme:'paper-white', terminalBackgroundOpacity:0.9, windowMaterialOpacity:0.12});
  await pause(500);
  for (const theme of ['paper-white','ink-black']) {
    await useSettingsStore.getState().updateAppearanceSettings({themeMode:'manual', theme});
    await pause(400);
    const state = snapshot();
    assert(state.mask === '90%', 'Mask is not 90%');
    assert(state.textOpacity === '1', 'Terminal text must be opaque');
    assert(state.rendererBackground.startsWith('rgba(') && state.rendererBackground.endsWith(', 0)'), 'xterm painted over the glass surface');
    assert(state.terminal.x === 0 && state.terminal.y === state.bar.height, 'Terminal has a gutter or duplicate header');
    assert(Math.abs(state.terminal.width - state.viewport.width) < 1, 'Terminal is not edge-to-edge');
    assert(Math.abs(state.terminal.height + state.bar.height - state.viewport.height) < 1, 'Terminal leaves bottom padding');
    assert(state.tab.y < state.bar.height, 'Tabs are not in the title bar');
    assert(state.toolbars === 0 && state.logos === 0, 'Redundant chrome remains');
    assert(state.nativeGlass === 'true', 'Native window material is not enabled');
    assert(state.chrome.reflections === 'none' && state.terminalReflections === 'none' && state.tabReflections === 'none', 'Decorative gradients remain');
    assert(getComputedStyle(document.querySelector('.app-shell')).backgroundImage === 'none', 'Fake wallpaper remains');
    assert(state.bodyBackground === 'rgba(0, 0, 0, 0)', 'WebView body blocks native material');
    assertClearAncestors(visible('.terminal-viewport')[0]);
    assertClearAncestors(visible('.titlebar-shell')[0]);
    translucent(visible('[role=tab]')[0], 'Session tab');
    for (const key of ['sidebar.sftp','sidebar.performance','sidebar.history','common.more']) {
      const action = button(key);
      assert(action, 'Missing direct action: '+key);
      assert(action.textContent.trim() === '' && action.querySelector('svg'), 'Top action is not icon-only: '+key);
      assert(action.getAttribute('data-tooltip') === i18n.t(key), 'Missing tooltip: '+key);
      assert(action.getBoundingClientRect().width >= 32, 'Small action hit target: '+key);
    }
    report('verified-theme', state);
  }
  button('common.more').click();
  await until(() => visible('.glass-menu').length, 'overflow menu');
  const menu = visible('.glass-menu')[0];
  for (const key of ['settings.dayMode','settings.nightMode','settings.themeModeSystem','sidebar.sftp','plugins.catalog.server-performance.name','session.splitHorizontal','session.splitVertical','session.closeSplits']) {
    assert(![...menu.querySelectorAll('[role=menuitem]')].some(el => el.textContent.trim() === i18n.t(key)), 'Removed item still in menu: '+key);
  }
  assert(material(menu).blur.includes('blur('), 'Menu has no frosted material');
  report('menu-verified', material(menu));
  button('common.more').click();

  button('sidebar.sftp').click();
  await until(() => visible('.sftp-panel-shell').length && visible('[data-testid=sftp-toolbar]').length, 'file manager');
  const files = visible('.sftp-panel-shell')[0];
  assert(material(files).reflections === 'none', 'File manager has fake gradients');
  report('sftp-verified', material(files));
  button('sidebar.sftp').click();

  const tabsBefore = usePluginWorkspaceStore.getState().tabs.length;
  button('sidebar.performance').click();
  await until(() => visible('.server-status-panel').length, 'performance monitor');
  await until(() => visible('.metric-card').length === 4, 'live performance metrics');
  assert(!visible('button').some(el => [i18n.t('plugins.splitRight'), i18n.t('plugins.splitDown')].includes(el.getAttribute('aria-label'))), 'Plugin split controls still visible');
  const terminal = visible('.terminal-viewport')[0];
  const dock = visible('.performance-dock')[0];
  assert(terminal && dock, 'Performance must not replace the terminal');
  const tr = terminal.getBoundingClientRect(), dr = dock.getBoundingClientRect();
  assert(Math.abs(tr.bottom - dr.top) < 2, 'Performance is not docked directly below the terminal');
  assert(tr.height > 100, 'Performance squeezed out the terminal');
  assert(usePluginWorkspaceStore.getState().tabs.length === tabsBefore, 'Performance created a workspace tab');
  report('performance-verified', {cards:visible('.metric-card').length, material:material(dock), terminalHeight:tr.height, dockY:dr.y, dockHeight:dr.height});
  button('sidebar.performance').click();
  await until(() => visible('.performance-dock').length === 0, 'close bottom performance pane');

  useNavigationStore.getState().goToSettings();
  await until(() => visible('.appearance-modes button').length === 3, 'theme controls in settings');
  assert(visible('.terminal-viewport').length === 0 && visible('.performance-dock').length === 0, 'Workspace bleeds through settings');
  button('settings.dayMode')?.click();
  visible('.appearance-modes button')[0].click();
  await until(() => document.documentElement.dataset.appearance === 'light', 'settings day switch');
  visible('.appearance-modes button')[1].click();
  await until(() => document.documentElement.dataset.appearance === 'dark', 'settings night switch');
  const slider = visible('input[type=range]').find(el => el.getAttribute('aria-label') === i18n.t('settings.windowMaterialOpacity'));
  assert(slider, 'Window material control missing');
  report('settings-verified', {modes:3, windowTint:slider.value, terminalMask:useSettingsStore.getState().settings.appearance.terminalBackgroundOpacity});
  const native = getCurrentWindow();
  const oldSize = await native.innerSize();
  await native.setSize(new LogicalSize(800, 600));
  for (const theme of ['paper-white', 'ink-black']) {
    await useSettingsStore.getState().updateAppearanceSettings({themeMode:'manual', theme});
    await pause(300);
    const page = document.querySelector('.settings-page');
    assert(page.querySelector('.settings-section').id === 'settings-appearance', 'Appearance is buried below other settings');
    const sections = visible('.settings-section-body');
    assert(sections.length >= 8, 'Settings sections missing');
    sections.forEach((section, index) => translucent(section, 'Settings section '+index));
    const fields = visible('.settings-row input:not([type=range]), .settings-row select, .settings-row textarea');
    for (const field of fields) {
      translucent(field, 'Setting field');
      assert(field.getAttribute('aria-label') || document.getElementById(field.getAttribute('aria-labelledby'))?.textContent, 'Unnamed settings control');
    }
    assert(page.scrollWidth <= page.clientWidth + 1, 'Settings overflow at minimum window width');
    assert(document.documentElement.scrollWidth <= innerWidth + 1, 'Page scrolls horizontally');
    const firstField = fields[0];
    firstField.focus();
    assert(document.activeElement === firstField, 'Settings field cannot be focused');
    report('settings-controls-verified', {theme, sections:sections.length, fields:fields.length, width:innerWidth});

    useNavigationStore.getState().goToPlugins();
    await until(() => visible('.plugin-marketplace').length && visible('.plugin-catalog-row').length, 'marketplace');
    visible('.plugin-catalog-row').forEach(row => translucent(row, 'Plugin directory row'));
    translucent(document.querySelector('.plugin-marketplace input'), 'Plugin search');
    assert(document.documentElement.scrollWidth <= innerWidth + 1, 'Marketplace overflows');
    assert(visible('.plugin-market-card').length === 0, 'Legacy plugin cards remain');
    report('marketplace-verified', {theme, rows:visible('.plugin-catalog-row').length});

    useNavigationStore.getState().goToMain();
    await until(() => button('session.newSession'), 'main actions');
    button('session.newSession').click();
    await until(() => visible('[role=dialog]').length, 'connection launcher');
    const dialog = visible('[role=dialog]')[0];
    translucent(dialog, 'Connection dialog');
    const search = dialog.querySelector('input');
    if (search) translucent(search, 'Connection search');
    report('dialog-verified', {theme, material:material(dialog)});
    dialog.querySelector('button[aria-label="'+i18n.t('common.close')+'"]').click();
    await until(() => visible('[role=dialog]').length === 0, 'dismiss launcher');
    useNavigationStore.getState().goToSettings();
    await until(() => visible('.settings-section-body').length, 'return to settings');
  }
  await native.setSize(oldSize);
  useNavigationStore.getState().goToMain();
  await until(() => visible('.terminal-viewport').length, 'return to terminal');
  button('sidebar.performance').click();
  await until(() => visible('.performance-dock').length, 'show bottom pane for inspection');
  // Native app rendering continues to be interactive for visual and menu checks.
  report('passed', {themes:2, surfaces:['terminal','titlebar-icons','menu','sftp','performance-dock','settings-controls','marketplace','dialogs'], noGradients:true, isolated:true});
}
run().catch(error => report('failed', String(error)));
`;
    },
  };
}
