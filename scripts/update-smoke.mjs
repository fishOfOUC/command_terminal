// Maintainer smoke test: real WebView2, signed local HTTP fixtures and isolated portable copies.
// Build an older updater-enabled app into .qa/update-old, then current debug binaries into target/debug.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {createPrivateKey,createHash,sign,randomUUID} from 'node:crypto';
import {spawn,spawnSync} from 'node:child_process';
import {cpSync,copyFileSync,existsSync,mkdirSync,mkdtempSync,readFileSync,writeFileSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {sleep} from './native-test-harness.mjs';

mkdirSync('.qa',{recursive:true});
const root=mkdtempSync(resolve('.qa/update-native-'));
const old=resolve(process.env.RUNBOOK_UPDATE_OLD || '.qa/update-old');
const candidate=resolve('target/debug');
const key=createPrivateKey(process.env.RUNBOOK_SIGNING_KEY_PEM || readFileSync(process.env.RUNBOOK_SIGNING_KEY || '.keys/update-private.pem'));
const version=JSON.parse(readFileSync('package.json')).version;
const checks=[];
function pass(message){checks.push(message);console.log('PASS '+message);}
async function eventually(fn,label,timeout=60000){const start=Date.now();let last;while(Date.now()-start<timeout){try{last=await fn();if(last)return last;}catch(e){last=e.message;}await sleep(150);}throw Error(`${label}: ${JSON.stringify(last)}`);}
const valid=join(root,'valid');mkdirSync(valid);
for(const [source,name] of [['runbook-desktop.exe','Runbook-Desktop.exe'],['runbook.exe','runbook.exe'],['runbook-updater.exe','runbook-updater.exe']])copyFileSync(join(candidate,source),join(valid,name));
writeFileSync(join(valid,'README.md'),'Updated Runbook documentation');
const bad=join(root,'bad');cpSync(valid,bad,{recursive:true});writeFileSync(join(bad,'Runbook-Desktop.exe'),'This intentionally cannot start as a Windows executable');
function zip(dir,name){const archive=join(root,name);const result=spawnSync('python',['-c','import zipfile,sys,pathlib; d=pathlib.Path(sys.argv[1]); z=zipfile.ZipFile(sys.argv[2],"w",zipfile.ZIP_DEFLATED); [z.write(p,p.relative_to(d).as_posix()) for p in d.rglob("*") if p.is_file()]; z.close()',dir,archive],{encoding:'utf8',windowsHide:true});assert.equal(result.status,0,result.stderr);return readFileSync(archive);}
const goodZip=zip(valid,'good.zip'),badZip=zip(bad,'bad.zip');
let mode='valid';
const server=createServer((req,res)=>{
  if(req.url==='/package.zip'){const bytes=mode==='bad-binary'?badZip:goodZip;res.setHeader('Content-Length',bytes.length);if(mode==='corrupt'){res.end(Buffer.alloc(bytes.length,1));}else res.end(bytes);return;}
  if(mode==='missing'){res.writeHead(404);res.end();return;}
  const bytes=mode==='bad-binary'?badZip:goodZip;
  const payload=JSON.stringify({schema:1,app:'dev.runbook.desktop',version,platform:'windows-x86_64',data_schema:1,url:`http://127.0.0.1:${server.address().port}/package.zip`,size:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex'),notes:'同步更新桌面和 CLI\n保留全部本地项目与主题'});
  const signature=sign(null,Buffer.from(payload),key).toString('base64');
  res.setHeader('Content-Type','application/json');res.end(JSON.stringify({payload:mode==='tampered'?payload.replace(version,'9.9.9'):payload,signature}));
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));

async function cdp(port) {
  const target=await eventually(async()=>{const r=await fetch(`http://127.0.0.1:${port}/json/list`);return (await r.json()).find(t=>t.type==='page' && /\/(projects|index)\.html/.test(t.url));},'WebView target');
  const ws=new WebSocket(target.webSocketDebuggerUrl);await new Promise((resolve,reject)=>{ws.onopen=resolve;ws.onerror=reject;});
  let id=0;const pending=new Map();
  ws.onmessage=e=>{const m=JSON.parse(e.data);const item=pending.get(m.id);if(item){pending.delete(m.id);clearTimeout(item.timer);m.error?item.reject(Error(m.error.message)):item.resolve(m.result);}};
  const send=(method,params={})=>new Promise((resolve,reject)=>{const request=++id;const timer=setTimeout(()=>{pending.delete(request);reject(Error('CDP timeout '+method));},20000);pending.set(request,{resolve,reject,timer});ws.send(JSON.stringify({id:request,method,params}));});
  const evaluate=async(fn,arg)=>{const expression=typeof fn==='function'?`(${fn.toString()})(${arg===undefined?'':JSON.stringify(arg)})`:fn;const result=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});if(result.exceptionDetails)throw Error(result.exceptionDetails.exception?.description||result.exceptionDetails.text);return result.result?.value;};
  await send('Runtime.enable');await send('Page.enable');
  return {send,evaluate,close(){ws.close();for(const p of pending.values()){clearTimeout(p.timer);p.reject(Error('CDP closed'));}},async screenshot(name){const image=await send('Page.captureScreenshot',{format:'png'});writeFileSync(join(root,name),Buffer.from(image.data,'base64'));}};
}
function cli(app,data,args){const result=spawnSync(join(app,'runbook.exe'),['--data-dir',data,...args],{encoding:'utf8',windowsHide:true});assert.equal(result.status,0,result.stdout+result.stderr);return JSON.parse(result.stdout).data;}
function cleanupApps(app) {
  // Only this test's uniquely named executable path is eligible; validate each process immediately.
  const result=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',
    '$taskExe = $env:RUNBOOK_TEST_EXE; Get-CimInstance Win32_Process -Filter "Name = \'Runbook-Desktop.exe\'" | Where-Object { $_.ExecutablePath -eq $taskExe } | ForEach-Object { $taskPid = $_.ProcessId; $taskCreated = $_.CreationDate; $taskParent = $_.ParentProcessId; $taskOwner = Invoke-CimMethod -InputObject $_ -MethodName GetOwner; $taskFresh = Get-CimInstance Win32_Process -Filter "ProcessId = $taskPid"; if ($taskOwner.User -eq $env:USERNAME -and $taskFresh.ExecutablePath -eq $taskExe -and $taskFresh.CreationDate -eq $taskCreated -and $taskFresh.ParentProcessId -eq $taskParent) { $taskProcess = Get-Process -Id $taskPid; if ($taskProcess.CloseMainWindow()) { if (-not $taskProcess.WaitForExit(10000)) { $taskProcess.Kill() } } else { $taskProcess.Kill() } } }'],
    {env:{...process.env,RUNBOOK_TEST_EXE:join(app,'Runbook-Desktop.exe')},encoding:'utf8',windowsHide:true});
  assert.equal(result.status,0,result.stderr);
}

