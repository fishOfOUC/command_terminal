'use strict';
// 快捷键说明对话框与全局键盘快捷键（/ 搜索、Ctrl+Enter 执行）。

const shortcuts=$('#shortcuts');
$('#shortcuts-close').innerHTML = icon('close');
$('#shortcuts-open').addEventListener('click',()=> {shortcuts.showModal();$('#shortcuts-close').focus();});
$('#shortcuts-close').addEventListener('click',()=>shortcuts.close());
shortcuts.addEventListener('close',()=>$('#shortcuts-open').focus());
document.addEventListener('keydown',event=> {
  if (Runbook.native && event.target.closest('#native-terminal')) return;
  if(editor.open || shortcuts.open) return;
  const editing=['INPUT','TEXTAREA','SELECT'].includes(event.target.tagName) || event.target.isContentEditable;
  if(event.key==='/' && !editing) {event.preventDefault();$('#command-search').focus();}
  if(event.key==='Enter' && (event.ctrlKey || event.metaKey)) {const command=commands.find(c=>c.id===state.selected);if(command){event.preventDefault();execute(command);}else toast('请先选择一条命令。');}
});
