// Real Tauri/WebView2 test harness. Each run owns a separate database, profile and child process.
import {spawn} from 'node:child_process';
import {createServer} from 'node:net';
import {mkdirSync,mkdtempSync,writeFileSync} from 'node:fs';
import {resolve,join,relative} from 'node:path';
import assert from 'node:assert/strict';
import {stopChildTree} from './stop-child-tree.mjs';

export const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
export async function desktopTest(test,options={}) {
  mkdirSync('.qa',{recursive:true});
  const dir=mkdtempSync(resolve('.qa','project-ux-'));
  const mode=process.argv.includes('--dist') ? 'dist' : process.argv.includes('--debug') ? 'debug' : 'release';
  const applicationDir=options.applicationDir ? resolve(options.applicationDir) : resolve(mode==='dist' ? 'dist' : `target/${mode}`);
  const desktop=join(applicationDir,mode==='dist' || options.applicationDir ? 'Runbook-Desktop.exe' : 'runbook-desktop.exe');
  const executable=join(applicationDir,'runbook.exe');
  const dataDir=options.portable ? join(applicationDir,'data') : join(dir,'data');
  const reserve=createServer();await new Promise(r=>reserve.listen(0,'127.0.0.1',r));
  const port=reserve.address().port;await new Promise(r=>reserve.close(r));
  const env={...process.env,RUNBOOK_DATA_DIR:options.relativeData ? join(dir,'wrong-env-data') : dataDir,WEBVIEW2_USER_DATA_FOLDER:join(dir,'webview'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
  if(options.portable) {delete env.RUNBOOK_DATA_DIR;delete env.WEBVIEW2_USER_DATA_FOLDER;}
  const logs=[],errors=[],checks=[],pending=new Map();let socket,nextId=0;
  async function eventually(fn,message,timeout=15000) {
    const started=Date.now();let last;
    while(Date.now()-started<timeout){try{last=await fn();if(last)return last;}catch(error){last=error.message;}await sleep(100);}
    throw Error(`${message}; last=${JSON.stringify(last)}; ${logs.join('').slice(-1000)}`);
  }
  function send(method,params={}) {
    const id=++nextId;
    return new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>{pending.delete(id);reject(Error(`CDP timeout: ${method}`));},15000);
      pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params}));
    });
  }
  async function evaluate(fn,arg) {
    const expression=typeof fn==='function' ? `(${fn.toString()})(${arg===undefined ? '' : JSON.stringify(arg)})` : fn;
    const result=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});
    if(result.exceptionDetails)throw Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
    return result.result?.value;
  }
  async function cli(args,input,cwd) {
    const selectedDir=options.relativeData ? relative(cwd || process.cwd(),dataDir) || '.' : dataDir;
    const argv=options.portable ? args : ['--data-dir',selectedDir,...args];
    const proc=spawn(executable,argv,{cwd,env,windowsHide:true,stdio:['pipe','pipe','pipe']});
    const stdout=[],stderr=[];proc.stdout.on('data',d=>stdout.push(d));proc.stderr.on('data',d=>stderr.push(d));
    proc.stdin.end(input===undefined ? undefined : JSON.stringify(input));
    const code=await new Promise((resolve,reject)=>{
      let timedOut=false;
      const timer=setTimeout(async()=>{
        timedOut=true;
        try {
          const result=JSON.parse(Buffer.concat(stdout).toString());
          if(args[0]==='gui' && result.data?.pid)await stopChildTree({pid:result.data.pid,exitCode:null,signalCode:null});
          else await stopChildTree(proc);
        } catch {}
        proc.stdout.destroy();proc.stderr.destroy();
        reject(Error(`CLI did not close its output within 20 seconds: ${args.join(' ')}`));
      },20000);
      proc.once('error',error=>{clearTimeout(timer);reject(error);});
      proc.once('close',code=>{clearTimeout(timer);if(!timedOut)resolve(code);});
    });
    assert.equal(Buffer.concat(stderr).toString(),'');return {code,...JSON.parse(Buffer.concat(stdout).toString())};
  }
  let startup,app;
  if(options.gui) {
    const {args=[],cwd}=await options.gui({dir,cli});
    startup=await cli(['gui',...args],undefined,cwd);
    assert.equal(startup.code,0,JSON.stringify(startup));
    app={pid:startup.data.pid,exitCode:null,signalCode:null};
  } else {
    app=spawn(desktop,[],{env,windowsHide:true,stdio:['ignore','pipe','pipe']});
    app.stdout.on('data',d=>logs.push(d.toString()));app.stderr.on('data',d=>logs.push(d.toString()));
    app.on('error',error=>logs.push(error.message));
  }
  async function click(selector){await evaluate(s=>{const el=document.querySelector(s);if(!el)throw Error(`Missing ${s}`);el.click();},selector);}
  async function fill(selector,value){await evaluate(({selector,value})=>{const el=document.querySelector(selector);el.value=value;el.dispatchEvent(new Event('input',{bubbles:true}));},{selector,value});}
  async function screenshot(name){const result=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});writeFileSync(join(dir,name),Buffer.from(result.data,'base64'));}
  async function snapshot(name) {
    const html=await evaluate(()=>{
      const root=document.documentElement.cloneNode(true);root.querySelectorAll('script').forEach(el=>el.remove());
      const base=document.createElement('base');base.href='/frontend/';root.querySelector('head').prepend(base);
      return '<!doctype html>'+root.outerHTML;
    });
    writeFileSync(join(dir,name),html);
  }
  function pass(name){checks.push(name);console.log(`PASS ${name}`);}
  try {
    const target=await eventually(async()=>{const res=await fetch(`http://127.0.0.1:${port}/json/list`);return (await res.json()).find(t=>t.type==='page' && /\/(projects|index)\.html/.test(t.url));},'WebView2 startup',30000);
    socket=new WebSocket(target.webSocketDebuggerUrl);await new Promise((resolve,reject)=>{socket.onopen=resolve;socket.onerror=reject;});
    socket.onmessage=event=>{
      const message=JSON.parse(event.data);
      if(message.id && pending.has(message.id)){const item=pending.get(message.id);pending.delete(message.id);clearTimeout(item.timer);if(message.error)item.reject(Error(message.error.message));else item.resolve(message.result);}
      if(message.method==='Runtime.exceptionThrown')errors.push(message.params.exceptionDetails.exception?.description || message.params.exceptionDetails.text);
    };
    await send('Runtime.enable');await send('Page.enable');
    await test({dir,cli,evaluate,send,eventually,click,fill,screenshot,snapshot,pass,startup,applicationDir,dataDir});
    assert.deepEqual(errors,[]);pass('no uncaught JavaScript errors');
    writeFileSync(join(dir,'result.json'),JSON.stringify({mode,checks,errors},null,2));
    console.log(`${checks.length} native checks passed; artifacts: ${dir}`);
  } finally {
    for(const item of pending.values())clearTimeout(item.timer);
    try{socket?.close();}catch{}
    if(options.portable && app.pid) {
      // A normal window close lets WebView flush localStorage before a relocation test.
      const closer=spawn('powershell.exe',['-NoProfile','-NonInteractive','-Command',
        '$taskApp = Get-Process -Id $env:RUNBOOK_TEST_PID -ErrorAction SilentlyContinue; if ($taskApp) { if ($taskApp.Path -ne $env:RUNBOOK_TEST_EXE) { exit 2 }; if (-not $taskApp.CloseMainWindow()) { exit 3 }; if (-not $taskApp.WaitForExit(10000)) { exit 4 } }'],
        {env:{...process.env,RUNBOOK_TEST_PID:String(app.pid),RUNBOOK_TEST_EXE:desktop},windowsHide:true,stdio:'ignore'});
      await new Promise(resolve=>{const timer=setTimeout(()=>{closer.kill();resolve();},12000);closer.once('close',()=>{clearTimeout(timer);resolve();});});
    }
    try {await stopChildTree(app);} finally {writeFileSync(join(dir,'native.log'),logs.join(''));}
  }
}
