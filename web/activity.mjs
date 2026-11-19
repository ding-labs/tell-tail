import {categories,age,el,safePreferences,savePreferences} from './activity-format.mjs';
import {renderRows,renderBins,renderExactTimes} from './activity-feed.mjs';
import {renderSources,sourceSummary} from './activity-sources.mjs';
import {renderDetail} from './activity-detail.mjs';

const $=id=>document.getElementById(id);
const defaults=()=>({q:'',category:'',range:'all',interval:null,client:'',source:'',plugin:'',server:'',outcome:'',historical:'',diagnostics:false});
const preferences=safePreferences();
const state={view:'activity',filters:{activity:defaults(),tools:defaults(),problems:defaults()},snapshot:null,sources:null,page:null,bins:null,groups:new Map(),epoch:null,at:null,latest:null,latestEpoch:null,paused:false,held:false,cursor:null,next:null,selected:null,item:null,group:null,loading:false,connected:false,error:'',generation:0,request:null,detailRequest:null,detailGeneration:0,returnFocus:null,now:Date.now()};
const labels={activity:'Activity',tools:'Tools & MCP',problems:'Problems',sources:'Sources'};
const narrow=matchMedia('(max-width:1099px)');
let refreshTimer, searchTimer, stream;
const frozen=()=>state.paused||state.held||state.cursor!==null||state.selected!==null||$('times-dialog').open;
const filter=()=>state.filters[state.view]||state.filters.activity;

