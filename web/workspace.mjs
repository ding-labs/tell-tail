import {safePreferences,savePreferences} from './preferences.mjs';
function appearance(){const {theme}=safePreferences();document.documentElement.dataset.theme=theme;const button=document.getElementById('theme');if(button){button.textContent=theme==='system'?'System theme':`${theme[0].toUpperCase()+theme.slice(1)} theme`;button.setAttribute('aria-label',`Appearance: ${theme}`);}}
const button=document.getElementById('theme');
button?.addEventListener('click',()=>{const themes=['system','dark','light'];savePreferences({theme:themes[(themes.indexOf(safePreferences().theme)+1)%themes.length]});appearance();});
window.addEventListener('storage',appearance);
window.addEventListener('pageshow',appearance);
appearance();
