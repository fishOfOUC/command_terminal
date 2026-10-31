'use strict';
// Project library. Dynamic text is escaped; unchanged rows retain their DOM identity.
Object.assign(paths, {
  folder:'<path d="M3 7h7l2 2h9v11H3zM3 7V4h7l2 3"/>',
  star:'<path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2-5.6-3-5.6 3 1.1-6.2L3 9.6l6.2-.9z"/>',
  more:'<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>',
  archive:'<rect x="3" y="3" width="18" height="5" rx="1"/><path d="M5 8v13h14V8M10 12h4"/>',
  restore:'<path d="M3 11a9 9 0 1 1 2 7M3 4v7h7M12 7v5l3 2"/>'
});
const viewKey='runbook.project-library',sortNames={recent:'最近打开',name:'项目名称',created:'最近创建'};
let savedView={};
try {savedView=JSON.parse(sessionStorage.getItem(viewKey)) || {};} catch {}
const projectView={filter:['all','starred','archived'].includes(savedView.filter) ? savedView.filter : 'all',search:typeof savedView.search==='string' ? savedView.search : '',sort:sortNames[savedView.sort] ? savedView.sort : 'recent',editing:null,menuId:null,confirmation:null,origin:null,loaded:false,syncing:false};
const projectEditor=$('#project-editor'),projectMenu=$('#project-menu'),projectConfirm=$('#project-confirm'),sortMenu=$('#sort-menu');
const projectURL=id=>`index.html?project=${encodeURIComponent(id)}`;
const rowCache=new Map(),busyProjects=new Set();
let restoreScroll=true;
function rememberView() {try {sessionStorage.setItem(viewKey,JSON.stringify({filter:projectView.filter,search:projectView.search,sort:projectView.sort,scroll:scrollY}));} catch {}}
window.addEventListener('pagehide',rememberView);
function openedLabel(value) {
  if(!value)return '尚未打开';
  const minutes=Math.max(0,Math.floor((Date.now()-value)/60000));
  if(minutes<1)return '刚刚';
  if(minutes<60)return `${minutes} 分钟前`;
  if(minutes<1440)return `${Math.floor(minutes/60)} 小时前`;
  if(minutes<10080)return `${Math.floor(minutes/1440)} 天前`;
  return new Date(value).toLocaleDateString('zh-CN');
}
function htmlWhenChanged(element,html) {
  if(element._html===html)return;
  const href=element.contains(document.activeElement) ? document.activeElement.getAttribute('href') : null;
  element.innerHTML=html;element._html=html;
  if(href)[...element.querySelectorAll('a')].find(el=>el.getAttribute('href')===href)?.focus({preventScroll:true});
}
function storageFeedback(message) {
  $('#storage-caption').textContent=ProjectStore.persistent ? '本地数据库 · 与 CLI 共享' : '存储不可用';
  $('#global-status').textContent=ProjectStore.persistent ? '就绪' : '本地存储不可用';
  if(message)toast(message);
}
function sessionLabel(project) {
  const sessions=ProjectStore.sessions(project.id).filter(s=>!s.finished);
  if(!sessions.length)return '';
  const jobs=sessions.filter(s=>s.commandId).length;
  return `<span class="project-session-state">${jobs ? `${jobs} 条命令运行中` : `${sessions.length} 个终端会话`}</span>`;
}
function rowHTML(p) {
  const star=p.archived ? (p.starred ? `<span class="star-mark" aria-label="已收藏">${icon('star')}</span>` : '') : `<button class="project-star icon-button" type="button" data-star="${p.id}" data-focus-id="star-${p.id}" aria-pressed="${p.starred}" aria-label="${p.starred ? '取消收藏' : '收藏'}${escapeHTML(p.name)}" title="${p.starred ? '取消收藏' : '收藏项目'}">${icon('star')}</button>`;
  return `<td><div class="project-name-line">${p.archived ? `<span class="project-name">${escapeHTML(p.name)}</span>` : `<a class="project-name" data-focus-id="open-${p.id}" href="${projectURL(p.id)}" title="${escapeHTML(p.name)}">${escapeHTML(p.name)}</a>`}${star}</div><p class="project-description">${escapeHTML(p.description || '')}</p>${sessionLabel(p)}</td><td><div class="project-directory" title="${escapeHTML(p.path)}">${icon('folder')}<code>${escapeHTML(p.path)}</code></div></td><td class="command-total">${p.commands.length}</td><td class="last-opened" title="${p.openedAt ? escapeHTML(new Date(p.openedAt).toLocaleString('zh-CN')) : '尚未打开'}">${openedLabel(p.openedAt)}</td><td><div class="project-row-actions">${p.archived ? `<button class="text-button" data-restore="${p.id}" data-focus-id="restore-${p.id}" type="button">恢复</button>` : `<a href="${projectURL(p.id)}" data-focus-id="action-${p.id}" aria-label="打开${escapeHTML(p.name)}">打开</a>`}<button class="icon-button" data-menu="${p.id}" data-focus-id="menu-${p.id}" type="button" aria-haspopup="true" aria-expanded="false" aria-controls="project-menu" aria-label="${escapeHTML(p.name)}的更多操作">${icon('more')}</button></div></td>`;
}
function renderRows(visible) {
  const list=$('#project-list'),focus=document.activeElement.dataset.focusId,ids=new Set(visible.map(p=>p.id));
  for(const [id,{row}] of rowCache)if(!ids.has(id)){row.remove();rowCache.delete(id);}
  let cursor=list.firstElementChild;
  for(const p of visible) {
    let entry=rowCache.get(p.id);
    if(!entry){entry={row:document.createElement('tr')};entry.row.dataset.project=p.id;rowCache.set(p.id,entry);}
    entry.row.classList.toggle('is-archived',p.archived);entry.row.classList.add('project-row');
    const html=rowHTML(p);
    if(entry.html!==html){entry.row.innerHTML=html;entry.html=html;}
    if(entry.row!==cursor)list.insertBefore(entry.row,cursor);
    cursor=entry.row.nextElementSibling;
  }
  if(focus && document.activeElement.dataset.focusId!==focus) {
    const target=[...list.querySelectorAll('[data-focus-id]')].find(el=>el.dataset.focusId===focus) || list.querySelector('a,button') || document.querySelector('[data-filter][aria-pressed=true]');
    target?.focus({preventScroll:true});
  }
}
function renderProjects() {
  const data=ProjectStore.read(),active=data.filter(p=>!p.archived),recent=active.filter(p=>p.openedAt).sort((a,b)=>b.openedAt-a.openedAt);
  $('#all-count').textContent=active.length;$('#starred-count').textContent=active.filter(p=>p.starred).length;$('#archived-count').textContent=data.length-active.length;
  document.querySelectorAll('[data-filter]').forEach(button=>button.setAttribute('aria-pressed',String(button.dataset.filter===projectView.filter)));
  htmlWhenChanged($('#recent-projects'),recent.slice(0,5).map(p=>`<a class="recent-project-link" href="${projectURL(p.id)}" title="${escapeHTML(p.path)}">${icon('folder')}<span>${escapeHTML(p.name)}</span></a>`).join('') || '<p class="helper">打开项目后显示在这里。</p>');
  const current=recent[0];
  $('#continue-project').hidden=!current || projectView.filter==='archived' || Boolean(projectView.search.trim());
  if(current)htmlWhenChanged($('#continue-project'),`<div class="continue-body"><div class="continue-topline"><span class="continue-label">上次打开</span><h2 id="continue-heading">${escapeHTML(current.name)}</h2></div><p class="continue-path mono">${escapeHTML(current.path)}</p></div><a class="secondary-button" href="${projectURL(current.id)}">继续工作 ${icon('chevron')}</a>`);
  const needle=projectView.search.trim().toLocaleLowerCase();
  const visible=data.filter(p=>{
    const inScope=projectView.filter==='archived' ? p.archived : !p.archived && (projectView.filter!=='starred' || p.starred);
    return inScope && `${p.name} ${p.path} ${p.description}`.toLocaleLowerCase().includes(needle);
  }).sort((a,b)=>projectView.sort==='name' ? a.name.localeCompare(b.name,'zh-CN') : (projectView.sort==='created' ? b.createdAt-a.createdAt : (b.openedAt || 0)-(a.openedAt || 0)) || a.name.localeCompare(b.name,'zh-CN'));
  renderRows(visible);$('#projects-empty').hidden=!projectView.loaded || Boolean(visible.length);$('.projects-table').hidden=!visible.length;
  if(!visible.length) {
    $('#empty-title').textContent=needle ? '没有找到匹配的项目' : projectView.filter==='starred' ? '还没有收藏的项目' : projectView.filter==='archived' ? '没有已归档项目' : '从第一个项目开始';
    $('#empty-description').textContent=needle ? '换一个名称或目录试试，或切换到全部项目。' : projectView.filter==='starred' ? '点击项目名称旁的星标，收藏常用项目。' : projectView.filter==='archived' ? '归档项目的命令和参数会保留，随时可以恢复。' : '在项目目录执行 runbook project add，或在这里手动添加。';
    $('#empty-action').textContent=needle ? '清除搜索' : projectView.filter==='all' ? '添加项目' : '查看全部项目';
  }
  const count=visible.reduce((sum,p)=>sum+p.commands.length,0);
  $('#project-summary').textContent=`${visible.length} 个项目 · ${count} 条命令${projectView.filter==='archived' ? ' · 已归档' : ''}`;
  $('#project-result').textContent=`找到 ${visible.length} 个项目，共 ${count} 条命令。`;
  $('#sort-label').textContent=sortNames[projectView.sort];$('#project-sort').setAttribute('aria-label',`排序：${sortNames[projectView.sort]}`);
  sortMenu.querySelectorAll('[data-sort]').forEach(el=>el.setAttribute('aria-pressed',String(el.dataset.sort===projectView.sort)));storageFeedback();
}
async function restoreProject(id) {
  if(busyProjects.has(id))return;busyProjects.add(id);
  try {await ProjectStore.update(id,{archived:false});renderProjects();storageFeedback('项目已恢复');}finally {busyProjects.delete(id);}
}
async function toggleStar(id) {
  if(busyProjects.has(id))return;
  const p=ProjectStore.get(id);if(!p || p.archived)return;busyProjects.add(id);
  try {await ProjectStore.update(id,{starred:!p.starred});renderProjects();storageFeedback(p.starred ? '已取消收藏' : '已收藏项目');}finally {busyProjects.delete(id);}
}
function closeProjectMenu() {if(projectMenu.matches(':popover-open'))projectMenu.hidePopover();}
function openProjectMenu(button) {
  const p=ProjectStore.get(button.dataset.menu);if(!p)return;
  closeProjectMenu();projectView.menuId=p.id;projectView.origin=button;
  projectMenu.innerHTML=`<button type="button" data-project-action="edit">${icon('edit')}编辑项目</button>${p.archived ? `<button type="button" data-project-action="restore">${icon('restore')}恢复项目</button>` : `<button type="button" data-project-action="archive">${icon('archive')}归档项目</button>`}<div class="menu-separator"></div><button type="button" class="danger" data-project-action="delete">${icon('trash')}删除项目</button>`;
  const rect=button.getBoundingClientRect();projectMenu.style.left=`${Math.max(12,Math.min(rect.right-176,innerWidth-188))}px`;projectMenu.showPopover();
  const height=projectMenu.getBoundingClientRect().height;projectMenu.style.top=`${Math.max(12,rect.bottom+height+8>innerHeight ? rect.top-height-6 : rect.bottom+6)}px`;
  button.setAttribute('aria-expanded','true');projectMenu.querySelector('button').focus();
}
projectMenu.addEventListener('toggle',event=>{if(event.newState==='closed')projectView.origin?.setAttribute('aria-expanded','false');});
function arrowMenu(event,buttons) {
  const i=buttons.indexOf(document.activeElement);
  if(['ArrowDown','ArrowUp','Home','End'].includes(event.key)){event.preventDefault();const next=event.key==='Home' ? 0 : event.key==='End' ? buttons.length-1 : (i+(event.key==='ArrowDown' ? 1 : -1)+buttons.length)%buttons.length;buttons[next]?.focus();}
}
projectMenu.addEventListener('keydown',event=>{arrowMenu(event,[...projectMenu.querySelectorAll('button')]);if(event.key==='Escape'){closeProjectMenu();projectView.origin?.focus();}});
function editorValues() {return {name:$('#project-name').value.trim(),path:$('#project-path').value.trim(),description:$('#project-description').value.trim(),starred:$('#project-starred').checked};}
function setEditorValues(p) {for(const key of ['name','path','description'])$(`#project-${key}`).value=p[key] || '';$('#project-starred').checked=Boolean(p.starred);}
function formFeedback(message='',action=null,label='') {
  $('#project-form-feedback').hidden=!message;$('#project-form-message').textContent=message;projectView.recovery=action;
  $('#project-form-recover').hidden=!action;$('#project-form-recover').textContent=label;
}
function showProjectEditor(project=null) {
  projectView.editing=project?.id || null;projectView.editingRevision=project?.revision;
  projectView.editingOriginal=project ? {name:project.name,path:project.path,description:project.description,starred:project.starred} : null;
  $('#project-editor-title').textContent=project ? '编辑项目' : '添加项目';$('#project-save').textContent=project ? '保存修改' : '添加项目';
  $('#project-editor .dialog-footer .helper').textContent=project ? '保留现有命令与参数' : '新项目从空命令库开始';$('.cli-add-hint').hidden=Boolean(project);
  setEditorValues(project || {});formFeedback();
  for(const name of ['name','path']){$(`#project-${name}`).removeAttribute('aria-invalid');$(`#project-${name}-error`).hidden=true;}
  projectEditor.showModal();$('#project-name').focus();
}
const normalizeDirectory=path=>{const clean=path.trim().replaceAll('\\','/').replace(/\/+$/,'');return /^[a-z]:/i.test(clean) || path.startsWith('\\\\') ? clean.toLocaleLowerCase() : clean;};
function fieldError(key,error) {$(`#project-${key}`).setAttribute('aria-invalid',String(Boolean(error)));$(`#project-${key}-error`).textContent=error;$(`#project-${key}-error`).hidden=!error;}
function duplicateFeedback(p) {
  fieldError('path',`这个目录已添加为“${p.name}”${p.archived ? '（已归档）' : ''}。`);
  formFeedback(p.archived ? '恢复已有项目即可继续使用原来的命令。' : '可以直接打开已有项目。',async()=>{if(p.archived)await restoreProject(p.id);rememberView();location.href=projectURL(p.id);},p.archived ? '恢复并打开已有项目' : '打开已有项目');
}
function validateProject() {
  const {name,path}=editorValues();
  const nameError=!name ? '请填写项目名称。' : [...name].length>60 ? '项目名称不能超过 60 个字符。' : '';
  const pathError=!path ? '请填写工作目录。' : !/^(?:[a-zA-Z]:[\\/]|\/|~(?:[\\/]|$)|\\\\[^\\]+\\)/.test(path) ? '请填写完整目录，例如 E:\\Projects\\app 或 ~/projects/app。' : '';
  fieldError('name',nameError);fieldError('path',pathError);
  if(nameError || pathError){$(nameError ? '#project-name' : '#project-path').focus();return false;}
  const duplicate=ProjectStore.read().find(p=>p.id!==projectView.editing && normalizeDirectory(p.path)===normalizeDirectory(path));
  if(duplicate){duplicateFeedback(duplicate);$('#project-path').focus();return false;}return true;
}
for(const name of ['name','path'])$(`#project-${name}`).addEventListener('input',()=>{fieldError(name,'');if(name==='path' && !projectView.editing)formFeedback();});
async function recoverEditor() {
  const id=projectView.editing,draft=editorValues(),base=projectView.editingOriginal;
  await ProjectStore.refresh();const latest=ProjectStore.get(id);if(!projectEditor.open || projectView.editing!==id)return;
  if(!latest){formFeedback('该项目已被删除。当前填写内容仍保留，可复制后重新添加。');return;}
  const changed=Object.keys(draft).filter(key=>draft[key]!==base[key]),conflicts=changed.filter(key=>latest[key]!==base[key] && latest[key]!==draft[key]);
  const merged={...latest};for(const key of changed)merged[key]=draft[key];
  projectView.editingRevision=latest.revision;projectView.editingOriginal={name:latest.name,path:latest.path,description:latest.description,starred:latest.starred};setEditorValues(merged);
  const names={name:'名称',path:'目录',description:'描述',starred:'收藏'};
  const detail=conflicts.map(key=>`${names[key]}的最新值：${key==='starred' ? (latest[key] ? '已收藏' : '未收藏') : latest[key] || '（空）'}`).join('；');
  formFeedback(`已载入最新版本，保留了你修改的字段。${detail ? `${detail}。` : ''}请核对后再次保存。`,conflicts.length ? ()=>{setEditorValues(latest);formFeedback('已使用最新版本，可继续编辑。');} : null,'使用最新版本的全部字段');
}
$('#project-form-recover').addEventListener('click',()=>Runbook.action(async()=>{
  if($('#project-form-recover').disabled || $('#project-save').disabled)return;$('#project-form-recover').disabled=true;$('#project-save').disabled=true;
  try {await projectView.recovery?.();}finally {$('#project-form-recover').disabled=false;$('#project-save').disabled=false;}
}));
function showCreated(project) {projectView.createdId=project.id;$('#project-notice').hidden=false;$('#project-notice-text').textContent=`已添加“${project.name}”`;$('#project-notice-open').href=projectURL(project.id);}
function locateProject(id) {
  const p=ProjectStore.get(id);if(!p)return;projectView.search='';$('#project-search').value='';projectView.filter=p.archived ? 'archived' : 'all';renderProjects();rememberView();
  const row=rowCache.get(id)?.row;if(!row)return;row.scrollIntoView({block:'center',behavior:'instant'});row.classList.add('is-highlighted');row.querySelector('a,button')?.focus({preventScroll:true});setTimeout(()=>row.classList.remove('is-highlighted'),2400);
}
$('#project-notice-locate').addEventListener('click',()=>locateProject(projectView.createdId));$('#project-notice-close').addEventListener('click',()=>{$('#project-notice').hidden=true;});
$('#project-form').addEventListener('submit',async event=>{
  event.preventDefault();if($('#project-save').disabled || !validateProject())return;$('#project-save').disabled=true;
  const editing=Boolean(projectView.editing);
  try {
    const changes=editorValues();let created;
    if(editing){const patch=Object.fromEntries(Object.entries(changes).filter(([key,value])=>value!==projectView.editingOriginal[key]));if(Object.keys(patch).length)await ProjectStore.update(projectView.editing,patch,projectView.editingRevision);}
    else created=await ProjectStore.create(changes);
    projectEditor.close();renderProjects();rememberView();if(created)showCreated(created);storageFeedback(editing ? '项目已更新' : '项目已添加');
  } catch(error) {
    if(error.code==='REVISION_CONFLICT')formFeedback('其他窗口或 CLI 已修改此项目。当前填写内容仍保留，请载入最新版本并核对。',recoverEditor,'载入最新版本，保留我的修改');
    else if(error.code==='ALREADY_EXISTS' && error.details?.projectId){try{await ProjectStore.refresh();const p=ProjectStore.get(error.details.projectId);if(p)duplicateFeedback(p);else formFeedback(Runbook.message(error));}catch(refreshError){formFeedback(Runbook.message(refreshError));}}
    else if(error.details?.field==='path' || error.details?.field==='name'){const key=error.details.field;fieldError(key,Runbook.message(error));$(`#project-${key}`).focus();}
    else formFeedback(Runbook.message(error));
  } finally {$('#project-save').disabled=false;}
});
async function refreshConfirmation() {
  const confirmation=projectView.confirmation;if(!confirmation)return;
  $('#confirm-action').disabled=true;$('#confirm-feedback').hidden=true;$('#confirm-session-summary').textContent='正在检查终端会话…';
  try {
    await ProjectStore.refresh();if(projectView.confirmation!==confirmation)return;
    const p=ProjectStore.get(confirmation.id);if(!p){projectConfirm.close();renderProjects();toast('项目已被删除。');return;}
    confirmation.revision=p.revision;const removing=confirmation.action==='delete';
    $('#confirm-description').textContent=removing ? `将删除“${p.name}”及其 ${p.commands.length} 条命令，无法撤销。磁盘上的目录和文件不受影响。` : `将“${p.name}”移入已归档。它的 ${p.commands.length} 条命令和参数会保留，恢复后可继续使用。`;
    const sessions=ProjectStore.sessions(p.id),running=sessions.filter(s=>!s.finished);
    $('#confirm-session-summary').textContent=sessions.length ? `此操作将关闭 ${sessions.length} 个终端会话，其中 ${running.length} 个仍在运行。` : '当前没有终端会话。';
    $('#confirm-sessions').innerHTML=sessions.map(s=>`<li><span>${escapeHTML(s.name)}</span><span>${s.finished ? '已结束' : s.commandId ? '命令运行中' : '交互终端'}</span></li>`).join('');$('#confirm-action').disabled=false;
  } catch(error){$('#confirm-message').textContent=Runbook.message(error);$('#confirm-feedback').hidden=false;$('#confirm-session-summary').textContent='暂时无法检查会话，请重试。';}
}
function confirmProjectAction(project,action) {
  projectView.confirmation={id:project.id,action,revision:project.revision};$('#confirm-title').textContent=action==='delete' ? '删除项目' : '归档项目';$('#confirm-action').textContent=action==='delete' ? '删除项目' : '归档项目';$('#confirm-action').classList.toggle('danger-button',action==='delete');
  $('#confirm-description').textContent='';$('#confirm-sessions').replaceChildren();projectConfirm.showModal();$('#confirm-cancel').focus();refreshConfirmation();
}
$('#confirm-reload').addEventListener('click',refreshConfirmation);
$('#confirm-action').addEventListener('click',async()=>{
  if($('#confirm-action').disabled)return;$('#confirm-action').disabled=true;projectView.confirmSaving=true;
  try {const {id,action,revision}=projectView.confirmation;if(action==='delete')await ProjectStore.remove(id,revision);else await ProjectStore.update(id,{archived:true},revision);projectConfirm.close();renderProjects();storageFeedback(action==='delete' ? '项目已删除' : '项目已归档，可在已归档中恢复');}
  catch(error){$('#confirm-message').textContent=Runbook.message(error);$('#confirm-feedback').hidden=false;}
  finally {projectView.confirmSaving=false;}
});
projectMenu.addEventListener('click',event=>{
  const action=event.target.closest('[data-project-action]')?.dataset.projectAction;if(!action)return;const p=ProjectStore.get(projectView.menuId);closeProjectMenu();if(!p)return;
  if(action==='edit')showProjectEditor(p);if(action==='restore')Runbook.action(()=>restoreProject(p.id));if(action==='delete' || action==='archive')confirmProjectAction(p,action);
});
$('#project-list').addEventListener('click',event=>{
  const menu=event.target.closest('[data-menu]');if(menu){openProjectMenu(menu);return;}
  const star=event.target.closest('[data-star]');if(star){Runbook.action(()=>toggleStar(star.dataset.star));return;}
  const restore=event.target.closest('[data-restore]');if(restore){Runbook.action(()=>restoreProject(restore.dataset.restore));return;}
  if(event.target.closest('a,button,input') || window.getSelection()?.toString())return;
  const row=event.target.closest('[data-project]');if(row && !row.classList.contains('is-archived')){rememberView();location.href=projectURL(row.dataset.project);}
});
document.querySelectorAll('[data-filter]').forEach(button=>button.addEventListener('click',()=>{closeProjectMenu();projectView.filter=button.dataset.filter;renderProjects();rememberView();}));
$('#project-search').value=projectView.search;$('#project-search').addEventListener('input',event=>{projectView.search=event.target.value;renderProjects();rememberView();});
sortMenu.addEventListener('toggle',event=>{
  $('#project-sort').setAttribute('aria-expanded',String(event.newState==='open'));
  if(event.newState==='open'){const rect=$('#project-sort').getBoundingClientRect();sortMenu.style.left=`${Math.max(12,rect.right-180)}px`;sortMenu.style.top=`${rect.bottom+6}px`;sortMenu.querySelector('[aria-pressed=true]').focus();}
});
sortMenu.addEventListener('click',event=>{const sort=event.target.closest('[data-sort]')?.dataset.sort;if(!sort)return;projectView.sort=sort;sortMenu.hidePopover();renderProjects();rememberView();$('#project-sort').focus();});
sortMenu.addEventListener('keydown',event=>{arrowMenu(event,[...sortMenu.querySelectorAll('button')]);if(event.key==='Escape')$('#project-sort').focus();});
$('#new-project').addEventListener('click',()=>{projectView.origin=$('#new-project');showProjectEditor();});
$('#empty-action').addEventListener('click',()=>{if(projectView.search.trim()){projectView.search='';$('#project-search').value='';renderProjects();$('#project-search').focus();}else if(projectView.filter!=='all'){projectView.filter='all';renderProjects();}else showProjectEditor();rememberView();});
for(const id of ['project-editor-close','confirm-close'])$(`#${id}`).innerHTML=icon('close');
for(const id of ['project-editor-close','project-editor-cancel'])$(`#${id}`).addEventListener('click',()=>{if(!$('#project-save').disabled)projectEditor.close();});
for(const id of ['confirm-close','confirm-cancel'])$(`#${id}`).addEventListener('click',()=>{if(!projectView.confirmSaving)projectConfirm.close();});
projectEditor.addEventListener('cancel',event=>{if($('#project-save').disabled)event.preventDefault();});
projectConfirm.addEventListener('cancel',event=>{if(projectView.confirmSaving)event.preventDefault();});
for(const dialog of [projectEditor,projectConfirm])dialog.addEventListener('close',()=>{
  const id=projectView.editing || projectView.confirmation?.id,origin=[...document.querySelectorAll('[data-menu]')].find(el=>el.dataset.menu===id) || projectView.origin;
  (origin?.isConnected ? origin : $('#new-project')).focus({preventScroll:true});if(dialog===projectEditor)projectView.editing=null;else projectView.confirmation=null;
});
document.addEventListener('keydown',event=>{if(event.key==='/' && !event.ctrlKey && !event.metaKey && !event.altKey && !['INPUT','TEXTAREA','SELECT'].includes(event.target.tagName) && !document.querySelector('dialog[open],:popover-open')){event.preventDefault();$('#project-search').focus();}});
window.addEventListener('resize',()=>{closeProjectMenu();if(sortMenu.matches(':popover-open'))sortMenu.hidePopover();});
window.addEventListener('scroll',event=>{if(!projectMenu.contains(event.target))closeProjectMenu();if(!sortMenu.contains(event.target) && sortMenu.matches(':popover-open'))sortMenu.hidePopover();},true);
async function syncProjects() {
  if(projectEditor.open || projectConfirm.open || document.querySelector(':popover-open') || projectView.syncing || busyProjects.size || window.getSelection()?.toString())return;
  projectView.syncing=true;
  try {if(!projectView.loaded)await ProjectStore.ready.catch(()=>{});await ProjectStore.refresh();projectView.loaded=true;renderProjects();$('#library-state').hidden=true;$('#retry-projects').hidden=true;if(restoreScroll){restoreScroll=false;requestAnimationFrame(()=>scrollTo(0,Number(savedView.scroll) || 0));}}
  catch(error){$('#library-state').hidden=false;$('#library-state-text').textContent=`项目加载失败：${Runbook.message(error)}`;$('#retry-projects').hidden=false;$('#global-status').textContent='加载失败';}
  finally {projectView.syncing=false;}
}
$('#retry-projects').addEventListener('click',syncProjects);window.addEventListener('pageshow',syncProjects);window.addEventListener('focus',syncProjects);
if(Runbook.native)setInterval(syncProjects,2000);
renderProjects();syncProjects();
if(new URLSearchParams(location.search).has('missing'))toast('该项目不存在或已归档，请选择其他项目。');