async function scenario(name,badBinary=false) {
  const app=join(root,name+' portable 空格');cpSync(old,app,{recursive:true});
  const data=join(root,name+' custom 数据');mkdirSync(data);
  const projectDir=join(root,name+' project');mkdirSync(projectDir);
  const project=cli(app,data,['project','add',projectDir]);
  cli(app,data,['command','create','--project',project.id,'--id','keep','--name','保留命令','--base','echo kept']);
  writeFileSync(join(data,'user-marker.txt'),'never overwrite');
  const initial=cli(app,data,['project','get','--id',project.id]);
  const reserve=createServer();await new Promise(r=>reserve.listen(0,'127.0.0.1',r));const port=reserve.address().port;await new Promise(r=>reserve.close(r));
  const env={...process.env,RUNBOOK_DATA_DIR:data,RUNBOOK_UPDATE_ENDPOINT:`http://127.0.0.1:${server.address().port}/latest.json`,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
  let child=spawn(join(app,'Runbook-Desktop.exe'),['--project',project.id],{env,windowsHide:true,stdio:'ignore'});
  let page;
  try {
    page=await cdp(port);const evaluate=(...args)=>page.evaluate(...args);
    await eventually(()=>evaluate(()=>typeof activeProject!=='undefined' && activeProject?.id),'workbench loaded');
    await evaluate(()=>{RunbookTheme.set('light');localStorage.setItem('runbook-update-checked',String(Date.now()));});
    await eventually(()=>evaluate(async()=> (await Runbook.invoke('terminal_list')).some(s=>s.ready)),'terminal ready');
    await evaluate(()=>document.querySelector('#check-updates').click());
    if(!badBinary) {
      mode='missing';await evaluate(()=>Runbook.invoke('update_check'));
      assert.match(await evaluate(()=>Runbook.invoke('update_status').then(s=>s.message)),/尚未提供更新清单/);pass('missing online manifest gives an actionable message');
      mode='tampered';await evaluate(()=>Runbook.invoke('update_check'));
      assert.match(await evaluate(()=>Runbook.invoke('update_status').then(s=>s.message)),/签名验证失败/);pass('tampered signed metadata is rejected in the real app');
      mode='corrupt';await evaluate(()=>Runbook.invoke('update_check'));await evaluate(()=>Runbook.invoke('update_download'));
      assert.match(await evaluate(()=>Runbook.invoke('update_status').then(s=>s.message)),/校验失败/);pass('corrupted download cannot reach installation');
    }
    mode=badBinary?'bad-binary':'valid';await evaluate(()=>Runbook.invoke('update_check'));
    // Reopening reads backend state even after checks deliberately invoked by the test.
    await evaluate(()=>{document.querySelector('#app-update').close();document.querySelector('#check-updates').click();});
    await eventually(()=>evaluate(()=>document.querySelector('#update-action').textContent==='下载更新'&&!document.querySelector('#update-action').disabled),'download action refreshed');
    await evaluate(()=>document.querySelector('#update-action').click());
    await eventually(()=>evaluate(async()=> (await Runbook.invoke('update_status')).phase==='ready'),'download verified');
    await eventually(()=>evaluate(()=>document.querySelector('#update-action').textContent==='重启并更新'),'ready UI');
    if(!badBinary) {
      page.close();page=null;cleanupApps(app);
      child=spawn(join(app,'Runbook-Desktop.exe'),['--project',project.id],{env,windowsHide:true,stdio:'ignore'});
      page=await cdp(port);
      await eventually(()=>evaluate(async()=> (await Runbook.invoke('update_status')).phase==='ready'),'restore downloaded update after restart');
      await evaluate(()=>document.querySelector('#check-updates').click());
      await eventually(()=>evaluate(()=>document.querySelector('#update-action').textContent==='重启并更新'),'restored ready UI');
      pass('completed download survives closing and reopening the desktop');
    }
    await page.screenshot(name+'-ready.png');
    // Check the actual utility at desktop, tablet and mobile widths without changing app design.
    if(!badBinary)for(const [width,height] of [[1440,900],[768,1024],[390,844]]) {
      await page.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
      const rect=await evaluate(()=>{const r=document.querySelector('#app-update').getBoundingClientRect();return {left:r.left,right:r.right,width:innerWidth,scroll:document.querySelector('#app-update').scrollWidth,client:document.querySelector('#app-update').clientWidth};});
      assert(rect.left>=0 && rect.right<=rect.width && rect.scroll<=rect.client+1,JSON.stringify(rect));
      await page.screenshot(`dialog-${width}.png`);
    }
    await evaluate(()=>document.querySelector('#update-action').click());
    await eventually(()=>evaluate(()=>!document.querySelector('#update-confirm').hidden),'session confirmation');
    assert.match(await evaluate(()=>document.querySelector('#update-confirm').textContent),/终端会话/);
    await evaluate(()=>document.querySelector('#update-action').click());
    await eventually(()=>child.exitCode!==null,'old app exits normally');page.close();page=null;
    await eventually(()=>{if(!existsSync(join(app,'.runbook-update/last-result.json')))return false;return readFileSync(join(app,'.runbook-update/last-result.json'),'utf8').includes(badBinary?'恢复旧版':'已成功更新');},'updater result',90000);
    page=await cdp(port);
    await eventually(()=>page.evaluate(()=>typeof activeProject!=='undefined' && activeProject?.id),'restarted selected project');
    assert.equal(await page.evaluate(()=>new URLSearchParams(location.search).get('project')),project.id);
    assert.equal(await page.evaluate(()=>RunbookTheme.current),'light');
    const info=await page.evaluate(()=>Runbook.api({op:'info'}));assert.equal(resolve(info.databasePath),join(data,'runbook.db'));
    const updated=cli(app,data,['project','get','--id',project.id]);
    const content=({openedAt,updatedAt,revision,...fields})=>fields;
    assert.deepEqual(content(updated),content(initial));
    assert.equal(readFileSync(join(data,'user-marker.txt'),'utf8'),'never overwrite');
    const cliVersion=spawnSync(join(app,'runbook.exe'),['--version'],{encoding:'utf8',windowsHide:true});
    assert.match(cliVersion.stdout,new RegExp(badBinary?'0\\.1\\.0':version.replaceAll('.','\\.')));
    for(const [file,source] of [['Runbook-Desktop.exe','runbook-desktop.exe'],['runbook.exe','runbook.exe'],['runbook-updater.exe','runbook-updater.exe']]) {
      const expected=badBinary?join(old,file):join(candidate,source);
      assert.equal(createHash('sha256').update(readFileSync(join(app,file))).digest('hex'),createHash('sha256').update(readFileSync(expected)).digest('hex'));
    }
    await page.screenshot(name+'-restarted.png');
    pass(badBinary?'unstartable new desktop rolls both binaries back and reopens the original project':'real signed update replaces desktop + CLI and reopens the selected project');
    pass(name+': custom data directory, command contents, theme and user files survive');
    if(!badBinary) {
      const snapshot=await page.evaluate(()=>{const root=document.documentElement.cloneNode(true);root.querySelectorAll('script').forEach(s=>s.remove());const base=document.createElement('base');base.href='/frontend/';root.querySelector('head').prepend(base);root.querySelector('#app-update').setAttribute('open','');return '<!doctype html>'+root.outerHTML;});
      writeFileSync(join(root,'dialog-snapshot.html'),snapshot);
    }
  } finally {try{page?.close();}catch{}cleanupApps(app);}
}
async function interrupted() {
  const app=join(root,'interrupted portable 空格');cpSync(valid,app,{recursive:true});
  const data=join(root,'interrupted data');mkdirSync(data);
  const projectDir=join(root,'interrupted project');mkdirSync(projectDir);
  const project=cli(app,data,['project','add',projectDir]);
  const stage=join(app,'.runbook-update','download-interrupted');mkdirSync(join(stage,'backup'),{recursive:true});
  const files=['Runbook-Desktop.exe','runbook.exe','runbook-updater.exe'];
  for(const file of files)copyFileSync(join(app,file),join(stage,'backup',file));
  copyFileSync(join(app,'runbook-updater.exe'),join(stage,'helper.exe'));
  copyFileSync(join(old,'runbook.exe'),join(app,'runbook.exe'));
  writeFileSync(join(data,'marker.txt'),'recovery must preserve this');
  const plan={app,stage,data,project:project.id,token:randomUUID()};
  writeFileSync(join(app,'.runbook-update','transaction.json'),JSON.stringify({plan,phase:'replacing',files:files.map(f=>[f,true])}));
  const reserve=createServer();await new Promise(r=>reserve.listen(0,'127.0.0.1',r));const port=reserve.address().port;await new Promise(r=>reserve.close(r));
  const env={...process.env,RUNBOOK_DATA_DIR:data,RUNBOOK_UPDATE_ENDPOINT:`http://127.0.0.1:${server.address().port}/latest.json`,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
  const child=spawn(join(app,'Runbook-Desktop.exe'),[],{env,windowsHide:true,stdio:'ignore'});
  let page;
  try {
    await eventually(()=>child.exitCode!==null,'interrupted executable exits before recovery');
    await eventually(()=>existsSync(join(app,'.runbook-update/last-result.json')) && readFileSync(join(app,'.runbook-update/last-result.json'),'utf8').includes('已恢复旧版'),'separate recovery helper result');
    page=await cdp(port);
    await eventually(()=>page.evaluate(()=>typeof activeProject!=='undefined' && activeProject?.id),'recovery reopens original project');
    const output=cli(app,data,['info']);assert.equal(output.version,version);
    for(const [file,source] of [['Runbook-Desktop.exe','runbook-desktop.exe'],['runbook.exe','runbook.exe'],['runbook-updater.exe','runbook-updater.exe']])assert.equal(createHash('sha256').update(readFileSync(join(app,file))).digest('hex'),createHash('sha256').update(readFileSync(join(candidate,source))).digest('hex'));
    assert.equal(readFileSync(join(data,'marker.txt'),'utf8'),'recovery must preserve this');
    assert.equal(await page.evaluate(()=>new URLSearchParams(location.search).get('project')),project.id);
    pass('interrupted transaction exits mapped executable, restores all binaries with a separate helper and restarts the project');
    await page.screenshot('interrupted-recovered.png');
  }finally{try{page?.close();}catch{}cleanupApps(app);}
}
try {await scenario('success');await scenario('rollback',true);await interrupted();writeFileSync(join(root,'result.json'),JSON.stringify({checks},null,2));console.log(`Artifacts: ${root}`);}
finally {server.closeAllConnections();await new Promise(r=>server.close(r));}
