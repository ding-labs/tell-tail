import {categories,outcomes,scopes,clientNames,clock,duration,dateTime,el,rangeForBin} from './activity-format.mjs';
const symbols={tools:'↗',mcp:'⌘',connections:'◎',desktop:'▣'};
export function title(row) {
  if(row.inspection?.operation)return row.inspection.operation;
  if(row.plugin && row.category==='tools')return `${row.plugin} · ${row.subject.split('.').pop().replace(/^_/, '').replaceAll('_',' ')}`;
  if(row.category==='tools')return `${row.plugin||'Tool'} · ${row.subject}`;
  if(row.category==='mcp')return `${row.subject} · ${row.summary}`;
  return row.summary;
}
export function renderRows(container,rows,selected,onSelect,groups=new Map()) {
  const prior=new Map([...container.children].map(n=>[n.dataset.id,n]));
  for(const row of rows){
    let wrapper=prior.get(row.id);
    if(!wrapper){wrapper=el('div');wrapper.dataset.id=row.id;wrapper.setAttribute('role','listitem');wrapper.append(el('button','event-row'));}
    prior.delete(row.id);
    const button=wrapper.firstElementChild;
    button.type='button';button.dataset.id=row.id;button.dataset.severity=row.severity;button.dataset.outcome=row.outcome;
    button.setAttribute('aria-pressed',String(row.id===selected));
    button.onclick=()=>onSelect(row.id,button,groups.get(row.id));
    const time=el('span','event-time',clock(row.occurred_ms ?? row.received_ms));
    time.title=row.occurred_ms===null?'Collector receipt time':dateTime(row.occurred_ms);
    const symbol=el('span','event-symbol',row.severity==='error'?'!':row.severity==='warning'?'△':symbols[row.category]);symbol.setAttribute('aria-hidden','true');
    const body=el('span','event-body');
    const heading=el('span','event-title',title(row));heading.title=title(row);
    const context=row.inspection;
    const scope=context?.chat || (row.scope==='unlinked_execution' && row.scope_ref ? `Execution ${row.scope_ref.slice(5,13)}` : scopes[row.scope]);
    const meta=[scope||'Scope unknown',context?.operation ? row.subject : categories[row.category]];
    if(context?.directory)meta.push(context.directory);
    if(row.occurred_ms==null)meta.push('Receipt time');
    if(row.client==='claude_desktop')meta.push(clientNames[row.client]);
    if(row.historical)meta.push('Imported history');
    else if(row.occurred_ms!==null&&row.received_ms-row.occurred_ms>5000)meta.push('Received later');
    const group=groups.get(row.id);
    if(group)meta.push(`${group.observations} observation${group.observations===1?'':'s'}`);
    body.append(heading,el('span','event-meta',meta.join(' · ')));
    const result=el('span','event-result',group?.recovered_ms!==null&&group?.recovered_ms!==undefined?'Recovery observed':outcomes[row.outcome]||'Unknown outcome');
    if(context?.exit_code!=null)result.append(el('span','command-exit',`Exit ${context.exit_code}`));
    if(row.duration_ms!==null)result.append(el('span','duration',duration(row.duration_ms)));
    button.replaceChildren(time,symbol,body,result);
    container.append(wrapper);
  }
  for(const node of prior.values())node.remove();
}
export function renderBins(bins,onRange) {
  const lanes=document.getElementById('lanes');lanes.replaceChildren();
  const meaningful=Number.isFinite(bins.start_ms)&&Number.isFinite(bins.end_ms)&&bins.end_ms-bins.start_ms>1;
  document.getElementById('time-navigator').hidden=!meaningful;
  if(!meaningful)return;
  const max=Math.max(1,...bins.lanes.flatMap(l=>l.counts));
  for(const lane of bins.lanes){
    const row=el('div','lane');row.dataset.category=lane.category;
    row.append(el('span','',categories[lane.category]));
    const track=el('div','track');track.setAttribute('role','group');track.setAttribute('aria-label',categories[lane.category]+' event intervals');
    lane.counts.forEach((count,index)=>{
      if(!count)return;
      const [start,end]=rangeForBin(bins,index),failures=lane.failures?.[index]||0;
      const button=el('button','bin');button.type='button';button.style.left=`${index/60*100}%`;button.style.setProperty('--height',`${4+8*Math.sqrt(count/max)}px`);
      if(failures)button.style.color='var(--error)';
      const label=`${categories[lane.category]}, ${dateTime(start)} to ${dateTime(end)}, ${count} observed item${count===1?'':'s'}${failures?`, ${failures} failure${failures===1?'':'s'}`:''}`;
      button.setAttribute('aria-label',label);button.title=label;button.onclick=()=>onRange(start,end,lane.category);
      track.append(button);
    });
    row.append(track);lanes.append(row);
  }
  document.getElementById('time-start').textContent=clock(bins.start_ms);
  document.getElementById('time-middle').textContent=clock((bins.start_ms+bins.end_ms)/2);
  document.getElementById('time-end').textContent=clock(bins.end_ms);
}
export function renderExactTimes(container,bins,onRange) {
  container.replaceChildren();
  if(bins.unknown_time)container.append(el('p','detail-note',`${bins.unknown_time} matching items have no source timestamp and are not placed on the time axis.`));
  const table=el('table'),head=el('thead'),tr=el('tr');
  for(const text of ['Interval begins','Category','Items','']){const th=el('th','',text);th.scope='col';tr.append(th);}head.append(tr);table.append(head);
  const body=el('tbody');
  for(let index=0;index<60;index++)for(const lane of bins.lanes){
    const count=lane.counts[index];if(!count)continue;
    const [start,end]=rangeForBin(bins,index),row=el('tr');
    row.append(el('td','',new Date(start).toLocaleString(undefined,{hour12:false,fractionalSecondDigits:3})),el('td','',categories[lane.category]),el('td','',count));
    const action=el('td'),button=el('button','','View');button.setAttribute('aria-label',`View ${categories[lane.category]} interval starting ${dateTime(start)}`);button.onclick=()=>onRange(start,end,lane.category);action.append(button);row.append(action);body.append(row);
  }
  table.append(body);container.append(table);
}
