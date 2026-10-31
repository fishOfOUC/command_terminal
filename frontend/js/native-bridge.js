'use strict';
// 唯一的桌面通信入口。
const Runbook = {
  native: Boolean(window.__TAURI_INTERNALS__),
  invoke(command,args={}) {
    if(!this.native) return Promise.reject(new Error('请启动 Runbook 桌面程序；直接打开 HTML 无法连接本地后端。'));
    return window.__TAURI__.core.invoke(command,args);
  },
  api(request) { return this.invoke('api',{request}); },
  message(error) {
    const field=error?.details?.field;
    if(error?.code==='VALIDATION_ERROR' && field==='path') return '工作目录不可用，请检查路径是否正确、目录是否存在，以及是否有访问权限。';
    if(error?.code==='VALIDATION_ERROR' && field==='name') return '项目名称不能为空，且不能超过 60 个字符。';
    if(error?.code==='VALIDATION_ERROR' && field==='description') return '项目描述不能超过 1000 个字符。';
    return ({REVISION_CONFLICT:'数据已被其他窗口或 CLI 修改。请重新载入最新版本后核对修改。',ALREADY_EXISTS:'该目录或 ID 已存在，请使用已有项目。',NOT_FOUND:'项目或命令已不存在，请刷新列表。',IO_ERROR:'无法访问本地数据，请检查目录和访问权限后重试。'})[error?.code] || error?.message || String(error);
  },
  report(error) {
    console.error('Runbook:',error);
    const message = this.message(error);
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
