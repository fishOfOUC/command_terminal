'use strict';
// 命令库 — 命令拼接 assembled、列表渲染、参数校验、搜索与列表交互。

function assembled(command) {
  const chunks = [command.base.trim()];
  for (const param of command.params) {
    if (param.type === 'boolean') { if (param.value && param.flag.trim()) chunks.push(param.flag.trim()); }
    else if (String(param.value).length) { if (param.flag.trim()) chunks.push(param.flag.trim()); chunks.push(quote(String(param.value))); }
  }
  return chunks.filter(Boolean).join(' ');
}

function parameterMarkup(command, param) {
  const id = `param-${command.id}-${param.id}`;
  if (param.type === 'boolean') return `<div class="toggle-line"><label for="${id}" class="od-field"><span>${escapeHTML(param.label)}</span><span class="parameter-flag">${escapeHTML(param.flag)}</span></label><label class="switch"><input id="${id}" type="checkbox" data-param="${param.id}" ${param.value ? 'checked' : ''} aria-label="${escapeHTML(param.label)}"><span class="switch-visual" aria-hidden="true"></span></label></div>`;
  return `<div class="parameter-field od-field ${command.params.filter(p => p.type !== 'boolean').length === 1 ? 'full-width' : ''}"><label for="${id}"><span>${escapeHTML(param.label)}</span><span class="parameter-flag">${escapeHTML(param.flag)}</span></label><input id="${id}" data-param="${param.id}" type="${param.type === 'number' ? 'number' : 'text'}" value="${escapeHTML(param.value)}" ${param.min !== undefined ? `min="${param.min}"` : ''} ${param.max !== undefined ? `max="${param.max}"` : ''} ${param.required ? 'required' : ''} spellcheck="false" autocomplete="off" aria-describedby="error-${id}"><p id="error-${id}" class="field-error" hidden></p></div>`;
}
function renderCommands() {
  const needle = state.search.trim().toLocaleLowerCase();
  const visible = commands.filter(command => `${command.name} ${command.base}`.toLocaleLowerCase().includes(needle));
  $('#command-count').textContent = commands.length;
  $('#list-caption').textContent = needle ? `搜索结果 · ${visible.length} 条` : '项目命令';
  $('#command-list').innerHTML = visible.length ? visible.map(command => {
    const selected = command.id === state.selected;
    const running = state.running && state.running.id === command.id;
    return `<article class="command ${selected ? 'selected' : ''}" data-command="${command.id}"><div class="command-summary"><button class="command-toggle" type="button" data-action="select" aria-expanded="${selected}" aria-controls="config-${command.id}">${icon(selected ? 'down' : 'chevron')}<span class="od-field"><span class="command-name">${escapeHTML(command.name)}</span><span class="command-base mono od-truncate">${escapeHTML(command.base)}</span></span></button><button class="icon-button run-button" type="button" data-action="run" aria-label="执行${escapeHTML(command.name)}" title="${'执行' + escapeHTML(command.name)}">${running ? '<span class="running-indicator" aria-hidden="true"></span>' : icon('play')}</button></div><div id="config-${command.id}" class="command-config" ${selected ? '' : 'hidden'}><div class="config-caption"><span>参数配置</span><button type="button" class="text-button" data-action="edit">${icon('edit')}编辑命令</button></div><div class="parameter-grid">${command.params.filter(p => p.type !== 'boolean').map(p => parameterMarkup(command,p)).join('')}</div>${command.params.filter(p => p.type === 'boolean').map(p => parameterMarkup(command,p)).join('')}${command.params.length ? '' : '<p class="no-parameters">这条命令无需配置参数。</p>'}<div class="preview-wrap"><div class="preview-label">执行预览</div><code class="command-preview" data-preview>${escapeHTML(assembled(command))}</code></div></div></article>`;
  }).join('') : commands.length ? '<div class="empty-search"><h3>没有找到匹配的命令</h3><p>尝试其他名称，或添加一条新命令。</p><button type="button" class="secondary-button" data-action="reset-search">清除搜索</button></div>' : '<div class="empty-search"><h3>项目的第一条命令</h3><p>把常用操作留在这里，下次打开项目就能继续。</p><button type="button" class="secondary-button" data-action="create-command">新增命令</button></div>';
}
function validateParameter(command,param) {
  if (param.type === 'boolean') return true;
  const input = document.getElementById(`param-${command.id}-${param.id}`);
  const feedback = document.getElementById(`error-param-${command.id}-${param.id}`);
  const value = String(param.value).trim();
  let error = '';
  if (param.required && !value) error = `请填写${param.label}后再执行。`;
  else if (value && param.type === 'number' && !Number.isFinite(Number(value))) error = '请输入有效数字。';
  else if (value && param.type === 'number' && param.min !== undefined && (!Number.isInteger(Number(value)) || Number(value) < param.min || Number(value) > param.max)) error = `请输入 ${param.min}–${param.max} 之间的整数。`;
  if (input) input.setAttribute('aria-invalid', String(Boolean(error)));
  if (feedback) { feedback.textContent = error; feedback.hidden = !error; }
  return !error;
}
function focusCommand(id,action='select') {
  const item = document.querySelector(`[data-command="${id}"] [data-action="${action}"]`);
  (item || $('#add-command')).focus({preventScroll:true});
}
$('#command-list').addEventListener('click', event => {
  const button = event.target.closest('button[data-action]');
  if (!button) return;
  if (button.dataset.action === 'create-command') { openEditor(); return; }
  if (button.dataset.action === 'reset-search') { state.search = ''; $('#command-search').value = ''; renderCommands(); $('#command-search').focus(); return; }
  const item = button.closest('[data-command]');
  if (!item) return;
  const command = commands.find(c => c.id === item.dataset.command);
  if (!command) return;
  if (button.dataset.action === 'select') { state.selected = state.selected === command.id ? null : command.id; renderCommands(); focusCommand(command.id); }
  if (button.dataset.action === 'run') execute(command);
  if (button.dataset.action === 'edit') openEditor(command);
});
function updateParameter(event) {
  const input = event.target.closest('[data-param]');
  if (!input) return;
  const item = input.closest('[data-command]');
  const command = commands.find(c => c.id === item.dataset.command);
  const param = command.params.find(p => p.id === input.dataset.param);
  param.value = param.type === 'boolean' ? input.checked : input.value;
  item.querySelector('[data-preview]').textContent = assembled(command);
}
$('#command-list').addEventListener('input', updateParameter);
const parameterSaveQueues=new Map();
$('#command-list').addEventListener('change', event => {
  updateParameter(event);
  const item=event.target.closest('[data-command]');
  const command=commands.find(c=>c.id===item?.dataset.command);
  if (command) {
    state.pendingParameterSaves=(state.pendingParameterSaves || 0)+1;
    const work=(parameterSaveQueues.get(command.id) || Promise.resolve()).then(()=>saveProjectCommands(command));
    parameterSaveQueues.set(command.id,work.catch(error=>Runbook.report(error)).finally(()=>{state.pendingParameterSaves--;}));
  }
});
$('#command-list').addEventListener('focusout', event => {
  const input = event.target.closest('[data-param]'); if (!input) return;
  const command = commands.find(c => c.id === input.closest('[data-command]').dataset.command);
  validateParameter(command,command.params.find(p => p.id === input.dataset.param));
});
$('#command-search').addEventListener('input', event => { state.search = event.target.value; renderCommands(); });
