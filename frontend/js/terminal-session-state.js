'use strict';
// Shared interpretation of persistent and legacy session snapshots; no DOM or IPC.
const TerminalSessionState = {
  running(info) {return Boolean(info && !info.finished && (info.execution ? !info.execution.finishedAt : info.commandId));},
  view(info) {
    if(!info)return {name:'终端',detail:'新建终端后即可输入命令',running:false,stop:false,elapsed:''};
    const run=info.execution;
    const running=this.running(info);
    let detail=info.error || (info.finished ? `终端已退出 · 退出码 ${info.exitCode ?? '?'}` : !info.interactive ? '正在执行 · 可在终端中输入' : info.ready ? '交互终端 · 可继续输入命令' : run && running ? '正在执行 · 可在终端中输入' : '终端忙碌或正在连接');
    if(!info.error && !info.finished && run?.finishedAt && info.ready)detail=`${run.stopped ? '已中断' : '命令已结束'} · 退出码 ${run.exitCode ?? '?'} · 可继续输入`;
    return {name:run?.name || info.name,detail,running,stop:!info.finished && (!info.interactive || !info.ready),elapsed:running ? `${((Date.now()-(run?.startedAt || info.startedAt))/1000).toFixed(1)} s` : ''};
  }
};
if(typeof module!=='undefined')module.exports=TerminalSessionState;
