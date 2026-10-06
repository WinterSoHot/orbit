export type Theme='light'|'dark';
export const themeKey='orbit.theme';
type ThemeStorage=Pick<Storage,'getItem'|'setItem'>;
export function readTheme(storage?:ThemeStorage,systemDark=false):Theme {
  try {const value=storage?.getItem(themeKey);if(value==='light'||value==='dark')return value;}catch{/* Storage may be unavailable; appearance still works. */}
  return systemDark?'dark':'light';
}
export function initialTheme():Theme {
  if(typeof window==='undefined')return 'light';
  let storage:ThemeStorage|undefined;try{storage=window.localStorage;}catch{/* Private storage can deny access. */}
  return readTheme(storage,window.matchMedia('(prefers-color-scheme: dark)').matches);
}
export function applyTheme(theme:Theme){document.documentElement.dataset.orbitTheme=theme;document.documentElement.style.colorScheme=theme;}
export function saveTheme(theme:Theme,storage:ThemeStorage):boolean {try{storage.setItem(themeKey,theme);return true;}catch{return false;}}
