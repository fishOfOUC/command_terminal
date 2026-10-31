'use strict';
// 工作台动作绑定；PTY 会话由 Rust 后端持有。
function execute(command) { return NativeTerminal.execute(command); }
$('#stop-command').addEventListener('click',()=>NativeTerminal.stop());
$('#clear-log').addEventListener('click',()=>NativeTerminal.clear());
$('#copy-log').addEventListener('click',()=>NativeTerminal.copy());
