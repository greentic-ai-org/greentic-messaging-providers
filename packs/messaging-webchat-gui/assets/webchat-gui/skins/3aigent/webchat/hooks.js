// 3aigent skin hooks. Theme handling is delegated to runtime-bootstrap's
// locale picker, which injects the toggle button (`.theme-toggle`) and
// persists the choice to sessionStorage["greentic-theme"]. The themed
// styleOptions URL swap (in runtime-bootstrap's skin.json patcher) reads the
// same key, so a page reload after toggle is enough to re-init Web Chat with
// the right bubble palette.
//
// This file is intentionally minimal — anything more would race or duplicate
// the SPA's existing theme system.

console.log('[hooks] 3aigent skin loaded');
