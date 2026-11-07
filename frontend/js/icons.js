'use strict';
// SVG 图标路径表与 icon() 生成器。

const paths = {
  play:'<path d="m8 5 11 7-11 7z"/>',
  chevron:'<path d="m9 5 7 7-7 7"/>',
  down:'<path d="m5 9 7 7 7-7"/>',
  edit:'<path d="m15 4 5 5M4 20l5-1L21 7l-5-5L4 14z"/>',
  copy:'<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>',
  plus:'<path d="M12 5v14M5 12h14"/>',
  close:'<path d="m6 6 12 12M18 6 6 18"/>',
  trash:'<path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7"/>'
};
const icon = name => `<svg class="od-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">${paths[name] || paths.play}</svg>`;
