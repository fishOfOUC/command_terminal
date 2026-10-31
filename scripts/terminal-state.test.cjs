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
test('execution reuses current session; stop never schedules automatic shell termination',async()=>{
  const nodes=new Map(),calls=[],intervals=[],timeouts=[],storage=new Map();let sessions=[idle()];
  function node(selector) {
    if(!nodes.has(selector))nodes.set(selector,{dataset:{},value:'',hidden:false,handlers:{},setAttribute(){},removeAttribute(){},focus(){},scrollIntoView(){},querySelectorAll(){return [];},addEventListener(name,fn){this.handlers[name]=fn;}});
    return nodes.get(selector);
  }
  class Terminal {constructor(){this.cols=80;this.rows=24;this.buffer={active:{length:0}};}loadAddon(){}open(){}reset(){}write(bytes,done){done();}writeln(){}focus(){}onData(){}onResize(){}clear(){}}
  const api={
    native:true,
    async action(work){return work();},
    async invoke(method,args){
      calls.push([method,args]);
      if(method==='terminal_list')return structuredClone(sessions);
      if(method==='terminal_read')return {session:structuredClone(sessions.find(s=>s.id===args.id)),chunks:[],nextSequence:0};
      if(method==='terminal_execute'){
        const s=sessions.find(s=>s.id===args.request.session);s.ready=false;s.execution={id:'run-'+calls.length,commandId:args.request.command,name:'task',startedAt:Date.now(),finishedAt:null};return structuredClone(s);
      }
      if(method==='terminal_start'){const s={...idle(),id:'two'};sessions.push(s);return s;}
      if(method==='terminal_close')sessions=sessions.filter(s=>s.id!==args.id);
    }
  };
  const context=vm.createContext({console,TerminalSessionState:Session,Terminal,FitAddon:{FitAddon:class {fit(){}}},ResizeObserver:class {observe(){}},window:{Terminal,FitAddon:true},document:{activeElement:{dataset:{}},querySelector:node},$:node,activeProject:{id:'p',path:'E:\\project'},state:{},Runbook:api,renderCommands(){},validateParameter(){return true;},escapeHTML:s=>s,sessionStorage:{getItem:key=>storage.get(key),setItem:(key,val)=>storage.set(key,val),removeItem:key=>storage.delete(key)},setInterval:fn=>intervals.push(fn),setTimeout:fn=>timeouts.push(fn),clearTimeout(){},toast(){},Date,Uint8Array,atob});
  vm.runInContext(fs.readFileSync(require.resolve('../frontend/js/native-terminal.js'),'utf8'),context);
  const native=vm.runInContext('NativeTerminal',context);
  await native.initialize();
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
  await node('#new-session').handlers.click();
  assert.equal(calls.filter(([m])=>m==='terminal_start').length,1);
  assert.equal(calls.find(([m])=>m==='terminal_start')[1].request.command,null);
  for(const id of ['two','one']) {
    node('#session-tabs').handlers.click({target:{closest:selector=>selector==='[data-close-session]' ? {dataset:{closeSession:id}} : null}});
    for(let i=0;i<30;i++)await Promise.resolve();
  }
  assert.equal(sessions.length,0);assert.equal(node('#terminal-empty').hidden,false);
  await native.execute(command);
  assert.equal(calls.filter(([m])=>m==='terminal_start').length,1,'closing last tab or executing with no tab must not create a shell');
});
