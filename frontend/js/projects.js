'use strict';
// 项目列表、筛选、编辑与生命周期交互。所有 HTML 动态文本均转义。
Object.assign(paths, {
  folder:'<path d="M3 7h7l2 2h9v11H3zM3 7V4h7l2 3"/>',
  star:'<path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2-5.6-3-5.6 3 1.1-6.2L3 9.6l6.2-.9z"/>',
  more:'<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>',
  archive:'<rect x="3" y="3" width="18" height="5" rx="1"/><path d="M5 8v13h14V8M10 12h4"/>',
  restore:'<path d="M3 11a9 9 0 1 1 2 7M3 4v7h7M12 7v5l3 2"/>'
});
const projectView = {filter:'all', search:'', sort:'recent', editing:null, menuId:null, confirmation:null, origin:null};
const projectEditor = $('#project-editor');
const projectMenu = $('#project-menu');
const projectConfirm = $('#project-confirm');
const projectURL = id => `index.html?project=${encodeURIComponent(id)}`;
const recentProjects = data => data.filter(p => !p.archived && p.openedAt).sort((a,b) => b.openedAt-a.openedAt);
function openedLabel(value) {
  if (!value) return '尚未打开';
  const minutes = Math.max(0, Math.floor((Date.now()-value)/60000));
  if (minutes < 1) return '刚刚';
  if (minutes < 60) return `${minutes} 分钟前`;
  if (minutes < 1440) return `${Math.floor(minutes/60)} 小时前`;
  if (minutes < 10080) return `${Math.floor(minutes/1440)} 天前`;
  return new Date(value).toLocaleDateString('zh-CN');
}
function storageFeedback(message) {
  $('#storage-caption').textContent = ProjectStore.persistent ? '本地数据库 · 与 CLI 共享' : '存储不可用';
  $('#global-status').textContent = ProjectStore.persistent ? '就绪' : '本地存储不可用';
  if (message) toast(message);
}
function renderProjects() {
  const data = ProjectStore.read();
  const active = data.filter(p => !p.archived);
  const recent = recentProjects(data);
  $('#all-count').textContent = active.length;
  $('#starred-count').textContent = active.filter(p => p.starred).length;
  $('#archived-count').textContent = data.length-active.length;
  document.querySelectorAll('[data-filter]').forEach(button => button.setAttribute('aria-pressed', String(button.dataset.filter === projectView.filter)));
  $('#recent-projects').innerHTML = recent.slice(0,3).map(p => `<a class="recent-project-link" href="${projectURL(p.id)}">${icon('folder')}<span>${escapeHTML(p.name)}</span></a>`).join('') || '<p class="helper">打开项目后显示在这里。</p>';
  const current = recent[0];
  $('#continue-project').hidden = !current;
  if (current) $('#continue-project').innerHTML = `<div class="continue-body"><div class="continue-topline"><h2 id="continue-heading">${escapeHTML(current.name)}</h2><span class="continue-label">上次的工作</span></div><p class="continue-path mono">${escapeHTML(current.path)}</p><div class="continue-commands">${current.commands.slice(0,3).map(c => `<code>${escapeHTML(c.base)}</code>`).join('') || '<span class="helper">还没有命令，进入工作台添加第一条。</span>'}</div></div><a class="secondary-button" href="${projectURL(current.id)}">继续工作 ${icon('chevron')}</a>`;
  const needle = projectView.search.trim().toLocaleLowerCase();
  const visible = data.filter(p => {
    const inScope = projectView.filter === 'archived' ? p.archived : !p.archived && (projectView.filter !== 'starred' || p.starred);
    return inScope && `${p.name} ${p.path} ${p.description}`.toLocaleLowerCase().includes(needle);
  }).sort((a,b) => projectView.sort === 'name' ? a.name.localeCompare(b.name,'zh-CN') : projectView.sort === 'created' ? b.createdAt-a.createdAt : (b.openedAt || 0)-(a.openedAt || 0));
  $('#project-list').innerHTML = visible.map(p => `<tr data-project="${p.id}"><td><div class="project-name-line">${p.archived ? `<span class="project-name">${escapeHTML(p.name)}</span>` : `<a class="project-name" href="${projectURL(p.id)}" title="${escapeHTML(p.name)}">${escapeHTML(p.name)}</a>`}${p.starred ? `<span class="star-mark" aria-label="已收藏">${icon('star')}</span>` : ''}</div><p class="project-description">${escapeHTML(p.description || '暂无描述')}</p></td><td><div class="project-directory" title="${escapeHTML(p.path)}">${icon('folder')}<code>${escapeHTML(p.path)}</code></div></td><td class="command-total">${p.commands.length}</td><td class="last-opened" data-opened="${Boolean(p.openedAt)}" title="${p.openedAt ? escapeHTML(new Date(p.openedAt).toLocaleString('zh-CN')) : '尚未打开'}">${openedLabel(p.openedAt)}</td><td><div class="project-row-actions">${p.archived ? `<button class="text-button" data-restore="${p.id}" type="button">恢复</button>` : `<a href="${projectURL(p.id)}" aria-label="打开${escapeHTML(p.name)}">打开</a>`}<button class="icon-button" data-menu="${p.id}" type="button" aria-haspopup="true" aria-expanded="false" aria-controls="project-menu" aria-label="${escapeHTML(p.name)}的更多操作">${icon('more')}</button></div></td></tr>`).join('');
  $('#projects-empty').hidden = Boolean(visible.length);
  $('.projects-table').hidden = !visible.length;
  if (!visible.length) {
    $('#empty-title').textContent = needle ? '没有找到匹配的项目' : projectView.filter === 'starred' ? '还没有收藏的项目' : projectView.filter === 'archived' ? '没有已归档项目' : '从第一个项目开始';
    $('#empty-description').textContent = needle ? '换一个名称或目录试试。' : projectView.filter === 'starred' ? '在项目的更多操作中选择收藏，常用项目就会出现在这里。' : projectView.filter === 'archived' ? '暂时不用的项目可以归档，命令和参数都会保留。' : '添加工作目录，为这个项目建立自己的命令库。';
    $('#empty-action').textContent = needle ? '清除搜索' : projectView.filter === 'all' ? '新建项目' : '查看全部项目';
  }
  const count = visible.reduce((sum,p) => sum+p.commands.length,0);
  $('#project-summary').textContent = `${visible.length} 个项目 · ${count} 条命令${projectView.filter === 'archived' ? ' · 已归档' : ''}`;
  $('#project-result').textContent = `找到 ${visible.length} 个项目，共 ${count} 条命令。`;
  storageFeedback();
}
async function restoreProject(id) {
  await ProjectStore.update(id,{archived:false}); renderProjects(); storageFeedback('项目已恢复');
}
function closeProjectMenu() { if (projectMenu.matches(':popover-open')) projectMenu.hidePopover(); }
function openProjectMenu(button) {
  const p = ProjectStore.get(button.dataset.menu);
  if (!p) return;
  closeProjectMenu(); projectView.menuId = p.id; projectView.origin = button;
  projectMenu.innerHTML = `<button type="button" data-project-action="edit">${icon('edit')}编辑项目</button>${p.archived ? `<button type="button" data-project-action="restore">${icon('restore')}恢复项目</button>` : `<button type="button" data-project-action="star">${icon('star')}${p.starred ? '取消收藏' : '收藏项目'}</button><button type="button" data-project-action="archive">${icon('archive')}归档项目</button>`}<button type="button" class="danger" data-project-action="delete">${icon('trash')}删除项目</button>`;
  const rect = button.getBoundingClientRect();
  projectMenu.style.left = `${Math.max(12,Math.min(rect.right-176,innerWidth-188))}px`;
  projectMenu.showPopover();
  const height = projectMenu.getBoundingClientRect().height;
  projectMenu.style.top = `${Math.max(12, rect.bottom+height+8 > innerHeight ? rect.top-height-6 : rect.bottom+6)}px`;
  button.setAttribute('aria-expanded','true');
  projectMenu.querySelector('button').focus();
}
projectMenu.addEventListener('toggle',event => { if (event.newState === 'closed') projectView.origin?.setAttribute('aria-expanded','false'); });
projectMenu.addEventListener('keydown',event => {
  const buttons = [...projectMenu.querySelectorAll('button')];
  const i = buttons.indexOf(document.activeElement);
  if (['ArrowDown','ArrowUp','Home','End'].includes(event.key)) {
    event.preventDefault();
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length-1 : (i+(event.key === 'ArrowDown' ? 1 : -1)+buttons.length)%buttons.length;
    buttons[next].focus();
  }
  if (event.key === 'Escape') { closeProjectMenu(); projectView.origin?.focus(); }
});
function showProjectEditor(project=null) {
  projectView.editing = project?.id || null;
  projectView.editingRevision = project?.revision;
  $('#project-editor-title').textContent = project ? '编辑项目' : '新建项目';
  $('#project-save').textContent = project ? '保存修改' : '创建项目';
  $('#project-editor .dialog-footer .helper').textContent = project ? '保留现有命令与参数' : '新项目从空命令库开始';
  $('#project-name').value = project?.name || '';
  $('#project-path').value = project?.path || '';
  $('#project-description').value = project?.description || '';
  $('#project-starred').checked = project?.starred || false;
  for (const name of ['name','path']) {
    $(`#project-${name}`).removeAttribute('aria-invalid');
    $(`#project-${name}-error`).hidden = true;
  }
  projectEditor.showModal(); $('#project-name').focus();
}
const normalizeDirectory = path => {
  const clean = path.trim().replaceAll('\\','/').replace(/\/+$/,'');
  return /^[a-z]:/i.test(clean) || path.startsWith('\\\\') ? clean.toLocaleLowerCase() : clean;
};
for (const name of ['name','path']) $(`#project-${name}`).addEventListener('input',() => {
  $(`#project-${name}`).removeAttribute('aria-invalid');
  $(`#project-${name}-error`).hidden = true;
});
function validateProject() {
  const name = $('#project-name').value.trim();
  const path = $('#project-path').value.trim();
  const others = ProjectStore.read().filter(p => p.id !== projectView.editing);
  const nameError = !name ? '请填写项目名称。' : '';
  const pathError = !path ? '请填写工作目录。' : !/^(?:[a-zA-Z]:[\\/]|\/|~\/|\\\\[^\\]+\\)/.test(path) ? '请填写完整目录，例如 E:\\Projects\\app、/home/me/app 或 ~/projects/app。' : others.some(p => normalizeDirectory(p.path) === normalizeDirectory(path)) ? '这个目录已经有项目了，请使用其他目录（含已归档项目）。' : '';
  for (const [key,error] of [['name',nameError],['path',pathError]]) {
    $(`#project-${key}`).setAttribute('aria-invalid',String(Boolean(error)));
    $(`#project-${key}-error`).textContent = error;
    $(`#project-${key}-error`).hidden = !error;
  }
  if (nameError || pathError) { $(nameError ? '#project-name' : '#project-path').focus(); return false; }
  return true;
}
$('#project-form').addEventListener('submit',async event => {
  event.preventDefault(); if (!validateProject()) return;
  if ($('#project-save').disabled) return;
  $('#project-save').disabled=true;
  try {
  const changes = {name:$('#project-name').value.trim(),path:$('#project-path').value.trim(),description:$('#project-description').value.trim(),starred:$('#project-starred').checked};
  const editing = Boolean(projectView.editing);
  if (editing) await ProjectStore.update(projectView.editing,changes,projectView.editingRevision);
  else {
    await ProjectStore.create(changes);
    projectView.filter = changes.starred && projectView.filter === 'starred' ? 'starred' : 'all';
    projectView.sort = 'created'; $('#project-sort').value = 'created';
    projectView.search = ''; $('#project-search').value = '';
  }
  projectEditor.close(); renderProjects(); storageFeedback(editing ? '项目已更新' : '项目已创建，打开项目即可添加命令');
  } catch(error) {Runbook.report(error);}
  finally {$('#project-save').disabled=false;}
});
function confirmProjectAction(project,action) {
  projectView.confirmation = {id:project.id,action,revision:project.revision};
  const removing = action === 'delete';
  $('#confirm-title').textContent = removing ? '删除项目' : '归档项目';
  $('#confirm-description').textContent = removing ? `将删除“${project.name}”及其 ${project.commands.length} 条命令，无法撤销。磁盘上的目录和文件不受影响。该项目的终端会话也会关闭。` : `将“${project.name}”移入已归档。它的 ${project.commands.length} 条命令和参数会保留，恢复后可继续使用。该项目的终端会话也会关闭。`;
  $('#confirm-action').textContent = removing ? '删除项目' : '归档项目';
  $('#confirm-action').classList.toggle('danger-button',removing);
  projectConfirm.showModal(); $('#confirm-cancel').focus();
}
$('#confirm-action').addEventListener('click',async () => {
  if ($('#confirm-action').disabled) return;
  $('#confirm-action').disabled=true;
  try {
  const {id,action,revision} = projectView.confirmation;
  if (action === 'delete') await ProjectStore.remove(id,revision);
  else await ProjectStore.update(id,{archived:true},revision);
  projectConfirm.close(); renderProjects(); storageFeedback(action === 'delete' ? '项目已删除' : '项目已归档，可在已归档中恢复');
  } catch(error) {Runbook.report(error);}
  finally {$('#confirm-action').disabled=false;}
});
projectMenu.addEventListener('click',async event => {
  const action = event.target.closest('[data-project-action]')?.dataset.projectAction;
  if (!action) return;
  const p = ProjectStore.get(projectView.menuId); closeProjectMenu(); if (!p) return;
  if (action === 'edit') showProjectEditor(p);
  if (action === 'restore') await Runbook.action(()=>restoreProject(p.id));
  if (action === 'star') await Runbook.action(async()=>{ await ProjectStore.update(p.id,{starred:!p.starred}); renderProjects(); storageFeedback(p.starred ? '已取消收藏' : '已收藏项目'); });
  if (action === 'delete' || action === 'archive') confirmProjectAction(p,action);
});
$('#project-list').addEventListener('click',event => {
  const button = event.target.closest('[data-menu]'); if (button) openProjectMenu(button);
  const restore = event.target.closest('[data-restore]'); if (restore) Runbook.action(()=>restoreProject(restore.dataset.restore));
});
document.querySelectorAll('[data-filter]').forEach(button => button.addEventListener('click',() => { closeProjectMenu(); projectView.filter = button.dataset.filter; renderProjects(); }));
$('#project-search').addEventListener('input',event => { projectView.search = event.target.value; renderProjects(); });
$('#project-sort').addEventListener('change',event => { projectView.sort = event.target.value; renderProjects(); });
$('#new-project').addEventListener('click',() => { projectView.origin = $('#new-project'); showProjectEditor(); });
$('#empty-action').addEventListener('click',() => {
  if (projectView.search.trim()) { projectView.search = ''; $('#project-search').value = ''; renderProjects(); $('#project-search').focus(); }
  else if (projectView.filter !== 'all') { projectView.filter = 'all'; renderProjects(); }
  else showProjectEditor();
});
for (const id of ['project-editor-close','confirm-close']) $(`#${id}`).innerHTML = icon('close');
for (const id of ['project-editor-close','project-editor-cancel']) $(`#${id}`).addEventListener('click',() => projectEditor.close());
for (const id of ['confirm-close','confirm-cancel']) $(`#${id}`).addEventListener('click',() => projectConfirm.close());
for (const dialog of [projectEditor,projectConfirm]) dialog.addEventListener('close',() => {
  const id = projectView.editing || projectView.confirmation?.id;
  const origin = document.querySelector(`[data-menu="${id}"]`) || projectView.origin;
  (origin?.isConnected ? origin : $('#new-project')).focus({preventScroll:true});
  projectView.editing = null; projectView.confirmation = null;
});
document.addEventListener('keydown',event => {
  if (event.key === '/' && !event.ctrlKey && !event.metaKey && !event.altKey && !['INPUT','TEXTAREA','SELECT'].includes(event.target.tagName) && !document.querySelector('dialog[open]')) { event.preventDefault(); $('#project-search').focus(); }
});
window.addEventListener('resize',closeProjectMenu);
window.addEventListener('scroll',closeProjectMenu,true);
async function syncProjects() {
  if (projectEditor.open || projectConfirm.open || projectMenu.matches(':popover-open') || projectView.syncing || $('#project-save').disabled) return;
  projectView.syncing=true;
  try {await ProjectStore.ready;await ProjectStore.refresh();renderProjects();}
  catch(error) {$('#global-status').textContent='加载失败';Runbook.report(error);}
  finally {projectView.syncing=false;}
}
window.addEventListener('pageshow',syncProjects);
window.addEventListener('focus',syncProjects);
if (Runbook.native) {
  setInterval(syncProjects,2000);
  $('#project-path-help').textContent='填写实际存在的完整目录，终端将从这里启动。支持 Windows 路径或 ~/。';
  $('.statusbar-right>span:first-child').textContent='SQLite · 本地存储';
}
syncProjects();
if (new URLSearchParams(location.search).has('missing')) toast('该项目不存在或已归档，请选择其他项目。');
