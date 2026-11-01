const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');

test('saved theme, button, native window and terminal palette stay in sync across page loads',async()=>{
  const storage=new Map([['runbook.theme','light']]);
  function page(){
    const root={dataset:{}},events=new Map(),nativeCalls=[];let ready;
    const button={handlers:{},setAttribute(name,value){this[name]=value;},addEventListener(name,fn){this.handlers[name]=fn;}},meta={setAttribute(name,value){this[name]=value;}};
    const palette={light:{'--terminal':'#fcfdfc','--text':'#202b24','--accent':'#20765e'},dark:{'--terminal':'#101112','--text':'#f1f2f3','--accent':'#78d9ba'}};
    const context=vm.createContext({console,Event,localStorage:{getItem:key=>storage.get(key),setItem:(key,value)=>storage.set(key,value)},document:{documentElement:root,querySelector:()=>meta,querySelectorAll:()=>[button],addEventListener:(name,fn)=>{ready=fn;}},getComputedStyle:()=>({getPropertyValue:name=>palette[root.dataset.theme][name]}),window:{__TAURI__:{core:{invoke:(command,args)=>{nativeCalls.push([command,args.theme]);return Promise.resolve();}}},addEventListener:(name,fn)=>events.set(name,fn),dispatchEvent:event=>events.get(event.type)?.(event)}});
    vm.runInContext(fs.readFileSync(require.resolve('../frontend/js/theme.js'),'utf8'),context);
    return {theme:vm.runInContext('RunbookTheme',context),root,button,meta,events,nativeCalls,ready};
  }
  const first=page();assert.equal(first.root.dataset.theme,'light');assert.equal(first.meta.content,'light');
  first.ready();assert.equal(first.theme.terminal().background,'#fcfdfc');
  assert.equal(first.theme.terminal().black,'#202b24');assert.equal(first.theme.terminal().brightBlack,'#59685e');
  first.button.handlers.click();assert.equal(first.root.dataset.theme,'dark');assert.equal(storage.get('runbook.theme'),'dark');
  assert.equal(first.theme.terminal().foreground,'#f1f2f3');assert.equal(first.button['aria-label'],'切换到浅色模式');
  const second=page();second.ready();assert.equal(second.root.dataset.theme,'dark');
  second.theme.set('light');first.events.get('storage')({key:'runbook.theme',newValue:'light'});
  assert.equal(first.root.dataset.theme,'light');assert.equal(second.root.dataset.theme,'light');
  assert(first.nativeCalls.some(([command,theme])=>command==='set_theme' && theme==='light'));
  assert.equal(first.theme.terminal().cursor,'#20765e');
});
