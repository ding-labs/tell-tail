// Appearance only: never persist sources, queries, payloads or endpoint credentials.
let memory={theme:'system',compact:false},unavailable=false;
export function safePreferences(){
 if(!unavailable)try{const value=JSON.parse(localStorage.getItem('tell-tail-preferences')||localStorage.getItem('tell-tail-activity-preferences')||'{}');memory={theme:['system','light','dark'].includes(value?.theme)?value.theme:'system',compact:value?.compact===true};}catch{unavailable=true;}
 return {...memory};
}
export function savePreferences(change){memory={...safePreferences(),...change};try{localStorage.setItem('tell-tail-preferences',JSON.stringify(memory));}catch{unavailable=true;}}
