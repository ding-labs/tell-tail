export const categories = {tools:'Tool calls',mcp:'MCP',connections:'Connections',desktop:'Desktop'};
export const outcomes = {observed:'Observed',opened_observed:'Opened observed',closed_observed:'Closed observed',retry_scheduled:'Retry scheduled',ready_observed:'Ready observed',start_observed:'Start observed',completed:'Completed',failure_reported:'Failure reported',execution_succeeded:'Execution succeeded',execution_failed:'Execution failed',command_failed:'Command exited nonzero',tool_error:'Tool reported error',cancelled:'Cancelled',denied:'Denied',incomplete:'Incomplete',conflicting_evidence:'Conflicting evidence'};
export const scopes = {app_wide:'App-wide',unknown:'Scope unknown',server_execution:'Server execution',unlinked_execution:'Execution',named_chat:'Codex chat'};
export const sourceNames = {desktop_logs:'Desktop log',mcp_logs:'MCP diagnostics',tool_telemetry:'Tool telemetry'};
export const clientNames = {chatgpt_codex_desktop:'ChatGPT / Codex desktop',codex_exporter:'Codex exporter',claude_desktop:'Claude Desktop'};
export function clock(ms) {return Number.isFinite(ms)?new Date(ms).toLocaleTimeString([], {hour12:false,hour:'2-digit',minute:'2-digit',second:'2-digit'}):'Unknown';}
export function dateTime(ms) {return Number.isFinite(ms)?new Date(ms).toLocaleString([], {hour12:false}):'Not reported';}
export function duration(ms) {if(!Number.isFinite(ms))return '—';if(ms<1000)return `${ms.toLocaleString()} ms`;if(ms<60000)return `${(ms/1000).toLocaleString(undefined,{maximumFractionDigits:2})} s`;return `${(ms/60000).toLocaleString(undefined,{maximumFractionDigits:1})} min`;}
export function age(ms, now) {if(!Number.isFinite(ms))return 'not observed';const seconds=Math.max(0,Math.floor((now-ms)/1000));if(seconds<1)return 'just now';if(seconds<60)return `${seconds}s ago`;if(seconds<3600)return `${Math.floor(seconds/60)}m ago`;if(seconds<86400)return `${Math.floor(seconds/3600)}h ago`;return `${Math.floor(seconds/86400)}d ago`;}
export function el(tag, cls, text) {const node=document.createElement(tag);if(cls)node.className=cls;if(text!==undefined)node.textContent=String(text);return node;}
export {safePreferences,savePreferences} from './preferences.mjs';
export function rangeForBin(bins,index) {const width=(bins.end_ms-bins.start_ms)/60;return [Math.ceil(bins.start_ms+width*index),Math.ceil(bins.start_ms+width*(index+1))];}
