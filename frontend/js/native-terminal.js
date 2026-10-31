'use strict';
// 会话进程留在 Rust 中；本页仅持有 xterm 渲染器和已读输出游标。
const NativeTerminal = (()=>{
  let terminal,fit,selected=null,sequence=0,polling=false,sessions=[],lastList=0;
  const selectionKey=()=>`runbook.terminal.${activeProject.id}`;
  function options() {
    const select=$('#session-select');
    const old=select.value;
    select.replaceChildren(...sessions.map(info=>{
      const option=document.createElement('option');option.value=info.id;
      option.textContent=`${info.name} · ${info.finished ? `退出 ${info.exitCode ?? '?'}` : '运行中'}`;
      return option;
    }));
    if (sessions.some(s=>s.id===selected?.id)) select.value=selected.id;
    else if (sessions.some(s=>s.id===old)) select.value=old;
  }
  function status(info) {
    selected=info;
    state.running=!info.finished && info.commandId ? {id:info.commandId,name:info.name,started:info.startedAt} : null;
    $('#execution-strip').hidden=false;
    $('#execution-name').textContent=info.name;
    $('#execution-detail').textContent=info.error ? info.error : info.finished ? `${info.stopped ? '已停止' : '已结束'} · 退出码 ${info.exitCode ?? '?'}` : info.commandId ? '正在执行 · 可在终端中输入' : '交互会话 · 可直接输入命令';
    $('#elapsed').textContent=info.finished ? '' : `${((Date.now()-info.startedAt)/1000).toFixed(1)} s`;
    $('#running-indicator').hidden=info.finished;
    $('#stop-command').hidden=info.finished;
    $('#global-status').textContent=info.finished ? `进程已结束 · ${info.exitCode ?? '?'}` : '终端已连接';
    $('.terminal-path .mono').textContent=info.cwd;$('.terminal-path .mono').title=info.cwd;
    $('#close-session').disabled=false;
  }
  async function refreshSessions() {
    sessions=await Runbook.invoke('terminal_list',{project:activeProject.id});
    options();lastList=Date.now();
  }
  async function select(id) {
    const info=sessions.find(s=>s.id===id);if(!info)return;
    selected=info;
    while(polling)await new Promise(resolve=>setTimeout(resolve,10));
    sequence=0;terminal.reset();
    sessionStorage.setItem(selectionKey(),id);
    options();status(info);await poll();
    fit.fit();
    if(!info.finished) await Runbook.invoke('terminal_resize',{id,cols:terminal.cols,rows:terminal.rows});
    terminal.focus();renderCommands();
  }
  async function poll() {
    if (polling || !selected) return;
    polling=true;
    const id=selected.id;
    try {
      const result=await Runbook.invoke('terminal_read',{id,after:sequence});
      if (selected?.id!==id) return;
      if (result.truncated) {terminal.reset();terminal.writeln('[较早输出已从会话缓冲区移除]');}
      for (const chunk of result.chunks) {
        if(selected?.id!==id)return;
        const bytes=Uint8Array.from(atob(chunk),c=>c.charCodeAt(0));
        await new Promise(resolve=>terminal.write(bytes,resolve));
      }
      if(selected?.id!==id)return;
      sequence=result.nextSequence;
      const changed=selected.finished!==result.session.finished;
      status(result.session);
      if(changed){renderCommands();$('#announcer').textContent=`${result.session.name}已结束，退出码 ${result.session.exitCode}`;}
      if(Date.now()-lastList>1000) await refreshSessions();
    } catch(error) {$('#global-status').textContent='终端连接失败';}
    finally {polling=false;}
  }
  async function open(command=null) {
    const values=Object.fromEntries((command?.params || []).map(p=>[p.id,p.value]));
    const info=await Runbook.invoke('terminal_start',{request:{project:activeProject.id,command:command?.id || null,values,ifRevision:command?.revision || null,cols:terminal.cols,rows:terminal.rows}});
    await refreshSessions();await select(info.id);return info;
  }
  return {
    async initialize() {
      if(!window.Terminal || !window.FitAddon) throw new Error('缺少终端组件，请先运行 npm run prepare:frontend。');
      $('.terminal-pane').setAttribute('aria-label','本地真实终端');
      $('#native-terminal').hidden=false;$('#native-session-tools').hidden=false;
      $('.terminal-tab .mono').hidden=true;
      $('.terminal-context>span').textContent='PowerShell · 本地';
      $('.terminal-note>span:first-child').textContent='真实终端 · 在当前用户权限下执行';
      $('#shortcuts .shortcut-row:first-child>span').textContent='执行选中命令（终端外）';
      terminal=new Terminal({fontFamily:'"Cascadia Code", Consolas, monospace',fontSize:13,lineHeight:1.35,cursorBlink:true,scrollback:10000,theme:{background:'#101112',foreground:'#f1f2f3',cursor:'#78d9ba',selectionBackground:'#1d302c',black:'#171719',red:'#ff9ca7',green:'#78d9ba',yellow:'#e3c58a',blue:'#9bbcf2',magenta:'#d9a6ca',cyan:'#98d8dd',white:'#f1f2f3'}});
      fit=new FitAddon.FitAddon();terminal.loadAddon(fit);terminal.open($('#native-terminal'));fit.fit();
      terminal.onData(data=>{if(selected && !selected.finished)Runbook.action(()=>Runbook.invoke('terminal_write',{id:selected.id,data}));});
      terminal.onResize(size=>{if(selected && !selected.finished)Runbook.action(()=>Runbook.invoke('terminal_resize',{id:selected.id,...size}));});
      const observer=new ResizeObserver(()=>{try{fit.fit();}catch{}});observer.observe($('#native-terminal'));
      $('#session-select').addEventListener('change',event=>Runbook.action(()=>select(event.target.value)));
      $('#new-session').addEventListener('click',()=>Runbook.action(()=>open()));
      $('#close-session').addEventListener('click',()=>Runbook.action(async()=>{
        if(!selected)return;await Runbook.invoke('terminal_close',{id:selected.id});selected=null;
        await refreshSessions();if(sessions.length)await select(sessions.at(-1).id);else await open();
      }));
      await refreshSessions();
      const previous=sessionStorage.getItem(selectionKey());
      if(sessions.length)await select(sessions.find(s=>s.id===previous)?.id || sessions.at(-1).id);
      else await open();
      setInterval(poll,150);
    },
    async execute(command) {
      const invalid=command.params.find(p=>!validateParameter(command,p));
      if(invalid){document.getElementById(`param-${command.id}-${invalid.id}`)?.focus();toast('请先修正参数，再执行命令。');return;}
      await Runbook.action(()=>open(command));
    },
    async stop() {
      if(!selected || selected.finished)return;
      const id=selected.id;
      await Runbook.action(async()=>{
        await Runbook.invoke('terminal_stop',{id,force:false});toast('已发送 Ctrl+C；若进程未退出，将结束这个会话。');
        setTimeout(()=>Runbook.action(async()=>{
          const snapshot=await Runbook.invoke('terminal_read',{id,after:0});
          if(!snapshot.session.finished)await Runbook.invoke('terminal_stop',{id,force:true});
        }),1500);
      });
    },
    clear(){terminal?.clear();},
    readText(){
      if(!terminal)return '';
      const buffer=terminal.buffer.active;const lines=[];
      for(let i=0;i<buffer.length;i++)lines.push(buffer.getLine(i)?.translateToString(true) || '');
      return lines.join('\n').trimEnd();
    },
    async copy(){
      if(!terminal)return;
      try{await navigator.clipboard.writeText(this.readText());toast('终端日志已复制');}
      catch{toast('请在终端中选中文本后复制。');}
    }
  };
})();