function applyPreferences(){
  const theme=['light','dark'].includes(preferences.theme)?preferences.theme:'system';
  document.documentElement.dataset.theme=theme;
  document.documentElement.dataset.density=preferences.compact?'compact':'comfortable';
  $('theme').textContent=theme==='system'?'System theme':`${theme[0].toUpperCase()+theme.slice(1)} theme`;
  $('theme').setAttribute('aria-label',`Appearance: ${theme}`);
  $('density').setAttribute('aria-pressed',String(!!preferences.compact));
}
function persist(){savePreferences({theme:preferences.theme,compact:!!preferences.compact});}
function cancelView(){state.request?.abort();state.generation++;state.loading=false;}
function params(at,epoch,withCursor=false,now=state.now){
  const f=filter(),p=new URLSearchParams({as_of:at,epoch,view:state.view==='sources'?'activity':state.view});
  for(const key of ['q','category','client','source','plugin','server','outcome','historical'])if(f[key])p.set(key,f[key]);
  if(f.diagnostics)p.set('diagnostics','true');
  if(f.range==='custom'&&f.interval){p.set('since',String(f.interval[0]));p.set('until',String(f.interval[1]));}
  else if(f.range!=='all'){p.set('since',String(Math.max(0,Math.floor(now-Number(f.range)))));}
  if(withCursor&&state.cursor)p.set('cursor',state.cursor);
  return p;
}
async function json(path,signal){
  const response=await fetch(path,{cache:'no-store',signal});
  if(!response.ok){const error=new Error(response.status===410?'This view is no longer retained. Return to live to load current evidence.':`Could not read activity (${response.status}).`);error.expired=response.status===410;throw error;}
  return response.json();
}
function updateStatus(){
  $('pause').disabled=!state.snapshot;
  const pending=state.latest!==null&&(state.latest!==state.at||state.latestEpoch!==state.epoch);
  $('pause').textContent=state.paused?'Resume display':'Pause display';$('pause').setAttribute('aria-pressed',String(state.paused));
  $('pending').hidden=!(state.held||state.cursor||pending&&frozen()||state.error);
  $('pending').textContent=pending?'Updates available · Return to live':'Return to live';
  const connection=$('connection');
  connection.dataset.state=state.connected?'connected':'disconnected';
  connection.textContent=state.connected?'Connected locally':'Disconnected';
  if(state.sources?.running===false&&state.connected){connection.dataset.state='stopped';connection.textContent='Collector stopped';}
  const now=frozen()?state.now:Date.now();
  if(state.sources){const summary=sourceSummary(state.sources,now);$('source-status').textContent=summary.logs;$('trace-status').textContent=summary.trace;}
  let note=state.error;
  if(!note&&!state.connected)note='Connection lost. Showing the last received view; reconnecting automatically.';
  if(!note&&state.paused)note='Display paused. Collection continues; filters and evidence use this frozen view.';
  if(!note&&frozen())note='Reviewing a fixed view. New observations will not move the evidence you are reading.';
  if(!note&&state.sources?.demo)note='Synthetic demo · No client logs are read and no live capture is implied.';
  if(!note&&state.page)note=`${state.page.diagnostics_hidden||0} routine diagnostics hidden${state.bins?.unknown_time?` · ${state.bins.unknown_time} items have no source time`:''} · App-wide observations do not establish chat state.`;
  if(!note)note='Reading supported desktop observations…';
  $('display-note').textContent=note;$('display-note').title=note;$('display-note').dataset.tone=state.error||!state.connected?'warning':'neutral';
  const snapshot=state.snapshot;
  if(snapshot)$('retention').textContent=`${state.sources?.demo?'Synthetic data · ':''}In-memory history · ${snapshot.evicted_items} items evicted${state.sources?.projection_gaps!=='0'?` · ${state.sources.projection_gaps} coalesced/missed revisions`:''}`;
}
function facetOptions(id,values,caption){
  const node=$(id),value=filter()[id];
  const wanted=[...new Set([...values,...(value?[value]:[])])];
  if(JSON.stringify([...node.options].slice(1).map(o=>o.value))===JSON.stringify(wanted)){node.value=value;return;}
  node.replaceChildren(new Option(caption,''),...wanted.map(v=>new Option(v,v)));node.value=value;
}
function syncControls(){
  const f=filter();$('view-title').textContent=labels[state.view];
  document.querySelectorAll('[data-view]').forEach(n=>{if(n.dataset.view===state.view)n.setAttribute('aria-current','page');else n.removeAttribute('aria-current');});
  $('query-controls').hidden=state.view==='sources';$('sources-view').hidden=state.view!=='sources';$('rows').hidden=state.view==='sources';$('list-heading').hidden=state.view==='sources';$('pagination').hidden=state.view==='sources';
  for(const id of ['client','source','plugin','server','outcome','historical'])$(id).value=f[id];
  $('search').value=f.q;$('diagnostics').checked=f.diagnostics;
  $('time-range').querySelector('[value=custom]').hidden=f.range!=='custom';$('time-range').value=f.range;
  document.querySelectorAll('button[data-category]').forEach(b=>{b.setAttribute('aria-pressed',String(b.dataset.category===f.category));b.hidden=state.view==='tools'&&['connections','desktop'].includes(b.dataset.category);});
  $('filter-count').textContent=['client','source','plugin','server','outcome','historical'].filter(k=>f[k]).length||'';
}
function showEmpty(){
  const empty=state.view!=='sources'&&!state.page?.items?.length;
  $('empty').hidden=!empty;if(!empty)return;
  const f=filter(),filtered=Object.entries(f).some(([k,v])=>!['range','interval','diagnostics'].includes(k)&&v)||f.range!=='all';
  $('empty-reset').hidden=!filtered;
  let title='No matching activity',note='Try another category, time range or search. Filters apply to all retained evidence.';
  if(!filtered){
    if(state.view==='problems'){title='No reported failures in this view';note='This means no matching failures were observed. It does not establish app or server health.';}
    else if(!state.sources?.files?.length&&!state.sources?.trace?.enabled){title='Waiting for supported sources';note='Discovery continues automatically. Open Sources to see the locations checked and capture options.';}
    else if(state.view==='tools'){title='Waiting for tool and MCP observations';note=state.sources?.trace?.enabled?'No matching results are retained yet. Check Sources for exporter status and coverage.':'MCP logs are passive. Enable tool telemetry through Tell-Tail Setup in Codex to capture plugin results.';}
    else {title='Waiting for recognized activity';note='Sources can be quiet or contain unsupported records. Check Sources for freshness and coverage; silence is not a failure.';}
  }
  $('empty-title').textContent=title;$('empty-note').textContent=note;
}
async function load({live=false}={}){
  cancelView();const generation=state.generation,request=new AbortController();state.request=request;state.loading=true;
  try{
    let snapshot=state.snapshot;
    if(live||!snapshot)snapshot=await json('/api/activity/snapshot',request.signal);
    if(generation!==state.generation)return;
    const epoch=snapshot.collector_epoch,at=live||!state.at?snapshot.watermark:state.at;
    const now=live||!state.at?snapshot.observed_ms:state.now;
    const query=params(at,epoch,false,now),pageQuery=params(at,epoch,true,now);
    const responses=await Promise.all([
      json(`/api/activity/sources?${query}`,request.signal),
      state.view==='sources'?null:json(`/api/activity/items?${pageQuery}`,request.signal),
      state.view==='sources'?null:json(`/api/activity/bins?${query}`,request.signal),
      state.view==='problems'?json(`/api/activity/problems?${query}`,request.signal):null,
    ]);
    if(generation!==state.generation)return;
    const [sourceResult,page,bins,problems]=responses;
    state.snapshot=snapshot;state.sources=sourceResult.sources;state.epoch=epoch;state.at=at;state.now=now;state.error='';
    state.page=page;state.bins=bins;state.next=page?.next_cursor||null;state.groups=new Map();
    if(state.view==='sources'){
      $('view-count').textContent='Sources and coverage';$('empty').hidden=true;
      renderSources($('sources-view'),state.sources,state.now,snapshot);
    }else{
      if(problems){state.groups=new Map(problems.groups.map(g=>[g.item_id,g]));page.items=problems.groups.map(g=>g.item);state.next=null;}
      renderRows($('rows'),page.items,state.selected,inspect,state.groups);
      renderBins(bins,selectRange);
      $('view-count').textContent=problems?`${problems.matched} failure group${problems.matched===1?'':'s'}`:`${page.matched.toLocaleString()} item${page.matched===1?'':'s'}`;
      $('diagnostic-count').textContent=page.diagnostics_hidden?`(${page.diagnostics_hidden} hidden)`:'';
      $('older').disabled=!state.next;
      $('page-note').textContent=problems?`${problems.groups.length} groups shown${problems.omitted?` · ${problems.omitted} omitted; narrow the filters`:''}`:`${page.items.length} shown · ${page.matched} match${page.matched===1?'':'es'}`;
      facetOptions('plugin',page.facets.plugins,'All observed plugins');facetOptions('server',page.facets.servers,'All observed servers');
      showEmpty();
    }
    updateStatus();
  }catch(error){if(error.name==='AbortError'||generation!==state.generation)return;state.error=error.message;state.held=true;updateStatus();}
  finally{if(generation===state.generation){state.loading=false;if(!frozen()&&state.latest&&state.latest!==state.at)schedule();}}
}
function schedule(){
  clearTimeout(refreshTimer);refreshTimer=setTimeout(()=>{if(!state.loading&&!frozen())load({live:true});},200);
}
function closeInspector(restore=true){
  state.detailRequest?.abort();state.detailGeneration++;state.selected=null;state.item=null;state.group=null;
  if($('inspector').open)$('inspector').close();
  $('workspace').dataset.inspector='false';
  document.querySelectorAll('.event-row').forEach(row=>row.setAttribute('aria-pressed','false'));
  if(restore&&state.returnFocus?.isConnected)state.returnFocus.focus({preventScroll:true});
}
function showInspector(){
  const dialog=$('inspector');$('workspace').dataset.inspector='true';
  if(!dialog.open){if(narrow.matches)dialog.showModal();else dialog.show();}
}
async function inspect(id,focus,group=null){
  cancelView();state.held=true;state.selected=id;state.item=null;state.group=group;
  if(focus&&!$('inspector').contains(focus))state.returnFocus=focus;
  state.detailRequest?.abort();const request=new AbortController();state.detailRequest=request;const generation=++state.detailGeneration;
  const placeholder=el('h2','','Loading evidence…');placeholder.id='detail-title';$('detail-content').replaceChildren(placeholder);$('copy').disabled=true;$('copy-status').textContent='';
  document.querySelectorAll('.event-row').forEach(row=>row.setAttribute('aria-pressed',String(row.dataset.id===id)));
  showInspector();updateStatus();
  const at=state.at,epoch=state.epoch;
  try{
    const item=await json(`/api/activity/item/${encodeURIComponent(id)}?${new URLSearchParams({as_of:at,epoch})}`,request.signal);
    if(generation!==state.detailGeneration||id!==state.selected)return;
    state.item=item;
    const related=async(selected,mode)=>{
      const query=new URLSearchParams({as_of:at,epoch,diagnostics:'true'});
      if(mode==='linked')query.set('scope',selected.scope_ref);
      else {query.set('since',String(Math.max(0,selected.occurred_ms-5000)));query.set('until',String(selected.occurred_ms+5001));}
      return json(`/api/activity/items?${query}`,request.signal);
    };
    renderDetail($('detail-content'),item,group,related,inspect);$('copy').disabled=false;
  }catch(error){if(error.name==='AbortError')return;placeholder.textContent='Evidence unavailable';$('detail-content').append(el('p','detail-note',error.message));}
}
function changed(){
  clearTimeout(searchTimer);closeInspector(false);state.cursor=null;state.next=null;syncControls();$('feed').scrollTop=0;load({live:!frozen()});
}
function selectRange(start,end,category){
  if($('times-dialog').open)$('times-dialog').close();
  filter().range='custom';filter().interval=[start,end];filter().category=category;state.held=true;changed();
}
function returnLive(){
  closeInspector(false);if($('times-dialog').open)$('times-dialog').close();state.paused=false;state.held=false;state.cursor=null;state.next=null;state.error='';$('feed').scrollTop=0;if(matchMedia('(max-width:759px)').matches)window.scrollTo({top:0});load({live:true});updateStatus();
}
function connect(){
  stream?.close();stream=new EventSource('/api/activity/events');
  stream.onopen=()=>{state.connected=true;updateStatus();};
  stream.onerror=()=>{state.connected=false;updateStatus();};
  stream.addEventListener('activity',event=>{
    let value;try{value=JSON.parse(event.data);}catch{return;}
    if(typeof value.watermark!=='string'||typeof value.collector_epoch!=='string')return;
    state.latest=value.watermark;state.latestEpoch=value.collector_epoch;
    if(state.epoch&&state.epoch!==value.collector_epoch){
      state.error='Collector restarted. Previous in-memory history is gone; return to live.';
      if(!frozen()){state.cursor=null;load({live:true});}
    }else if(!frozen())schedule();
    updateStatus();
  });
}

