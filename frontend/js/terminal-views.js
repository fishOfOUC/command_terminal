'use strict';
// Keep xterm buffers and viewports per session. Only a cold attach reads history from sequence zero.
function createTerminalViews({container,onData,onResize,theme}) {
  const views=new Map();let activeId=null;
  function position(view){const buffer=view.terminal.buffer.active;return {line:buffer.viewportY,bottom:buffer.viewportY>=buffer.baseY};}
  function restore(view,where){if(where.bottom)view.terminal.scrollToBottom();else view.terminal.scrollToLine(Math.min(where.line,view.terminal.buffer.active.baseY));}
  function create(id) {
    const host=document.createElement('div');host.className='terminal-instance';host.dataset.session=id;host.style.visibility='hidden';container.append(host);
    const terminal=new Terminal({fontFamily:'"Cascadia Code", Consolas, monospace',fontSize:13,lineHeight:1.35,cursorBlink:true,scrollback:10000,smoothScrollDuration:0,theme:theme()});
    const fit=new FitAddon.FitAddon();terminal.loadAddon(fit);terminal.open(host);
    terminal.onData(data=>onData(id,data));terminal.onResize(size=>onResize(id,size));
    const view={id,host,terminal,fit,sequence:0,revealed:false,position:{line:0,bottom:true}};views.set(id,view);return view;
  }
  return {
    get:id=>views.get(id),
    active:()=>views.get(activeId),
    prepare(id){
      const previous=views.get(activeId);
      if(previous?.revealed)previous.position=position(previous);
      for(const view of views.values()){view.host.hidden=true;view.revealed=false;}
      const view=views.get(id) || create(id);activeId=id;
      view.host.hidden=false;view.host.style.visibility='hidden';view.fit.fit();return view;
    },
    async append(view,snapshot){
      if(snapshot.truncated)view.terminal.writeln('[部分未读取输出已从会话缓冲区移除]');
      if(snapshot.chunks.length){
        const decoded=snapshot.chunks.map(chunk=>Uint8Array.from(atob(chunk),c=>c.charCodeAt(0)));
        const bytes=new Uint8Array(decoded.reduce((size,chunk)=>size+chunk.length,0));let offset=0;
        for(const chunk of decoded){bytes.set(chunk,offset);offset+=chunk.length;}
        await new Promise(resolve=>view.terminal.write(bytes,resolve));
      }
      view.sequence=snapshot.nextSequence;
    },
    reveal(view){
      if(activeId!==view.id || views.get(view.id)!==view)return;
      restore(view,view.position);view.host.style.visibility='';view.revealed=true;
    },
    fit(){const view=views.get(activeId);if(!view?.revealed)return;const where=position(view);view.fit.fit();restore(view,where);},
    updateTheme(){for(const view of views.values())view.terminal.options.theme=theme();},
    remove(id){const view=views.get(id);if(!view)return;view.terminal.dispose();view.host.remove();views.delete(id);if(activeId===id)activeId=null;}
  };
}
