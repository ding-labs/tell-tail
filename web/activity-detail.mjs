import {inputDisclosure} from './call-input.mjs';
import {outcomes,scopes,sourceNames,clientNames,dateTime,duration,el} from './activity-format.mjs';
import {title} from './activity-feed.mjs';
function fields(values){const dl=el('dl','detail-fields');for(const [key,value] of values){dl.append(el('dt','',key),el('dd','',value));}return dl;}
function disclosure(label){const section=el('details','detail-section');section.append(el('summary','',label));return section;}
export function renderDetail(container,item,group,loadRelated,onSelect,inputUrl,showScope) {
  container.replaceChildren();
  const heading=el('h2','',title(item));heading.id='detail-title';
  const badge=el('span','detail-outcome',outcomes[item.outcome]||'Observed');badge.dataset.severity=item.severity;
  container.append(heading,badge);
  if(item.category==='tools'){
    const context=item.inspection;
    const values=[['Tool',item.subject]];
    if(context?.chat)values.unshift(['Chat',context.chat]);
    if(context?.directory)values.push(['Working directory',context.directory]);
    if(context?.parameters?.length)values.push(['Input fields',context.parameters.join(', ')]);
    if(context?.exit_code!=null)values.push(['Shell exit code',context.exit_code]);
    container.append(fields(values));
    if(context?.input_kind)container.append(inputDisclosure(context,inputUrl));
    if(item.scope_ref){
      const button=el('button','related-event',context?.chat?'Show calls in this chat':'Show calls in this execution');
      button.onclick=()=>showScope(item);
      container.append(button);
    }
  }
  container.append(el('p','detail-note',item.note));
  if(group){
    const section=el('section','detail-section');section.append(el('h3','','Failure group'),fields([['Observations',group.observations],['First reported',dateTime(group.first_ms)],['Last reported',dateTime(group.last_ms)],['Recovery',group.recovered_ms===null?'No matching recovery observed':dateTime(group.recovered_ms)]]));container.append(section);
  }
  if(item.advice){const advice=el('section','detail-section');advice.append(el('h3','','Suggested check'),el('p','',item.advice));container.append(advice);}
  container.append(fields([
    ['Source',sourceNames[item.source]||item.source],['Client',clientNames[item.client]||item.client],
    ['Scope',scopes[item.scope]||'Unknown'],['Recorded',dateTime(item.occurred_ms)],['Received',dateTime(item.received_ms)],
    ['Duration',item.duration_ms===null?'Not reported':`${duration(item.duration_ms)} · ${item.duration_basis==='source_timestamps'?'source timestamps':'source reported'}`],
    ['Origin',item.historical?'Imported history':'Received after collector startup'],
  ]));
  if(item.category==='tools')container.append(fields([
    ['Start seen',item.start_observed?'Yes':'No'],['Result seen',item.completion_observed?'Yes':'No'],
    ['Execution',item.execution_success===null?'Outcome not reported':item.execution_success?'Reported success':'Reported failure'],
    ['Tool error',item.tool_error===null?'Not reported':item.tool_error?'Reported':'Not reported by tool'],
    ['Observed gap',item.gap_observed?'Yes; evidence incomplete':'None reported'],
  ]));
  const evidence=disclosure('Provenance and adapter details');
  const values=[['Kind',item.kind],['Adapter',item.adapter],['Source ID',item.source_ref],['Source level',item.source_level],['Local scope',item.scope_ref||'No verified scope']];
  if(item.method)values.push(['Method',item.method]);if(item.attempt!==null)values.push(['Retry count',item.attempt]);
  evidence.append(fields(values));
  for(const ref of item.evidence||[])evidence.append(el('p','',`${ref.source} · generation ${ref.generation} · byte ${ref.offset}`));
  evidence.append(el('p','','Copied diagnostics omit local call inspection details.'));container.append(evidence);
  for(const mode of ['linked','nearby']){
    if(mode==='nearby'&&item.occurred_ms===null)continue;
    const section=disclosure(mode==='linked'?'Linked evidence':'Nearby in time');
    const list=el('ul','evidence-list');
    section.append(el('p','',mode==='linked'?'Results with the same execution identifier.':'Within five seconds of the source timestamp. Timing does not establish a causal link.'),list);
    let loaded=false;
    section.addEventListener('toggle',async()=>{
      if(!section.open||loaded)return;loaded=true;
      if(mode==='linked'&&!item.scope_ref){list.append(el('li','muted','No verified shared scope is available.'));return;}
      list.append(el('li','muted','Loading retained evidence…'));
      try{const page=await loadRelated(item,mode);if(!section.isConnected)return;list.replaceChildren();const others=page.items.filter(row=>row.id!==item.id).slice(0,10);
        for(const row of others){const li=el('li'),button=el('button','related-event',`${dateTime(row.occurred_ms ?? row.received_ms)} · ${title(row)}`);button.onclick=()=>onSelect(row.id,button);li.append(button);list.append(li);}
        if(!others.length)list.append(el('li','muted','No other matching evidence is retained in this view.'));
        if(page.matched>11)list.append(el('li','muted','Showing up to 10 nearby or linked items.'));
      }catch{list.replaceChildren(el('li','muted','Related evidence is unavailable or this frozen view has expired.'));}
    });
    container.append(section);
  }
}
