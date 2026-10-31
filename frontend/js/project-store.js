'use strict';
// GUI 与 CLI 只使用后端的共享数据库，不维护浏览器副本。
const commands = [];
const ProjectStore = createNativeStore();
