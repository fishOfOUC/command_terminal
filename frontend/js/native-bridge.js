'use strict';
// 唯一的桌面通信入口。
const Runbook = {
  native: Boolean(window.__TAURI_INTERNALS__),
  invoke(command,args={}) {
    if(!this.native) return Promise.reject(new Error('请启动 Runbook 桌面程序；直接打开 HTML 无法连接本地后端。'));
    return window.__TAURI__.core.invoke(command,args);
  },
  api(request) { return this.invoke('api',{request}); },
  report(error) {
    console.error('Runbook:',error);
    const message = error?.code === 'REVISION_CONFLICT' ? '数据已被其他窗口或 CLI 修改。请刷新后再保存；当前编辑内容仍保留。' : error?.message || String(error);
    toast(message);
    return message;
  },
  async action(work) {
    try { return await work(); }
    catch(error) { this.report(error); return null; }
  }
};
if (Runbook.native) {
  document.documentElement.dataset.runtime = 'desktop';
  window.addEventListener('DOMContentLoaded',() => {
    document.querySelectorAll('.statusbar-right .mono:last-child').forEach(el=>el.textContent='RUNBOOK v0.1');
  });
}
