import { mkdir, copyFile } from 'node:fs/promises';
await mkdir('frontend/vendor', {recursive:true});
for (const [source,target] of [
  ['@xterm/xterm/lib/xterm.js','xterm.js'],
  ['@xterm/xterm/css/xterm.css','xterm.css'],
  ['@xterm/addon-fit/lib/addon-fit.js','addon-fit.js'],
  ['@xterm/xterm/LICENSE','xterm-LICENSE'],
  ['@xterm/addon-fit/LICENSE','addon-fit-LICENSE']
]) await copyFile(`node_modules/${source}`,`frontend/vendor/${target}`);
console.log('Terminal assets prepared (local files, no CDN).');
