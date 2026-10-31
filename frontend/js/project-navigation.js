'use strict';
// Shared project navigation for the library and command workbench.
const ProjectNavigation = (() => {
  const trigger = $('#project-switch');
  const panel = document.createElement('div');
  panel.id = 'project-switcher';
  panel.className = 'project-switcher';
  panel.setAttribute('popover','auto');
  panel.setAttribute('aria-label','切换项目');
  panel.setAttribute('role','dialog');
  panel.innerHTML = '<label class="sr-only" for="switch-project-search">搜索项目名称或目录</label><input id="switch-project-search" type="search" placeholder="搜索项目名称或目录…" autocomplete="off"><p class="switch-caption">最近项目</p><div class="switch-results" aria-label="可切换的项目"></div><a class="switch-manage" href="projects.html">查看全部项目</a>';
  document.body.append(panel);
  const input = panel.querySelector('input');
  const results = panel.querySelector('.switch-results');
  let signature = '';
  function render() {
    const current = new URLSearchParams(location.search).get('project');
    const data = ProjectStore.read().filter(p=>!p.archived).sort((a,b)=>(b.openedAt || 0)-(a.openedAt || 0) || a.name.localeCompare(b.name,'zh-CN'));
    const needle = input.value.trim().toLocaleLowerCase();
    const visible = data.filter(p=>`${p.name} ${p.path}`.toLocaleLowerCase().includes(needle));
    const next = JSON.stringify([current,needle,visible.map(p=>[p.id,p.name,p.path])]);
    if(next===signature) return;
    signature=next;
    results.innerHTML = visible.map(p=>`<a href="index.html?project=${encodeURIComponent(p.id)}" ${p.id===current ? 'aria-current="page"' : ''}><span><strong>${escapeHTML(p.name)}</strong><code title="${escapeHTML(p.path)}">${escapeHTML(p.path)}</code></span><span class="switch-current">${p.id===current ? '当前' : ''}</span></a>`).join('') || '<p class="switch-empty">'+(needle ? '没有匹配的项目' : '还没有项目，可在项目目录执行 runbook project add')+'</p>';
    panel.querySelector('.switch-caption').textContent = needle ? `找到 ${visible.length} 个项目` : '最近项目';
  }
  function close() { if(panel.matches(':popover-open')) panel.hidePopover(); }
  function position() {
    const rect=trigger.getBoundingClientRect();
    panel.style.left=`${Math.max(12,Math.min(rect.left,innerWidth-Math.min(400,innerWidth-24)-12))}px`;
    panel.style.top=`${rect.bottom+8}px`;
    panel.style.maxHeight=`${Math.max(160,innerHeight-rect.bottom-24)}px`;
  }
  trigger.addEventListener('click',()=>{
    if(panel.matches(':popover-open')) {close();return;}
    input.value='';render();position();panel.showPopover();input.focus();
  });
  panel.addEventListener('toggle',event=>trigger.setAttribute('aria-expanded',String(event.newState==='open')));
  input.addEventListener('input',render);
  panel.addEventListener('keydown',event=>{
    if(event.key==='Escape') {event.preventDefault();close();trigger.focus();return;}
    const links=[...results.querySelectorAll('a')];
    if(!links.length) return;
    const i=links.indexOf(document.activeElement);
    if(event.key==='Enter' && event.target===input) {event.preventDefault();links[0].click();}
    if(event.key==='ArrowDown' || event.key==='ArrowUp') {
      event.preventDefault();
      const next=i<0 ? (event.key==='ArrowDown' ? 0 : links.length-1) : (i+(event.key==='ArrowDown' ? 1 : -1)+links.length)%links.length;
      links[next].focus();
    }
  });
  results.addEventListener('click',event=>{
    if(event.target.closest('[aria-current="page"]')) {event.preventDefault();close();trigger.focus();}
  });
  window.addEventListener('runbook:projects-changed',()=>{if(panel.matches(':popover-open') && !results.contains(document.activeElement))render();});
  window.addEventListener('resize',close);
  window.addEventListener('scroll',event=>{if(!panel.contains(event.target))close();},true);
  return {render,close};
})();
