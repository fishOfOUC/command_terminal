import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {existsSync,mkdirSync,mkdtempSync,readFileSync,realpathSync,renameSync} from 'node:fs';
import {join,resolve,sep} from 'node:path';
import {desktopTest,sleep} from './native-test-harness.mjs';
import {stopChildTree} from './stop-child-tree.mjs';

const root=resolve('.');
mkdirSync(join(root,'.qa'),{recursive:true});
const fixture=mkdtempSync(join(root,'.qa','portable-install-'));
const applicationDir=join(fixture,'downloaded Runbook with spaces');
const version=JSON.parse(readFileSync(join(root,'package.json'),'utf8')).version;
const archive=join(root,'release',`Runbook-${version}-windows-x64.zip`);
const extracted=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',
  'Expand-Archive -LiteralPath $env:RUNBOOK_TEST_ARCHIVE -DestinationPath $env:RUNBOOK_TEST_DEST -Force'],
  {env:{...process.env,RUNBOOK_TEST_ARCHIVE:archive,RUNBOOK_TEST_DEST:applicationDir},windowsHide:true,encoding:'utf8'});
assert.equal(extracted.status,0,extracted.stderr);
assert(!existsSync(join(applicationDir,'data')),'download must not contain a developer database or cache');

function appProcesses(dir) {
  const result=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',
    '@(Get-Process -Name Runbook-Desktop -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $env:RUNBOOK_TEST_EXE } | Select-Object -ExpandProperty Id) | ConvertTo-Json -Compress'],
    {env:{...process.env,RUNBOOK_TEST_EXE:join(dir,'Runbook-Desktop.exe')},windowsHide:true,encoding:'utf8'});
  assert.equal(result.status,0,result.stderr);
  const ids=result.stdout.trim() ? JSON.parse(result.stdout) : [];
  return Array.isArray(ids) ? ids : [ids];
}

const projectDir=join(fixture,'我的真实项目');mkdirSync(projectDir);
let homeId;
await desktopTest(async({cli,evaluate,eventually,click,fill,screenshot,pass,dataDir})=>{
  await eventually(()=>evaluate(()=>typeof projectView!=='undefined' && projectView.loaded),'fresh portable library');
  const items=(await cli(['project','list'])).data.items;
  assert.equal(items.length,1);const home=items[0];homeId=home.id;
  assert.equal(home.name,'Runbook');assert.equal(resolve(home.path),applicationDir);
  assert.deepEqual(home.commands.map(c=>[c.name,c.base]),[['添加项目','runbook project add'],['启动 GUI','runbook gui']]);
  assert.equal(resolve((await cli(['info'])).data.databasePath),join(dataDir,'runbook.db'));
  await eventually(()=>existsSync(join(dataDir,'EBWebView')),'portable WebView profile');
  pass('downloaded archive has no local data; first GUI creates its default project, database and WebView profile beside the app');
  await click(`[data-project="${homeId}"] .project-name`);
  await eventually(()=>evaluate(()=>typeof activeProject!=='undefined' && activeProject?.id && document.querySelector('#native-terminal .xterm')),'default workbench');
  await eventually(()=>evaluate(async()=> (await Runbook.invoke('terminal_list',{project:activeProject.id})).some(s=>s.ready)),'default terminal');
  await fill('#param-add-project-path',projectDir);
  await click('[data-command="add-project"] [data-action="run"]');
  await eventually(async()=> (await cli(['project','list'])).data.items.some(p=>resolve(p.path)===projectDir),'built-in add project command');
  await eventually(()=>evaluate(async()=> (await Runbook.invoke('terminal_list',{project:activeProject.id})).some(s=>s.ready && s.execution?.commandId==='add-project' && s.execution.exitCode===0)),'add finishes successfully');
  pass('human add command works in the downloaded app without global CLI registration and uses the same database');
  await evaluate(()=>RunbookTheme.set('light'));
  await screenshot('portable-default-project.png');

  const original=appProcesses(applicationDir);assert.equal(original.length,1);
  try {
    await click('[data-command="open-gui"] [data-action="select"]');
    await fill('#param-open-gui-path',projectDir);
    await click('[data-command="open-gui"] [data-action="run"]');
    const ids=await eventually(()=>{const ids=appProcesses(applicationDir);return ids.length===2 && ids;},'built-in GUI launch');
    const child=ids.find(id=>!original.includes(id));assert(child);
    await eventually(()=>evaluate(async()=> (await Runbook.invoke('terminal_list',{project:activeProject.id})).some(s=>s.ready && s.execution?.commandId==='open-gui' && s.execution.exitCode===0)),'GUI command returns');
    await evaluate(async()=>{
      for(const session of await Runbook.invoke('terminal_list',{project:activeProject.id}))await Runbook.invoke('terminal_close',{id:session.id});
    });
    assert(appProcesses(applicationDir).includes(child),'new GUI must survive closing its launching terminal');
    pass('human GUI command opens another window and that window survives closing the original terminal');
  } finally {
    for(const pid of appProcesses(applicationDir).filter(id=>!original.includes(id))) {
      await stopChildTree({pid,exitCode:null,signalCode:null});
    }
  }
  assert.equal((await cli(['command','update','--project',homeId,'--id','open-gui','--description','保留我的说明'])).code,0);
},{applicationDir,portable:true});

// Move only this test-owned, closed installation, including its database and WebView profile.
const moved=join(fixture,'moved Runbook');
const boundary=realpathSync(fixture)+sep;
assert(realpathSync(applicationDir).startsWith(boundary));
assert(resolve(moved).startsWith(boundary));
assert(!existsSync(moved));
// Windows can retain an executable/profile handle briefly after taskkill reports success.
for(let attempt=0;;attempt++) {
  try {renameSync(applicationDir,moved);break;}
  catch(error) {
    if(!['EPERM','EBUSY'].includes(error.code) || attempt>=50)throw error;
    await sleep(100);
  }
}
await desktopTest(async({cli,evaluate,eventually,pass,screenshot})=>{
  await eventually(()=>evaluate(()=>typeof projectView!=='undefined' && projectView.loaded),'relocated portable library');
  const items=(await cli(['project','list'])).data.items;
  assert.equal(items.length,2);
  const home=items.find(p=>p.id===homeId);assert(home);
  assert.equal(resolve(home.path),moved);
  assert.equal(home.commands.length,2);
  assert.equal(home.commands.find(c=>c.id==='open-gui').description,'保留我的说明');
  assert(items.some(p=>resolve(p.path)===projectDir));
  assert.equal(await evaluate(()=>document.documentElement.dataset.theme),'light');
  await screenshot('portable-relocated.png');
  pass('moving the app preserves projects, command edits and theme while rebinding the default project directory');
},{applicationDir:moved,portable:true});
console.log(`Portable download and relocation verified: ${fixture}`);
