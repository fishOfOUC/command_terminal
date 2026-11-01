'use strict';
// Apply the saved theme before styles paint, and share it between both pages and terminal views.
const RunbookTheme=(()=>{
  const key='runbook.theme';let current='dark';
  try{if(localStorage.getItem(key)==='light')current='light';}catch{}
  const sun='<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5 19 19M5 19l1.5-1.5M17.5 6.5 19 5"/>';
  const moon='<path d="M20 15.5A8.5 8.5 0 0 1 8.5 4 8.5 8.5 0 1 0 20 15.5Z"/>';
  function syncButton(){
    document.querySelectorAll('[data-theme-toggle]').forEach(button=>{
      const label=current==='dark' ? '切换到浅色模式' : '切换到深色模式';
      button.setAttribute('aria-label',label);button.title=label;
      button.innerHTML=`<svg class="od-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">${current==='dark' ? sun : moon}</svg><span class="theme-label">${current==='dark' ? '浅色' : '深色'}</span>`;
    });
  }
  function nativeTheme(){
    if(window.__TAURI__?.core)window.__TAURI__.core.invoke('set_theme',{theme:current}).catch(error=>console.warn('Window theme:',error));
  }
  function set(theme,persist=true){
    current=theme==='light' ? 'light' : 'dark';document.documentElement.dataset.theme=current;
    document.querySelector('meta[name=color-scheme]')?.setAttribute('content',current);
    if(persist)try{localStorage.setItem(key,current);}catch{}
    syncButton();nativeTheme();window.dispatchEvent(new Event('runbook:theme-changed'));
  }
  function terminal(){
    const style=getComputedStyle(document.documentElement),color=name=>style.getPropertyValue(name).trim();
    const light=current==='light';
    return {background:color('--terminal'),foreground:color('--text'),cursor:color('--accent'),selectionBackground:light ? '#c6e2d5' : '#1d302c',black:light ? '#202b24' : '#171719',red:light ? '#b43744' : '#ff9ca7',green:light ? '#20765e' : '#78d9ba',yellow:light ? '#875a14' : '#e3c58a',blue:light ? '#1d5bc1' : '#9bbcf2',magenta:light ? '#8b4897' : '#d9a6ca',cyan:light ? '#1d6c83' : '#98d8dd',white:light ? '#58645e' : '#f1f2f3',brightBlack:light ? '#59685e' : '#92989d',brightRed:light ? '#b43744' : '#ff9ca7',brightGreen:light ? '#20765e' : '#78d9ba',brightYellow:light ? '#875a14' : '#e3c58a',brightBlue:light ? '#1d5bc1' : '#9bbcf2',brightMagenta:light ? '#8b4897' : '#d9a6ca',brightCyan:light ? '#1d6c83' : '#98d8dd',brightWhite:light ? '#25322a' : '#fff'};
  }
  document.documentElement.dataset.theme=current;
  document.querySelector('meta[name=color-scheme]')?.setAttribute('content',current);
  document.addEventListener('DOMContentLoaded',()=>{
    syncButton();nativeTheme();document.querySelectorAll('[data-theme-toggle]').forEach(button=>button.addEventListener('click',()=>set(current==='dark' ? 'light' : 'dark')));
  });
  window.addEventListener('storage',event=>{if(event.key===key)set(event.newValue,false);});
  return {set,terminal,get current(){return current;}};
})();
