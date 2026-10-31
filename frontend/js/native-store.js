'use strict';
// 桌面适配器只进行单对象增删改，禁止整库覆盖。CLI 与桌面共用 Rust 服务和版本检查。
function createNativeStore() {
  let cache=[];
  let available=false;
  let info=null;
  const clone=value=>JSON.parse(JSON.stringify(value));
  async function refresh() {
    const result=await Runbook.api({op:'project_list'});
    cache=result.items; available=true;
    // Archived/deleted projects must not leave invisible running sessions behind.
    const active=new Set(cache.filter(p=>!p.archived).map(p=>p.id));
    const sessions=await Runbook.invoke('terminal_list',{project:null});
    const cleanup=await Promise.allSettled(sessions.filter(s=>!active.has(s.projectId)).map(s=>Runbook.invoke('terminal_close',{id:s.id})));
    cleanup.forEach(result=>{if(result.status==='rejected' && result.reason?.code!=='NOT_FOUND')console.warn('Terminal cleanup:',result.reason);});
    return clone(cache);
  }
  const store={
    ready:null,
    read:()=>clone(cache),
    get:id=>clone(cache.find(p=>p.id===id) || null),
    refresh,
    get persistent(){return available;},
    get info(){return info;},
    async create(input) {
      const result=await Runbook.api({op:'project_create',input});
      await refresh(); return result;
    },
    async update(id,patch,revision) {
      const current=cache.find(p=>p.id===id);
      const result=await Runbook.api({op:'project_update',id,patch,ifRevision:revision ?? current?.revision});
      await refresh();return result;
    },
    async remove(id,revision) {
      const current=cache.find(p=>p.id===id);
      await Runbook.api({op:'project_delete',id,ifRevision:revision ?? current?.revision});
      await refresh();
    },
    async saveCommand(project,command,creating=false) {
      const fields={name:command.name,base:command.base,description:command.description || '',params:command.params};
      const request=creating ? {op:'command_create',project,input:{id:command.id,...fields}} : {op:'command_update',project,id:command.id,patch:fields,ifRevision:command.revision};
      const result=await Runbook.api(request);
      await refresh(); return result;
    },
    async deleteCommand(project,command) {
      await Runbook.api({op:'command_delete',project,id:command.id,ifRevision:command.revision});
      await refresh();
    },
    async touch(id) {await Runbook.api({op:'project_touch',id});await refresh();}
  };
  store.ready=(async()=>{info=await Runbook.api({op:'info'});await refresh();return store;})();
  return store;
}
