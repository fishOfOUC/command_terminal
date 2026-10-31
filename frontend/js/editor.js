'use strict';
// 命令编辑器对话框 — 新增 / 编辑命令与参数定义、校验与保存。

const editor = $('#command-editor');
$('#editor-close').innerHTML = icon('close'); $('#add-parameter').innerHTML = icon('plus')+'添加参数';
function editorValue() { return {base:$('#editor-base').value,params:state.editor?.params || []}; }
function updateEditorPreview() { $('#editor-preview').textContent = assembled(editorValue()) || '填写基础命令后预览'; }
function renderEditorParameters() {
  const params = state.editor.params;
  $('#editor-parameters').innerHTML = params.length ? params.map(p => `<div class="parameter-definition" data-definition="${p.id}"><div class="od-field"><label for="definition-label-${p.id}">参数名称</label><input id="definition-label-${p.id}" data-key="label" value="${escapeHTML(p.label)}" placeholder="端口" maxlength="30" aria-describedby="definition-error-${p.id}"></div><div class="od-field"><label for="definition-flag-${p.id}">命令标记</label><input id="definition-flag-${p.id}" data-key="flag" class="mono" value="${escapeHTML(p.flag)}" placeholder="--port" maxlength="60" aria-describedby="definition-error-${p.id}"></div><div class="od-field"><label for="definition-type-${p.id}">类型</label><select id="definition-type-${p.id}" data-key="type"><option value="text" ${p.type==='text'?'selected':''}>文本</option><option value="number" ${p.type==='number'?'selected':''}>数字</option><option value="boolean" ${p.type==='boolean'?'selected':''}>开关</option></select></div><div class="od-field"><label for="definition-value-${p.id}">默认值</label>${p.type==='boolean' ? `<select id="definition-value-${p.id}" data-key="value"><option value="false" ${!p.value?'selected':''}>关闭</option><option value="true" ${p.value?'selected':''}>开启</option></select>` : `<input id="definition-value-${p.id}" data-key="value" value="${escapeHTML(p.value)}" type="${p.type==='number'?'number':'text'}" placeholder="可选" aria-describedby="definition-error-${p.id}">`}</div><button class="icon-button" type="button" data-remove="${p.id}" aria-label="移除参数${escapeHTML(p.label || '')}">${icon('trash')}</button><p id="definition-error-${p.id}" class="field-error" hidden></p></div>`).join('') : '<p class="parameter-empty">还没有参数。固定命令可以直接保存，也可以添加可配置参数。</p>';
  updateEditorPreview();
}
function setFieldError(id,message) { const node = document.getElementById(id); node.textContent = message; node.hidden = !message; }
function validateEditorField(field) {
  const isName = field.id === 'editor-name';
  const message = !field.value.trim() ? (isName ? '请填写命令名称，例如“启动开发服务”。' : '请填写基础命令，例如 pnpm run dev --。') : '';
  field.setAttribute('aria-invalid',String(Boolean(message))); setFieldError(isName?'name-error':'base-error',message); return !message;
}
function openEditor(command=null) {
  state.origin = command ? {id:command.id,action:'edit'} : null;
  state.editor = command ? JSON.parse(JSON.stringify(command)) : {id:newId(),name:'',base:'',params:[]};
  $('#editor-title').textContent = command ? '编辑命令' : '新增命令';
  $('#editor-delete').hidden=!command;
  $('#editor-save').textContent = command ? '保存修改' : '添加命令';
  $('#editor-name').value = state.editor.name; $('#editor-base').value = state.editor.base;
  for (const id of ['editor-name','editor-base']) document.getElementById(id).removeAttribute('aria-invalid');
  setFieldError('name-error',''); setFieldError('base-error','');
  renderEditorParameters(); editor.showModal(); $('#editor-name').focus();
}
function closeEditor() { editor.close(); }
editor.addEventListener('close',()=> { const origin = state.origin; state.editor = null; if (origin) focusCommand(origin.id,origin.action); else $('#add-command').focus(); });
$('#add-command').addEventListener('click',()=>openEditor());
$('#editor-close').addEventListener('click',closeEditor); $('#editor-cancel').addEventListener('click',closeEditor);
editor.addEventListener('click',event=> { if (event.target === editor) { const rect=editor.getBoundingClientRect(); if(event.clientX<rect.left || event.clientX>rect.right || event.clientY<rect.top || event.clientY>rect.bottom) closeEditor(); } });
$('#editor-name').addEventListener('blur',event=>validateEditorField(event.target));
$('#editor-base').addEventListener('blur',event=>validateEditorField(event.target));
$('#editor-base').addEventListener('input',updateEditorPreview);
$('#add-parameter').addEventListener('click',()=> { const p={id:newId(),label:'',flag:'',type:'text',value:'',required:false}; state.editor.params.push(p); renderEditorParameters(); document.getElementById(`definition-label-${p.id}`).focus(); });
$('#editor-parameters').addEventListener('input',event=> {
  const field=event.target.closest('[data-key]'); if (!field) return;
  const p=state.editor.params.find(p=>p.id===field.closest('[data-definition]').dataset.definition);
  const key=field.dataset.key;
  if (key==='type') {
    p.type=field.value; p.value=p.type==='boolean' ? false : ''; delete p.min; delete p.max;
    renderEditorParameters(); document.getElementById(`definition-type-${p.id}`).focus();
  } else { p[key] = key==='value' && p.type==='boolean' ? field.value==='true' : field.value; updateEditorPreview(); }
});
$('#editor-parameters').addEventListener('click',event=> {
  const remove=event.target.closest('[data-remove]'); if (!remove) return;
  state.editor.params=state.editor.params.filter(p=>p.id!==remove.dataset.remove); renderEditorParameters(); $('#add-parameter').focus();
});
async function saveEditor() {
  if (!state.editor || state.saving) return;
  const nameValid = validateEditorField($('#editor-name'));
  const baseValid = validateEditorField($('#editor-base'));
  let firstInvalid = !nameValid ? $('#editor-name') : !baseValid ? $('#editor-base') : null;
  const flags = new Set();
  state.editor.params.forEach(p=> {
    let error='';
    if (!p.label.trim()) error='请填写参数名称。';
    else if (p.flag.trim() && !/^-{1,2}[a-zA-Z][a-zA-Z0-9_-]*$/.test(p.flag.trim())) error='命令标记请使用 --port 或 -p 格式；位置参数可留空。';
    else if (p.type==='boolean' && !p.flag.trim()) error='开关参数需要命令标记，例如 --open。';
    else if (p.flag.trim() && flags.has(p.flag.trim())) error='命令标记重复，请为每个参数设置不同标记。';
    else if (p.type==='number' && String(p.value).trim() && !Number.isFinite(Number(p.value))) error='数字参数的默认值需为有效数字。';
    else if (p.type==='number' && p.min!==undefined && String(p.value).trim() && (!Number.isInteger(Number(p.value)) || Number(p.value)<p.min || Number(p.value)>p.max)) error=`默认值需为 ${p.min}–${p.max} 之间的整数。`;
    if (p.flag.trim()) flags.add(p.flag.trim());
    setFieldError(`definition-error-${p.id}`,error);
    const node=document.getElementById(`definition-label-${p.id}`); node.setAttribute('aria-invalid',String(Boolean(error)));
    if (error && !firstInvalid) firstInvalid=node;
  });
  if(firstInvalid) { firstInvalid.focus(); return; }
  const draft=state.editor; draft.name=$('#editor-name').value.trim(); draft.base=$('#editor-base').value.trim();
  draft.params.forEach(p=> {p.label=p.label.trim();p.flag=p.flag.trim();});
  const index=commands.findIndex(c=>c.id===draft.id);
  state.saving=true;$('#editor-save').disabled=true;
  let saved;
  try {saved=await persistCommand(draft,index<0);}
  catch(error) {Runbook.report(error);return;}
  finally {state.saving=false;$('#editor-save').disabled=false;}
  if(index>=0) commands[index]=saved; else commands.push(saved);
  await saveProjectCommands();
  state.selected=draft.id; state.search=''; $('#command-search').value=''; renderCommands();
  state.origin={id:draft.id,action:'edit'}; editor.close();
  toast(index>=0?'命令已更新':'命令已添加');
  document.querySelector(`[data-command="${draft.id}"]`)?.scrollIntoView({block:'nearest',behavior:'auto'});
}
$('#editor-save').addEventListener('click',saveEditor);
const commandDelete=$('#command-delete');
$('#editor-delete').addEventListener('click',()=>{
  $('#command-delete-description').textContent=`删除“${state.editor.name}”？此操作不会删除项目文件，也不会停止已启动的进程。`;
  commandDelete.showModal();$('#command-delete-cancel').focus();
});
$('#command-delete-cancel').addEventListener('click',()=>commandDelete.close());
$('#command-delete-confirm').addEventListener('click',async()=>{
  if(state.saving)return;
  state.saving=true;$('#command-delete-confirm').disabled=true;
  try {
    const command=state.editor;
    await ProjectStore.deleteCommand(activeProject.id,command);
    const index=commands.findIndex(c=>c.id===command.id);if(index>=0)commands.splice(index,1);
    state.selected=commands[0]?.id || null;state.origin=null;
    commandDelete.close();editor.close();renderCommands();toast('命令已删除');
  } catch(error){Runbook.report(error);}
  finally{state.saving=false;$('#command-delete-confirm').disabled=false;}
});
editor.addEventListener('keydown',event=> { if(event.key==='Enter' && event.target.tagName==='INPUT') {event.preventDefault();saveEditor();} });
