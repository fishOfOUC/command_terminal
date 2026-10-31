'use strict';
// 入口 — 初始渲染命令库。

initializeProject().then(ready=>{if(ready)renderCommands();}).catch(error=>{Runbook.report(error);$('#global-status').textContent='加载失败';});
