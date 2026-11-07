'use strict';
// Shared by the project list and workbench; update state lives in Rust across navigation.
(() => {
  const host=document.querySelector('.header-actions');
  if(!host)return;
  const button=document.createElement('button');
  button.id='check-updates';button.className='text-button update-entry';button.type='button';
  button.textContent='检查更新';button.setAttribute('aria-haspopup','dialog');host.prepend(button);
  const dialog=document.createElement('dialog');dialog.id='app-update';
  dialog.setAttribute('aria-labelledby','app-update-title');
  dialog.innerHTML='<header class="dialog-header"><div><h2 id="app-update-title">软件更新</h2><p id="update-version"></p></div><button id="update-close" class="icon-button" type="button" aria-label="关闭软件更新">×</button></header><div class="dialog-body"><p id="update-message" role="status" aria-live="polite"></p><progress id="update-progress" aria-label="更新下载进度" hidden></progress><p id="update-transfer" hidden></p><p id="update-notes" hidden></p><p id="update-confirm" hidden></p><p id="update-previous" hidden></p><p class="update-data-note">更新会保留项目、命令、主题和数据目录。安装时将关闭 Runbook 的全部终端会话。</p></div><footer class="dialog-footer"><button id="update-later" class="secondary-button" type="button">关闭</button><button id="update-action" class="primary-button" type="button">检查更新</button></footer>';
  document.body.append(dialog);
  const el=id=>dialog.querySelector('#'+id);
  let status=null,busy=false,confirming=false,polling=false;
  const size=bytes=>`${(bytes/1024/1024).toFixed(1)} MB`;
  function render(next) {
    status=next;
    el('update-version').textContent=`当前版本 ${next.currentVersion}`;
    document.querySelectorAll('.statusbar-right .mono:last-child').forEach(node=>node.textContent=`RUNBOOK v${next.currentVersion}`);
    el('update-message').textContent=next.message;
    const downloading=next.phase==='downloading',ready=next.phase==='ready';
    el('update-progress').hidden=!downloading;
    el('update-progress').max=next.total||1;el('update-progress').value=next.downloaded;
    el('update-transfer').hidden=!downloading;
    el('update-transfer').textContent=`${size(next.downloaded)} / ${size(next.total)}`;
    el('update-notes').hidden=!next.available;
    el('update-notes').textContent=next.available ? `${next.available.version} 更新说明\n${next.available.notes}` : '';
    el('update-previous').hidden=!next.previousResult;
    el('update-previous').textContent=next.previousResult||'';
    el('update-confirm').hidden=!confirming;
    const active=busy||['checking','downloading','installing'].includes(next.phase);
    el('update-action').disabled=active;
    el('update-action').textContent=ready ? (confirming?'结束会话并更新':'重启并更新') : next.available ? '下载更新' : '检查更新';
    el('update-later').textContent=ready||downloading ? '稍后' : '关闭';
    button.textContent=ready?'重启更新':next.available?'有新版本':'检查更新';
    if(active)startPolling();
  }
  async function refresh() {const next=await Runbook.invoke('update_status');render(next);return next;}
  function startPolling() {
    if(polling)return;polling=true;
    const tick=async()=>{
      try{await refresh();}catch{}
      if(busy||['checking','downloading'].includes(status?.phase))setTimeout(tick,350);
      else polling=false;
    };setTimeout(tick,350);
  }
  async function operation(command,args={}) {
    if(busy)return;busy=true;el('update-action').disabled=true;startPolling();
    try{const next=await Runbook.invoke(command,args);if(next)render(next);}
    catch(error){el('update-message').textContent=Runbook.message(error);}
    finally{busy=false;el('update-action').disabled=false;}
  }
  button.addEventListener('click',async()=>{
    confirming=false;el('update-confirm').hidden=true;dialog.showModal();
    if(!Runbook.native){el('update-message').textContent='请在 Runbook 桌面程序中检查更新。';el('update-action').disabled=true;return;}
    try{await refresh();if(status.phase==='idle')await operation('update_check');}
    catch(error){el('update-message').textContent=Runbook.message(error);}
  });
  el('update-close').onclick=el('update-later').onclick=()=>dialog.close();
  el('update-action').addEventListener('click',async()=>{
    if(busy)return;
    if(status?.phase==='ready') {
      if(!confirming) {
        try{
          const sessions=await Runbook.invoke('terminal_list');
          const count=sessions.filter(s=>!s.finished).length;
          confirming=true;el('update-confirm').hidden=false;
          el('update-confirm').textContent=count ? `将关闭 ${count} 个终端会话。请保存正在编辑的文件，并等待任务完成后再更新。` : '应用将退出，更新完成后自动重新打开。';
          el('update-action').textContent='结束会话并更新';
        }catch(error){el('update-message').textContent=Runbook.message(error);}
        return;
      }
      const project=new URLSearchParams(location.search).get('project');
      await operation('update_install',{project});
    }else await operation(status?.available?'update_download':'update_check');
  });
  if(Runbook.native) {
    window.addEventListener('DOMContentLoaded',async()=>{
      try{
        await Runbook.api({op:'info'});
        await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
        await Runbook.invoke('update_ready');
        await refresh();
        // Throttle automatic checks across page navigation and restarts; manual checks always work.
        const last=Number(localStorage.getItem('runbook-update-checked')||0);
        if(Date.now()-last>24*60*60*1000) {
          localStorage.setItem('runbook-update-checked',String(Date.now()));
          setTimeout(()=>operation('update_check'),3000);
        }
      }catch(error){console.error('Update initialization:',error);}
    });
  }
})();
