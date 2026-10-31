'use strict';
// One persistent shell per tab; the xterm view reattaches to the selected session's output.
const NativeTerminal = (()=>{
  let terminal,fit,selected=null,sequence=0,polling=false,sessions=[],lastList=0,opening=false,selectionVersion=0;
  let forceTimer;
  const labels=new Map();let nextLabel=1;
  const selectionKey=()=>`runbook.terminal.${activeProject.id}`;
  function tabName(info) {
    if(!labels.has(info.id)){
      labels.set(info.id,`终端 ${nextLabel++}`);
      try{sessionStorage.setItem(`${selectionKey()}.labels`,JSON.stringify({labels:[...labels],next:nextLabel}));}catch{}
    }
    return labels.get(info.id);
  }
  function tabs() {
    const list=$('#session-tabs');
    const focus=document.activeElement?.dataset?.session || document.activeElement?.dataset?.closeSession;
    const html=sessions.map(info=>{
      const active=info.id===selected?.id,running=TerminalSessionState.running(info);
      const title=`${tabName(info)} · ${info.finished ? '已退出' : running ? info.execution?.name || info.name : info.ready ? '可输入' : '忙碌'}`;
      return `<div class="session-tab${active ? ' is-active' : ''}" role="presentation"><button type="button" role="tab" id="tab-${info.id}" data-session="${info.id}" aria-controls="native-terminal" aria-selected="${active}" tabindex="${active ? 0 : -1}" title="${escapeHTML(title)}"><span class="session-tab-state${running ? ' is-running' : ''}${info.finished ? ' is-finished' : ''}" aria-hidden="true"></span><span>${escapeHTML(tabName(info))}</span></button><button type="button" class="session-close" data-close-session="${info.id}" aria-label="关闭${escapeHTML(tabName(info))}" title="关闭终端并结束会话">×</button></div>`;
    }).join('');
    if(list._html!==html){
      list.innerHTML=html;list._html=html;
      if(focus)[...list.querySelectorAll('button')].find(b=>b.dataset.session===focus || b.dataset.closeSession===focus)?.focus({preventScroll:true});
    }
    if(selected)$('#native-terminal').setAttribute('aria-labelledby',`tab-${selected.id}`);
    else $('#native-terminal').removeAttribute('aria-labelledby');
    $('#terminal-empty').hidden=Boolean(selected);$('#native-terminal').hidden=!selected;
  }
  function status(info) {
    selected=info;
    const view=TerminalSessionState.view(info),run=info?.execution;
    state.running=view.running ? {id:run?.commandId || info.commandId,name:view.name,started:run?.startedAt || info.startedAt} : null;
    $('#execution-strip').hidden=!info;$('#execution-name').textContent=view.name;$('#execution-detail').textContent=view.detail;
    $('#elapsed').textContent=view.elapsed;$('#running-indicator').hidden=!view.running;$('#stop-command').hidden=!view.stop;
    $('#global-status').textContent=!info ? '新建终端即可开始' : info.finished ? '终端已退出' : info.interactive && info.ready ? '终端就绪' : '终端已连接';
    $('.terminal-path .mono').textContent=info?.cwd || activeProject.path;$('.terminal-path .mono').title=info?.cwd || activeProject.path;
    if(!info || info.finished || info.ready){clearTimeout(forceTimer);$('#force-stop-session').hidden=true;}
    tabs();
  }
  async function refreshSessions() {sessions=await Runbook.invoke('terminal_list',{project:activeProject.id});tabs();lastList=Date.now();}
  async function select(id) {
    const info=sessions.find(s=>s.id===id);if(!info)return;
    const version=++selectionVersion;
    while(polling)await new Promise(resolve=>setTimeout(resolve,10));
    if(version!==selectionVersion)return;
    sequence=0;terminal.reset();status(info);
    try{sessionStorage.setItem(selectionKey(),id);}catch{}
    await poll();if(version!==selectionVersion)return;
    fit.fit();if(!info.finished)await Runbook.invoke('terminal_resize',{id,cols:terminal.cols,rows:terminal.rows});
    terminal.focus();renderCommands();
    $('#session-tabs [aria-selected=true]')?.scrollIntoView({block:'nearest',inline:'nearest'});
  }
  async function poll() {
    if(polling || !selected)return;
    polling=true;const id=selected.id;
    try {
      const result=await Runbook.invoke('terminal_read',{id,after:sequence});if(selected?.id!==id)return;
      if(result.truncated){terminal.reset();terminal.writeln('[较早输出已从会话缓冲区移除]');}
      for(const chunk of result.chunks){if(selected?.id!==id)return;const bytes=Uint8Array.from(atob(chunk),c=>c.charCodeAt(0));await new Promise(resolve=>terminal.write(bytes,resolve));}
      if(selected?.id!==id)return;
      sequence=result.nextSequence;
      const before=TerminalSessionState.view(selected),after=TerminalSessionState.view(result.session);
      const changed=before.running!==after.running || selected.execution?.id!==result.session.execution?.id || selected.finished!==result.session.finished;
      const index=sessions.findIndex(s=>s.id===id);if(index>=0)sessions[index]=result.session;
      status(result.session);
      if(changed){renderCommands();$('#announcer').textContent=after.detail;}
      if(Date.now()-lastList>1000)await refreshSessions();
    } catch(error){$('#global-status').textContent='终端连接失败';}
    finally {polling=false;}
  }
  async function open() {
    if(opening)return;opening=true;$('#new-session').disabled=true;
    try {
      const info=await Runbook.invoke('terminal_start',{request:{project:activeProject.id,command:null,values:{},ifRevision:null,cols:terminal.cols,rows:terminal.rows}});
      await refreshSessions();await select(info.id);return info;
    } finally {opening=false;$('#new-session').disabled=false;}
  }
  async function close(id) {
    const index=sessions.findIndex(s=>s.id===id),active=selected?.id===id;
    await Runbook.invoke('terminal_close',{id});
    if(active){++selectionVersion;while(polling)await new Promise(resolve=>setTimeout(resolve,10));selected=null;sequence=0;terminal.reset();}
    await refreshSessions();
    if(active && sessions.length)await select(sessions[Math.min(index,sessions.length-1)].id);
    else if(active){try{sessionStorage.removeItem(selectionKey());}catch{}status(null);renderCommands();$('#new-session').focus();}
  }
  return {
    async initialize() {
      if(!window.Terminal || !window.FitAddon)throw new Error('缺少终端组件，请先运行 npm run prepare:frontend。');
      $('.terminal-pane').setAttribute('aria-label','本地真实终端');$('#native-session-tools').hidden=false;
      try {const saved=JSON.parse(sessionStorage.getItem(`${selectionKey()}.labels`));if(Array.isArray(saved?.labels)){for(const [id,label] of saved.labels)labels.set(id,label);nextLabel=Number(saved.next) || 1;}}catch{}
      $('.terminal-context>span').textContent='PowerShell · 本地';$('.terminal-note>span:first-child').textContent='命令在当前标签页执行 · 执行结束后可继续输入';
      $('#shortcuts .shortcut-row:first-child>span').textContent='在当前终端执行选中命令（终端外）';
      terminal=new Terminal({fontFamily:'"Cascadia Code", Consolas, monospace',fontSize:13,lineHeight:1.35,cursorBlink:true,scrollback:10000,theme:{background:'#101112',foreground:'#f1f2f3',cursor:'#78d9ba',selectionBackground:'#1d302c',black:'#171719',red:'#ff9ca7',green:'#78d9ba',yellow:'#e3c58a',blue:'#9bbcf2',magenta:'#d9a6ca',cyan:'#98d8dd',white:'#f1f2f3'}});
      fit=new FitAddon.FitAddon();terminal.loadAddon(fit);$('#native-terminal').hidden=false;terminal.open($('#native-terminal'));fit.fit();
      terminal.onData(data=>{if(selected && !selected.finished)Runbook.action(()=>Runbook.invoke('terminal_write',{id:selected.id,data}));});
      terminal.onResize(size=>{if(selected && !selected.finished)Runbook.action(()=>Runbook.invoke('terminal_resize',{id:selected.id,...size}));});
      new ResizeObserver(()=>{try{fit.fit();}catch{}}).observe($('#native-terminal'));
      $('#new-session').addEventListener('click',()=>Runbook.action(open));$('#empty-new-session').addEventListener('click',()=>Runbook.action(open));
      $('#session-tabs').addEventListener('wheel',event=>{const list=$('#session-tabs');if(list.scrollWidth>list.clientWidth && Math.abs(event.deltaY)>Math.abs(event.deltaX)){event.preventDefault();list.scrollLeft+=event.deltaY;}},{passive:false});
      $('#session-tabs').addEventListener('click',event=>{const closing=event.target.closest('[data-close-session]');if(closing)Runbook.action(()=>close(closing.dataset.closeSession));else {const tab=event.target.closest('[data-session]');if(tab)Runbook.action(()=>select(tab.dataset.session));}});
      $('#session-tabs').addEventListener('keydown',event=>{
        const button=event.target.closest('[role=tab]');if(!button)return;
        const i=sessions.findIndex(s=>s.id===button.dataset.session);
        if(['ArrowLeft','ArrowRight','Home','End'].includes(event.key)){event.preventDefault();const next=event.key==='Home' ? 0 : event.key==='End' ? sessions.length-1 : (i+(event.key==='ArrowRight' ? 1 : -1)+sessions.length)%sessions.length;Runbook.action(async()=>{await select(sessions[next].id);$('#session-tabs [aria-selected=true]')?.focus();});}
        if(event.key==='Delete'){event.preventDefault();Runbook.action(()=>close(button.dataset.session));}
      });
      $('#force-stop-session').addEventListener('click',()=>Runbook.action(async()=>{if(selected && !selected.finished)await Runbook.invoke('terminal_stop',{id:selected.id,force:true});}));
      await refreshSessions();let previous;try{previous=sessionStorage.getItem(selectionKey());}catch{}
      if(sessions.length)await select(sessions.find(s=>s.id===previous)?.id || sessions.at(-1).id);else await open();
      setInterval(poll,150);
    },
    async execute(command) {
      const invalid=command.params.find(p=>!validateParameter(command,p));
      if(invalid){document.getElementById(`param-${command.id}-${invalid.id}`)?.focus();toast('请先修正参数，再执行命令。');return;}
      if(!selected){toast('请先新建一个终端标签页。');$('#new-session').focus();return;}
      if(selected.finished){toast('当前终端已退出，请新建终端标签页。');return;}
      await Runbook.action(async()=>{
        const values=Object.fromEntries(command.params.map(p=>[p.id,p.value]));
        const info=await Runbook.invoke('terminal_execute',{request:{session:selected.id,project:activeProject.id,command:command.id,values,ifRevision:command.revision}});
        status(info);renderCommands();terminal.focus();
      });
    },
    async stop() {
      if(!selected || selected.finished)return;
      const id=selected.id;
      await Runbook.action(async()=>{
        await Runbook.invoke('terminal_stop',{id,force:false});toast('已发送 Ctrl+C，终端返回提示符后可继续输入。');
        clearTimeout(forceTimer);forceTimer=setTimeout(()=>{if(selected?.id===id && !selected.finished && !selected.ready)$('#force-stop-session').hidden=false;},2000);
      });
    },
    clear(){terminal?.clear();},
    readText(){if(!terminal)return '';const buffer=terminal.buffer.active,lines=[];for(let i=0;i<buffer.length;i++)lines.push(buffer.getLine(i)?.translateToString(true) || '');return lines.join('\n').trimEnd();},
    async copy(){try{await navigator.clipboard.writeText(this.readText());toast('终端日志已复制');}catch{toast('请在终端中选中文本后复制。');}}
  };
})();
