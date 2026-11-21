import {age,el} from './activity-format.mjs';
const traceNames={disabled:'Disabled',waiting_for_exporter:'Waiting for exporter',receiving:'Receiving telemetry',quiet:'No recent telemetry',stopped:'Receiver stopped',unavailable:'Receiver unavailable',synthetic_replay:'Synthetic replay'};
function section(title,status,text){const root=el('section','source-section'),header=el('div','source-section-header');header.append(el('h3','',title),el('span','source-state',status));root.append(header,el('p','',text));return root;}
function counters(root,values){const dl=el('dl');for(const [label,value] of values){const entry=el('div');entry.append(el('dt','',label),el('dd','',value));dl.append(entry);}root.append(dl);}
export function renderSources(container,sources,now,snapshot) {
  container.replaceChildren();
  const intro=el('div','source-intro');intro.append(el('h2','','Know what is being observed'),el('p','','Sources report observations, not continuous app health. Tool telemetry is opt-in. Prompt content, tool outputs and native chat identifiers are omitted.'));container.append(intro);
  const files=sources.files||[],roots=sources.roots||[];
  const logs=section('Desktop logs and MCP diagnostics',sources.running?`${files.length} files followed`:'Collector stopped','The desktop adapter recognizes connection, plugin lifecycle, browser startup and runtime check events. MCP retains its server lifecycle and failure diagnostics. Other record bodies are discarded.');
  counters(logs,[['Unrecognized records',sources.unsupported_records||'0'],['Other records omitted',sources.ignored_records||'0'],['Rejected files',sources.rejected_files||'0'],['Open failures',sources.open_failures||'0'],['Retired sources',sources.retired_sources||'0'],['Discovery limit',sources.discovery_limited?'Reached; some files omitted':'Not reached']]);
  for(const root of roots)logs.append(el('p','',`${root.location} · ${root.status}`));
  if(!roots.length)logs.append(el('p','',sources.demo?'Replay uses no client log roots.':'No automatic roots on this platform. Use --source and --log-dir for an explicit directory.'));
  const details=el('details'),summary=el('summary','',`Followed files (${files.length})`),list=el('ul','source-files');details.append(summary);
  for(const file of files){const li=el('li'),name=el('span','',`Source ${file.id} · ${file.name}`),state=el('small','',`${file.status} · modified ${age(file.modified_ms,now)} · ${file.records} records`);li.append(name,state);list.append(li);if([file.oversized,file.history_omitted_bytes,file.backlog_bytes].some(v=>v!=null&&String(v)!=='0'))list.append(el('li','',`Source ${file.id}: ${file.oversized} oversized records · ${file.history_omitted_bytes} history bytes omitted · ${file.backlog_bytes} bytes pending`));}
  details.append(list);logs.append(details);container.append(logs);
  const trace=sources.trace||{};
  const telemetry=section('Tool telemetry',traceNames[trace.status]||'Unknown',sources.demo?'Synthetic tool observations exercise the same call store. This is not live capture verification.':'Result-only observations from the configured Codex exporter. Hosted plugin attribution comes from the observed namespace. Starts, parent relationships and complete cloud coverage are not established.');
  counters(telemetry,[['Last received',age(trace.last_received_ms,now)],['Rejected batches',trace.rejected_batches||'0'],['Other telemetry omitted',trace.ignored_records||'0']]);
  if(!trace.enabled)telemetry.append(el('p','','Use Tell-Tail Setup in Codex to enable the local exporter. Passive desktop logs continue without telemetry setup.'));
  container.append(telemetry);
  const retention=section('Retention and coverage',sources.demo?'Synthetic data':'Metadata only','History is bounded and in memory. Collector restarts clear it. Source silence is not a failure, and records close in time are not necessarily related.');
  counters(retention,[['Retained activity items',snapshot?.retained_items??'—'],['Evicted items',snapshot?.evicted_items??'0'],['Evicted revisions',snapshot?.evicted_revisions??'0'],['Coalesced or missed projections',sources.projection_gaps||'0']]);
  retention.append(el('p','',trace.inspecting?'Local call inspection is enabled. Tool results can include a matched chat title, shell exit code and an on-demand input preview.':'Local call inspection is off. Enable with tracing upgrade --inspect-calls to match chat titles and inspect shell and tool-script inputs.'));
  container.append(retention);
}
export function sourceSummary(sources,now){const trace=sources.trace||{};return {logs:sources.running?`${sources.files?.length||0} log source${sources.files?.length===1?'':'s'} followed`:'Collector stopped',trace:`Tools: ${traceNames[trace.status]||'status unknown'}`,freshness:trace.last_received_ms?`Last telemetry ${age(trace.last_received_ms,now)}`:''};}
