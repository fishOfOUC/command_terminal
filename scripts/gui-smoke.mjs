import assert from 'node:assert/strict';
import {mkdirSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {desktopTest} from './native-test-harness.mjs';

async function verify({startup,evaluate,eventually,screenshot,pass,cli},created) {
  assert.equal(startup.data.created,created);
  const id=startup.data.project.id;
  await eventually(()=>evaluate(id=>typeof activeProject!=='undefined' && activeProject?.id===id && document.querySelector('#native-terminal .xterm'),id),'CLI-selected project workbench');
  assert.equal(await evaluate(()=>new URLSearchParams(location.search).get('project')),id);
  assert.equal(await evaluate(()=>document.querySelector('.workspace-name').textContent),startup.data.project.name);
  await eventually(()=>evaluate(async()=> (await Runbook.invoke('terminal_list',{project:activeProject.id})).some(s=>s.ready)),'project terminal prompt');
  const guiInfo=await evaluate(()=>Runbook.api({op:'info'}));
  const cliInfo=await cli(['info']);
  assert.equal(resolve(guiInfo.databasePath),resolve(cliInfo.data.databasePath));
  await screenshot('gui-project.png');
  pass(`CLI exits after launching GUI, opens selected project, shares database and starts terminal (created=${created})`);
}

await desktopTest(async context=>{
  await verify(context,true);
  assert.equal(context.startup.data.project.name,'当前项目 with spaces');
  assert.equal((await context.cli(['project','list'])).data.total,2);
}, {relativeData:true,gui:async({dir})=>{
  const cwd=join(dir,'当前项目 with spaces');mkdirSync(cwd);
  return {cwd};
}});

let expectedId;
await desktopTest(async context=>{
  await verify(context,false);
  assert.equal(context.startup.data.project.id,expectedId);
  assert.equal(context.startup.data.project.archived,false);
  assert.equal(context.startup.data.project.commands[0].id,'keep');
  assert.equal((await context.cli(['project','list'])).data.total,3);
  context.pass('relative child path chooses closest existing project and restores archived commands');
}, {gui:async({dir,cli})=>{
  const root=join(dir,'root'),nested=join(root,'内层项目'),leaf=join(nested,'src');
  mkdirSync(leaf,{recursive:true});
  assert.equal((await cli(['project','add',root])).code,0);
  const inner=await cli(['project','add',nested]);assert.equal(inner.code,0);expectedId=inner.data.id;
  assert.equal((await cli(['command','create','--project',expectedId,'--id','keep','--name','保留命令','--base','echo keep'])).code,0);
  assert.equal((await cli(['project','update','--id',expectedId,'--archived','true'])).code,0);
  return {cwd:root,args:['./内层项目/src/../src']};
}});

await desktopTest(async context=>{
  await verify(context,true);
  assert.equal(context.startup.data.project.name,'app-other');
  assert.equal((await context.cli(['project','list'])).data.total,3);
  context.pass('relative sibling with shared name prefix creates a separate project');
}, {gui:async({dir,cli})=>{
  const root=join(dir,'app'),other=join(dir,'app-other');mkdirSync(root);mkdirSync(other);
  assert.equal((await cli(['project','add',root])).code,0);
  return {cwd:root,args:['../app-other']};
}});
