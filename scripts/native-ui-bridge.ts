import type { Plugin } from 'vite';

/** Opt-in, fixed native WebView smoke scenario. No HTTP control endpoint or supplied code. */
export function nativeUiBridge(): Plugin {
  const id = '/@vibeshell/native-ui-smoke';
  return {
    name: 'vibeshell-native-ui-smoke', apply: 'serve',
    resolveId(source) { if (source === id) return id; },
    transformIndexHtml() { return [{ tag: 'script', attrs: { type: 'module', src: id }, injectTo: 'head' }]; },
    configureServer(server) { server.ws.on('vibeshell:ui-smoke:report', data => console.log('[NATIVE_UI_SMOKE]', JSON.stringify(data))); },
    load(source) {
      if (source !== id) return;
      return `
import i18n from '/src/i18n/index.ts';
import { useSessionStore } from '/src/stores/sessionStore.ts';
import { captureTransfer, parseDetachTarget, useDetachedOwnership } from '/src/lib/detach.ts';
import { requestDock } from '/src/lib/nativeDock.ts';
import { getCurrentWindow, getAllWindows } from '@tauri-apps/api/window';
const label = getCurrentWindow().label;
const report = (stage, value) => import.meta.hot.send('vibeshell:ui-smoke:report', {label,stage,value});
const pause = ms => new Promise(r=>setTimeout(r,ms));
const checkpointKey = 'vibeshell.ui-smoke.return';
const errors = [];
window.addEventListener('error',e=>{errors.push(String(e.error?.stack??e.message));report('error',errors.at(-1));});
window.addEventListener('unhandledrejection',e=>{errors.push(String(e.reason?.stack??e.reason));report('rejection',errors.at(-1));});
async function until(predicate, name, timeout=15000){const start=Date.now();while(Date.now()-start<timeout){const v=predicate();if(v)return v;await pause(50);}throw new Error('Timed out: '+name);}
function click(el){if(!el||el.disabled)throw new Error('Unavailable button');const r=el.getBoundingClientRect();const h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(el!==h&&!el.contains(h))throw new Error('Button covered: '+(el.ariaLabel||el.textContent)+' by '+h?.outerHTML.slice(0,300));el.dispatchEvent(new MouseEvent('click',{bubbles:true,cancelable:true,detail:1}));}
const pane = id => [...document.querySelectorAll('[data-pane-id]')].find(e=>e.dataset.paneId==='session:'+id&&e.getBoundingClientRect().width);
const tab = id => [...document.querySelectorAll('[data-tab-kind=session]')].find(e=>e.dataset.tabId===id);
const panes = () => [...document.querySelectorAll('[data-pane-id]')].filter(e=>e.getBoundingClientRect().width).map(e=>e.dataset.paneId);
function drag(source, target, side){
  const from=source.getBoundingClientRect();const x=from.x+from.width/2,y=from.y+from.height/2;
  const r=target?.getBoundingClientRect();const tx=r?r.x+r.width*(side==='left'?.08:side==='right'?.92:.5):x;
  const ty=r?r.y+r.height*(side==='top'?.08:side==='bottom'?.92:.5):20;
  const send=(el,type,cx,cy,buttons)=>el.dispatchEvent(new MouseEvent(type,{bubbles:true,cancelable:true,button:0,buttons,clientX:cx,clientY:cy,screenX:window.screenX+cx,screenY:window.screenY+cy,detail:1}));
  send(source,'mousedown',x,y,1);send(document.body,'mousemove',tx,ty,1);send(document.body,'mouseup',tx,ty,0);
}
async function run(){
  await pause(1200);
  report('alive',{buttons:[...document.querySelectorAll('button')].filter(e=>e.getBoundingClientRect().width).map(e=>e.ariaLabel||e.title||e.textContent),errors});
  if(label!=='main'){
    const checkpoint=JSON.parse(localStorage.getItem(checkpointKey)??'null');
    const target=parseDetachTarget(location.search);
    if(!checkpoint||target?.sessionId!==checkpoint.sessionId)return;
    const back=await until(()=>[...document.querySelectorAll('button')].find(e=>e.ariaLabel===i18n.t('workspaceLayout.mergeBack')&&!e.disabled),'detached ready');
    await pause(600);
    report('detached-ready',{sessionId:target.sessionId,sessionType:useSessionStore.getState().sessions[0]?.sessionType});
    if(checkpoint.mode==='point'){
      const accepted=await requestDock(captureTransfer(target),checkpoint.point);
      report('drop-accepted',{accepted});if(accepted)await getCurrentWindow().destroy();
    }else{click(back);report('merge-button-clicked',true);}
    return;
  }
  if(sessionStorage.getItem('vibeshell.ui-smoke.v5'))return;
  sessionStorage.setItem('vibeshell.ui-smoke.v5','running');
  await until(()=>useSessionStore.getState().sessions.length,'initial session');
  const restart=JSON.parse(localStorage.getItem('vibeshell.ui-smoke.restart')??'null');
  if(restart){
    await until(()=>panes().length===2,'restored two-pane layout');
    const sessions=useSessionStore.getState().sessions;
    if(sessions.length!==2||sessions[0].serverId!==restart.originalShell||sessions[1].serverId!==restart.testShell)throw new Error('Unexpected workspace; preserve it instead of cleanup');
    const layout=JSON.parse(localStorage.getItem('vibeshell.workspace-layout.v2'));
    if(layout.tree.direction!=='column'||layout.tree.splitPercentage!==50)throw new Error('Restored layout differs');
    report('restart-restored',{oldIds:restart.ids,newIds:sessions.map(s=>s.id),tree:layout.tree,position:await getCurrentWindow().outerPosition(),size:await getCurrentWindow().innerSize()});
    localStorage.removeItem('vibeshell.ui-smoke.restart');
    click(tab(sessions[1].id).querySelector('button[aria-label^=Close]'));
    await until(()=>document.querySelector('[role=alertdialog]'),'close confirmation');
    click([...document.querySelectorAll('[role=alertdialog] button')].find(e=>e.textContent===i18n.t('session.closeLocalShell')));
    await until(()=>useSessionStore.getState().sessions.length===1,'close restored test session');
    report('passed',{closeSession:true,restartLayout:true,remainingSessions:useSessionStore.getState().sessions.map(s=>s.id),errors});return;
  }
  // Recover only a test session explicitly recorded by this smoke scenario.
  const interrupted=sessionStorage.getItem('vibeshell.ui-smoke.session');
  if(useSessionStore.getState().sessions.some(s=>s.id===interrupted)){
    click(tab(interrupted).querySelector('button[aria-label^=Close]'));
    await until(()=>document.querySelector('[role=alertdialog]'),'recover test confirmation');
    click([...document.querySelectorAll('[role=alertdialog] button')].find(e=>e.textContent===i18n.t('session.closeLocalShell')));
    await until(()=>!useSessionStore.getState().sessions.some(s=>s.id===interrupted),'cleanup previous test');
    await until(()=>!document.querySelector('[role=alertdialog]'),'cleanup confirmation gone');
  }
  const before=useSessionStore.getState().sessions.map(s=>s.id);
  if(before.length!==1){report('skipped-layout-test','Existing multi-session workspace preserved');return;}
  const original=before[0];
  click(document.querySelector('.session-new-action'));
  const dialog=await until(()=>document.querySelector('[role=dialog]'),'new dialog');
  click([...dialog.querySelectorAll('button')].find(e=>e.ariaLabel===i18n.t('common.close')));
  await until(()=>!document.querySelector('[role=dialog]'),'close dialog');report('dialog-open-close',true);
  click(document.querySelector('.session-new-action'));
  await until(()=>document.querySelector('[role=dialog]'),'new dialog again');
  const shell=await until(()=>[...document.querySelectorAll('[role=dialog] button.connection-card')].find(e=>e.textContent.includes('/bin/sh')),'POSIX shell choice');
  await pause(300);
  click(shell);
  const created=await until(()=>useSessionStore.getState().sessions.find(s=>!before.includes(s.id)),'new shell');
  const testId=created.id;sessionStorage.setItem('vibeshell.ui-smoke.session',testId);
  report('new-session-created',{id:testId,type:created.sessionType});
  await until(()=>!document.querySelector('[role=dialog]'),'launcher dismissed');
  click(tab(original));await pause(200);
  drag(tab(testId),pane(original),'left');await until(()=>panes().length===2,'left split');report('split-left',panes());
  drag(tab(testId),pane(original),'top');await pause(300);report('move-to-top',panes());
  localStorage.setItem(checkpointKey,JSON.stringify({sessionId:testId,mode:'button'}));
  drag(tab(testId),null);await until(()=>useDetachedOwnership.getState().owners['session:'+testId],'tear-out');
  report('torn-out',{windows:(await getAllWindows()).map(w=>w.label),panes:panes()});
  await until(()=>!useDetachedOwnership.getState().owners['session:'+testId],'merge-back');
  await pause(200);report('merged-tab',{sessions:useSessionStore.getState().sessions.map(s=>s.id),panes:panes()});
  click(tab(original));await pause(200);
  const r=pane(original).getBoundingClientRect();const win=getCurrentWindow();const origin=await win.innerPosition(),scale=await win.scaleFactor();
  localStorage.setItem(checkpointKey,JSON.stringify({sessionId:testId,mode:'point',point:{x:origin.x+(r.x+r.width*.5)*scale,y:origin.y+(r.y+r.height*.92)*scale}}));
  drag(tab(testId),null);await until(()=>useDetachedOwnership.getState().owners['session:'+testId],'tear-out again');
  await until(()=>!useDetachedOwnership.getState().owners['session:'+testId],'drop back into bottom split');
  await until(()=>panes().length===2,'bottom split');report('native-drop-bottom',panes());
  click([...document.querySelectorAll('button')].find(e=>e.ariaLabel===i18n.t('workspaceLayout.save')));await pause(500);
  report('saved-layout',JSON.parse(localStorage.getItem('vibeshell.workspace-layout.v2'))?.tree);
  localStorage.removeItem(checkpointKey);
  click(tab(testId).querySelector('button[aria-label^=Close]'));
  await pause(100);
  click([...document.querySelectorAll('button')].find(e=>e.textContent===i18n.t('common.cancel')));
  await until(()=>!document.querySelector('[role=alertdialog]'),'cancel confirmation gone');
  if(!useSessionStore.getState().sessions.some(s=>s.id===testId))throw new Error('Cancel killed the session');
  report('before-restart',{scenarios:['new dialog','close dialog','new native session','left split','move top','tear-out','merge button','native bottom drop','layout save','cancel close'],errors});
  const sessions=useSessionStore.getState().sessions;
  localStorage.setItem('vibeshell.ui-smoke.restart',JSON.stringify({ids:sessions.map(s=>s.id),originalShell:sessions[0].serverId,testShell:sessions[1].serverId}));
  await pause(400);
  click(document.querySelector('.titlebar-shell button[aria-label=Close]'));
  report('main-close-clicked',true);
}
run().catch(e=>report('smoke-failed',String(e)+' | '+String(e.stack??'')));
`;
    },
  };
}
