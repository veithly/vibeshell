import type { Plugin } from 'vite';
import { resolve } from 'node:path';

/** Fixed, opt-in native UI scenario. Only project-owned fixture files are read. */
export function documentUiSmoke(): Plugin {
  const id = '/@vibeshell/document-ui-smoke';
  const root = resolve(process.cwd());
  return {
    name: 'vibeshell-document-ui-smoke', apply: 'serve',
    resolveId(source) { if (source === id) return id; },
    transformIndexHtml() { return [{ tag: 'script', attrs: { type: 'module', src: id }, injectTo: 'head' }]; },
    configureServer(server) { server.ws.on('vibeshell:document-smoke', report => console.log('[DOCUMENT_UI_SMOKE]', JSON.stringify(report))); },
    load(source) {
      if (source !== id) return;
      return `
import i18n from '/src/i18n/index.ts';
import { openLocalFiles } from '/src/lib/localFiles.ts';
import { useFileWorkspaceStore } from '/src/stores/fileWorkspaceStore.ts';
import { useSessionStore } from '/src/stores/sessionStore.ts';
import { openDetachedWindow, useDetachedOwnership, parseDetachTarget } from '/src/lib/detach.ts';
import { useCustomThemeStore, customTerminalColors, wallpaperCss } from '/src/lib/customTheme.ts';
import { physicalPosition } from '/src/lib/physicalPixels.ts';
import { getCurrentWindow } from '@tauri-apps/api/window';
const root=${JSON.stringify(root)};
const native=getCurrentWindow();
const report=(stage,value)=>import.meta.hot.send('vibeshell:document-smoke',{label:native.label,stage,value});
const pause=ms=>new Promise(r=>setTimeout(r,ms));
async function until(predicate,name){const start=Date.now();while(Date.now()-start<18000){const result=predicate();if(result)return result;await pause(50);}throw new Error('Timeout: '+name);}
function check(value,name){if(!value)throw new Error(name);}
function click(element){check(element&&!element.disabled,'Missing or disabled button');const r=element.getBoundingClientRect();const hit=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);check(hit===element||element.contains(hit),'Button is covered');element.click();}
async function run(){
  await pause(1400);
  if(native.label!=='main'){
    const target=parseDetachTarget(location.search);
    if(target?.kind!=='file'||target.path!==root+'/test-fixtures/documents/sample.md')return;
    const back=await until(()=>[...document.querySelectorAll('button')].find(b=>b.ariaLabel===i18n.t('workspaceLayout.mergeBack')&&!b.disabled),'detached file ready');
    await until(()=>document.querySelector('.vibe-markdown h1'),'detached Markdown preview');
    check(useFileWorkspaceStore.getState().tabs.some(t=>t.source==='local'),'Local source retained');
    check(getComputedStyle(document.documentElement).getPropertyValue('--tokyo-blue').trim()==='#126789','CSS in detached window');
    report('detached-file-and-css',true);click(back);return;
  }
  if(sessionStorage.getItem('vibeshell.document-smoke.done'))return;
  sessionStorage.setItem('vibeshell.document-smoke.done','running');
  await until(()=>document.querySelector('.session-new-action'),'main workspace ready');
  const before=useSessionStore.getState().sessions.map(s=>s.id);
  const theme=useCustomThemeStore.getState();const saved={css:theme.css,enabled:theme.enabled};
  const position=await native.outerPosition();
  const files=[root+'/test-fixtures/documents/sample.md',root+'/test-fixtures/documents/sample.svg',root+'/test-fixtures/documents/sample.txt',root+'/src-tauri/icons/32x32.png'];
  const opened=[];
  try{
    await native.setTitle('VibeShell · Document verification');
    await native.setPosition(physicalPosition(1277.640625,210.125));
    const moved=await native.outerPosition();check(Number.isInteger(moved.x)&&Number.isInteger(moved.y),'Native i32 position');
    report('fractional-position-accepted',{x:moved.x,y:moved.y});
    await native.setPosition(physicalPosition(position.x,position.y));
    for(const path of files){
      await openLocalFiles([path]);
      const tab=await until(()=>useFileWorkspaceStore.getState().tabs.find(t=>t.path===path),'file tab '+path);
      opened.push(tab.id);
      if(path.endsWith('.md')){
        await until(()=>document.querySelector('.vibe-markdown h1')?.textContent==='VibeShell Document Check','Markdown heading');
        await until(()=>document.querySelector('.vibe-markdown img')?.naturalWidth>0,'relative SVG image');
        click([...document.querySelectorAll('button')].find(b=>b.textContent===i18n.t('localFiles.split')));
        await until(()=>document.querySelector('.markdown-workspace.is-split textarea'),'Markdown source and preview');
      }else if(tab.kind==='image'){
        await until(()=>[...document.querySelectorAll('img')].some(img=>img.alt===tab.name&&img.naturalWidth>0),'image decoded '+tab.name);
      }else{
        await until(()=>[...document.querySelectorAll('textarea')].some(el=>el.value.includes('中文 UTF-8')),'text editor');
      }
      report('file-opened',{name:tab.name,kind:tab.kind,source:tab.source});
    }
    check(JSON.stringify(before)===JSON.stringify(useSessionStore.getState().sessions.map(s=>s.id)),'Opening documents created a shell');
    const css=':root { --tokyo-blue: #126789 !important; }'+wallpaperCss('data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciLz4=');
    useCustomThemeStore.getState().save(css,true);
    await until(()=>getComputedStyle(document.documentElement).getPropertyValue('--tokyo-blue').trim()==='#126789','applied CSS');
    check(customTerminalColors().cursor==='#126789','Terminal palette CSS');
    check(getComputedStyle(document.querySelector('.app-shell'),'::after').pointerEvents==='none','Wallpaper does not intercept clicks');
    const target={kind:'file',source:'local',sessionId:'local-files',path:files[0],name:'sample.md',size:300};
    const label=await openDetachedWindow(target);check(label,'Native file tear-out');
    await until(()=>!Object.values(useDetachedOwnership.getState().owners).includes(label),'file returns to main window');
    useCustomThemeStore.getState().preview('body { visibility: hidden !important; }');
    await until(()=>getComputedStyle(document.body).visibility==='hidden','hidden CSS preview');
    window.dispatchEvent(new KeyboardEvent('keydown',{key:'F12',metaKey:true,shiftKey:true,bubbles:true,cancelable:true}));
    await until(()=>!document.getElementById('vibeshell-custom-css'),'emergency CSS disable');
    report('passed',{files:files.length,sessionCountUnchanged:true,markdownSplit:true,nativeFileReturn:true,cssPalette:true,wallpaper:true,emergencyRecovery:true});
    sessionStorage.setItem('vibeshell.document-smoke.done','passed');
  }finally{
    useCustomThemeStore.getState().save(saved.css,saved.enabled);
    for(const id of opened)useFileWorkspaceStore.getState().closeTab(id);
    await native.setPosition(physicalPosition(position.x,position.y));
  }
}
run().catch(error=>report('failed',String(error)+' '+String(error.stack??'')));
`;
    },
  };
}
