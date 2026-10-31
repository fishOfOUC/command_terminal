'use strict';
// 命令工作台的项目上下文；数据保存与项目页共用一个存储层。
let activeProject = null;
async function initializeProject() {
  await ProjectStore.ready;
  const id = new URLSearchParams(location.search).get('project') || '';
  activeProject = ProjectStore.get(id);
  if (!activeProject || activeProject.archived) {
    location.replace('projects.html?missing=1'); return false;
  }
  await ProjectStore.touch(id);
  commands.splice(0,commands.length,...activeProject.commands);
  state.selected = commands[0]?.id || null;
  document.title = `${activeProject.name} · Runbook 命令执行器`;
  $('.workspace-name').textContent = activeProject.name;
  $('.workspace-name').title = activeProject.name;
  $('.terminal-path .mono').textContent = activeProject.path;
  $('.terminal-path .mono').title = activeProject.path;
  $('#workspace-storage').textContent = '本地数据库 · CLI 共享';
  $('#command-editor .dialog-footer .helper').textContent = '保存到当前项目';
  await NativeTerminal.initialize();
  return true;
}
async function saveProjectCommands(command=null) {
  if (!activeProject) return;
  if (!command) return;
  const saved=await ProjectStore.saveCommand(activeProject.id,command,false);
  command.revision=saved.revision;command.updatedAt=saved.updatedAt;
  return saved;
}
async function persistCommand(command,creating=false) {
  return ProjectStore.saveCommand(activeProject.id,command,creating);
}
async function refreshWorkspace() {
  const editingField=document.activeElement.matches('input,textarea,select') && !document.activeElement.closest('#native-terminal');
  if (!Runbook.native || !activeProject || editor.open || editingField || state.saving || state.pendingParameterSaves) return;
  try {
    await ProjectStore.refresh();
    const project=ProjectStore.get(activeProject.id);
    if (!project || project.archived) {location.href='projects.html?missing=1';return;}
    if (JSON.stringify(commands)!==JSON.stringify(project.commands)) {
      commands.splice(0,commands.length,...project.commands);
      if (!commands.some(c=>c.id===state.selected)) state.selected=commands[0]?.id || null;
      renderCommands();
    }
    activeProject=project;
    $('.workspace-name').textContent=project.name;$('.workspace-name').title=project.name;
    // A running session retains its own cwd; this label follows the selected session in native-terminal.js.
    document.title=`${project.name} · Runbook 命令执行器`;
  } catch(error) {$('#global-status').textContent='同步失败';}
}
setInterval(refreshWorkspace,2000);
window.addEventListener('focus',refreshWorkspace);
