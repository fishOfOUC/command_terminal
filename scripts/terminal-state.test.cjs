const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const Session=require('../frontend/js/terminal-session-state.js');
const idle=()=>({id:'one',projectId:'p',name:'PowerShell',cwd:'E:\\project',interactive:true,ready:true,finished:false,startedAt:100,execution:null});
test('completed or interrupted command leaves a live terminal ready for input',()=>{
  for(const stopped of [false,true]) {
    const info={...idle(),execution:{id:'run',commandId:'c',name:'command',startedAt:100,finishedAt:200,exitCode:stopped ? 130 : 0,stopped}};
    assert.equal(Session.running(info),false);assert.equal(Session.view(info).stop,false);assert.match(Session.view(info).detail,/可继续输入/);
    assert.equal(info.finished,false);
  }
});
test('busy manual program can be stopped; idle and exited shells are distinct',()=>{
  assert.equal(Session.view({...idle(),ready:false}).stop,true);
  assert.equal(Session.view(idle()).stop,false);
  assert.match(Session.view({...idle(),finished:true,exitCode:7}).detail,/终端已退出/);
  assert.equal(Session.view({...idle(),finished:true}).stop,false);
});
test('running and failed executions report the current command independently of shell lifetime',()=>{
  const info={...idle(),ready:false,execution:{id:'run',commandId:'c',name:'build',startedAt:Date.now(),finishedAt:null}};
  assert.equal(Session.running(info),true);assert.equal(Session.view(info).name,'build');
  info.ready=true;info.execution.finishedAt=Date.now();info.execution.exitCode=7;
  assert.equal(Session.running(info),false);assert.match(Session.view(info).detail,/退出码 7/);
});
test('session routing, cached long output, scroll position and theme survive tab changes',async()=>{
  const nodes=new Map(),calls=[],intervals=[],timeouts=[],storage=new Map(),instances=[],events=new Map();let sessions=[idle()],activeTheme='dark';
  const history=Buffer.from(Array.from({length:5000},(_,i)=>`schema line ${i}\n`).join(''));
  const chunks=[];for(let i=0;i<history.length;i+=1000)chunks.push(history.subarray(i,i+1000).toString('base64'));
  function element(){return {style:{},dataset:{},value:'',hidden:false,handlers:{},append(){},remove(){},setAttribute(){},removeAttribute(){},focus(){},scrollIntoView(){},querySelectorAll(){return [];},addEventListener(name,fn){this.handlers[name]=fn;}};}
  function node(selector) {
    if(!nodes.has(selector))nodes.set(selector,element());
    return nodes.get(selector);
  }
  class Terminal {
    constructor(options){this.options=options;this.cols=80;this.rows=24;this.writes=0;this.resets=0;this.buffer={active:{length:0,viewportY:0,baseY:0}};instances.push(this);}
    loadAddon(){}open(){}reset(){this.resets++;}write(bytes,done){this.writes++;const b=this.buffer.active;b.baseY+=Buffer.from(bytes).toString().split('\n').length;b.viewportY=b.baseY;done();}writeln(){}focus(){}onData(){}onResize(){}clear(){}dispose(){this.disposed=true;}
    scrollToBottom(){this.buffer.active.viewportY=this.buffer.active.baseY;}scrollToLine(line){this.buffer.active.viewportY=line;}
  }
  const api={
    native:true,
    async action(work){return work();},
    async invoke(method,args){
      calls.push([method,args]);
      if(method==='terminal_list')return structuredClone(sessions);
      if(method==='terminal_read')return {session:structuredClone(sessions.find(s=>s.id===args.id)),chunks:args.id==='one' ? chunks.slice(args.after) : [],nextSequence:args.id==='one' ? chunks.length : 0};
      if(method==='terminal_execute'){
        const s=sessions.find(s=>s.id===args.request.session);s.ready=false;s.execution={id:'run-'+calls.length,commandId:args.request.command,name:'task',startedAt:Date.now(),finishedAt:null};return structuredClone(s);
      }
      if(method==='terminal_start'){const s={...idle(),id:'two'};sessions.push(s);return s;}
      if(method==='terminal_close')sessions=sessions.filter(s=>s.id!==args.id);
    }
  };
  const context=vm.createContext({console,TerminalSessionState:Session,Terminal,RunbookTheme:{terminal:()=>({background:activeTheme})},FitAddon:{FitAddon:class {fit(){}}},ResizeObserver:class {observe(){}},window:{Terminal,FitAddon:true,addEventListener:(name,fn)=>events.set(name,fn)},document:{activeElement:{dataset:{}},querySelector:node,createElement:element},$:node,activeProject:{id:'p',path:'E:\\project'},state:{},Runbook:api,renderCommands(){},validateParameter(){return true;},escapeHTML:s=>s,sessionStorage:{getItem:key=>storage.get(key),setItem:(key,val)=>storage.set(key,val),removeItem:key=>storage.delete(key)},setInterval:fn=>intervals.push(fn),setTimeout:fn=>timeouts.push(fn),clearTimeout(){},toast(){},Date,Uint8Array,atob});
  vm.runInContext(fs.readFileSync(require.resolve('../frontend/js/terminal-views.js'),'utf8'),context);
  vm.runInContext(fs.readFileSync(require.resolve('../frontend/js/native-terminal.js'),'utf8'),context);
  const native=vm.runInContext('NativeTerminal',context);
  await native.initialize();
  assert.equal(instances[0].writes,1,'long history is batched into one write');
  assert.equal(instances[0].options.smoothScrollDuration,0);
  const command={id:'c',name:'task',revision:3,params:[]};
  await native.execute(command);
  assert.equal(calls.filter(([m])=>m==='terminal_start').length,0);
  assert.equal(calls.find(([m])=>m==='terminal_execute')[1].request.session,'one');
  sessions[0].ready=true;sessions[0].execution.finishedAt=Date.now();sessions[0].execution.exitCode=0;
  await intervals[0]();await native.execute(command);
  assert.equal(calls.filter(([m])=>m==='terminal_execute').length,2);
  assert.equal(sessions.length,1);
  await native.stop();timeouts.at(-1)();
  assert.deepEqual(calls.filter(([m])=>m==='terminal_stop').map(([,args])=>args.force),[false]);
  instances[0].scrollToLine(100);
  await node('#new-session').handlers.click();
  assert.equal(calls.filter(([m])=>m==='terminal_start').length,1);
  assert.equal(calls.find(([m])=>m==='terminal_start')[1].request.command,null);
  node('#session-tabs').handlers.click({target:{closest:selector=>selector==='[data-session]' ? {dataset:{session:'one'}} : null}});
  for(let i=0;i<30;i++)await Promise.resolve();
  assert.equal(instances.length,2,'switching tabs reuses existing xterm objects');
  assert.equal(instances[0].buffer.active.viewportY,100,'switching preserves the scrolled viewport');
  assert.equal(instances[0].writes,1,'switching does not replay old history');
  assert.equal(instances[0].resets,0,'switching does not reset the terminal');
  assert.equal(calls.filter(([method,args])=>method==='terminal_read' && args.id==='one').at(-1)[1].after,chunks.length);
  activeTheme='light';events.get('runbook:theme-changed')();
  assert(instances.every(term=>term.options.theme.background==='light'));
  assert.equal(instances[0].buffer.active.viewportY,100,'theme updates preserve the viewport');
  for(const id of ['two','one']) {
    node('#session-tabs').handlers.click({target:{closest:selector=>selector==='[data-close-session]' ? {dataset:{closeSession:id}} : null}});
    for(let i=0;i<30;i++)await Promise.resolve();
  }
  assert.equal(sessions.length,0);assert.equal(node('#terminal-empty').hidden,false);
  await native.execute(command);
  assert.equal(calls.filter(([m])=>m==='terminal_start').length,1,'closing last tab or executing with no tab must not create a shell');
});