$('filters').addEventListener('submit',event=>event.preventDefault());
$('search').addEventListener('input',()=>{filter().q=$('search').value;clearTimeout(searchTimer);searchTimer=setTimeout(changed,180);});
for(const id of ['client','source','plugin','server','outcome','historical'])$(id).addEventListener('change',()=>{filter()[id]=$(id).value;changed();});
$('time-range').addEventListener('change',()=>{filter().range=$('time-range').value;filter().interval=null;changed();});
$('diagnostics').addEventListener('change',()=>{filter().diagnostics=$('diagnostics').checked;changed();});
for(const button of document.querySelectorAll('button[data-category]'))button.onclick=()=>{filter().category=button.dataset.category;changed();};
for(const button of document.querySelectorAll('[data-view]'))button.onclick=()=>{
  if(state.view===button.dataset.view)return;
  closeInspector(false);state.cursor=null;state.view=button.dataset.view;syncControls();$('feed').scrollTop=0;load({live:!frozen()});
};
for(const id of ['clear','empty-reset'])$(id).onclick=()=>{state.filters[state.view]=defaults();changed();};
$('pause').onclick=()=>{if(state.paused){returnLive();return;}cancelView();state.paused=true;updateStatus();};
$('pending').onclick=returnLive;$('latest').onclick=returnLive;
$('older').onclick=()=>{if(!state.next)return;closeInspector(false);state.held=true;state.cursor=state.next;$('feed').scrollTop=0;load();};
$('feed').addEventListener('scroll',()=>{if($('feed').scrollTop>8){state.held=true;updateStatus();}},{passive:true});
window.addEventListener('scroll',()=>{if(matchMedia('(max-width:759px)').matches&&window.scrollY>64){state.held=true;updateStatus();}},{passive:true});
$('rows').addEventListener('focusin',()=>{state.held=true;updateStatus();});
$('lanes').addEventListener('focusin',()=>{state.held=true;updateStatus();});
$('close-detail').onclick=()=>closeInspector();
$('inspector').addEventListener('cancel',event=>{event.preventDefault();closeInspector();});
narrow.addEventListener('change',()=>{if($('inspector').open){$('inspector').close();showInspector();}});
$('copy').onclick=async()=>{if(!state.item)return;try{await navigator.clipboard.writeText(`Tell-Tail diagnostic summary · Metadata only\n${JSON.stringify({event:state.item,group:state.group||undefined},null,2)}`);$('copy-status').textContent='Copied sanitized metadata.';}catch{$('copy-status').textContent='Clipboard unavailable in this browser.';}};
$('exact-times').onclick=()=>{if(!state.bins)return;state.held=true;renderExactTimes($('times-values'),state.bins,selectRange);$('times-dialog').showModal();updateStatus();};
$('close-times').onclick=()=>$('times-dialog').close();
$('theme').onclick=()=>{const themes=['system','dark','light'];preferences.theme=themes[(themes.indexOf(preferences.theme||'system')+1)%themes.length];applyPreferences();persist();};
$('density').onclick=()=>{preferences.compact=!preferences.compact;applyPreferences();persist();};
document.addEventListener('pointerdown',event=>{if(!$('more-filters').contains(event.target))$('more-filters').open=false;});
document.addEventListener('keydown',event=>{if(event.key==='Escape'&&$('inspector').open&&!$('times-dialog').open){event.preventDefault();closeInspector();}});
window.addEventListener('pagehide',()=>{stream?.close();state.request?.abort();state.detailRequest?.abort();});
$('timezone').textContent=`Times: ${Intl.DateTimeFormat().resolvedOptions().timeZone||'local timezone'}`;
applyPreferences();syncControls();connect();load({live:true});
