const $ = id => document.getElementById(id);
const el = (tag, text) => { const n = document.createElement(tag); if (text != null) n.textContent = text; return n; };
const date = ms => ms == null ? 'Unavailable' : new Date(ms).toLocaleString();
const exact = n => { try { return BigInt(n).toLocaleString(); } catch { return '0'; } };
const outcome = c => c.status === 'completed' ? (c.execution_success === true ? 'Execution succeeded' : 'Completed · outcome unknown') : c.status.replaceAll('_', ' ');
export function initTraces({ chooseDefault, pause }) {
  let source, page, frozen, cursor, pending, serial = 0, detailSerial = 0, detail, stream, timer, filterTimer, retry = 1000, initialized = false;
  const now = () => frozen?.time ?? Date.now();
  const context = () => frozen || (page ? { asOf: page.as_of, epoch: page.collector_epoch } : null);
  function coverage(s) {
    const states = { disabled: 'Capture disabled', waiting_for_exporter: 'Waiting for exporter', receiving: 'Receiving tool results', quiet: 'No recent telemetry · connection unknown', stopped: 'Receiver stopped', loading_replay: 'Loading synthetic replay', replay_complete: 'Synthetic replay complete' };
    $('trace-status').textContent = states[s.status] || 'Unknown capture state';
    $('trace-coverage').textContent = s.mode === 'synthetic_replay' ? 'Synthetic fixture only. No client logs, network intake, or live plugin coverage. Imported call revisions are retained for inspection.' : s.enabled
      ? 'Result-only capture. Railway nested and parallel calls verified. Starts, cancellation, subagents and provider-level errors are unverified. All exported tool namespaces are captured; hosted providers are discovered automatically.'
      : 'Enable capture with --trace-port and --trace-token-file, configure the generated exporter, then restart the client. Passive logs do not contain complete tool-call traces.';
    if (!frozen) $('trace-plugin-options').replaceChildren(...(s.observed_plugins || []).map(p => { const o = el('option'); o.value = p; return o; }));
    if (!frozen) $('traces-tab-count').textContent = exact(s.snapshot.counts.distinct_calls);
    $('trace-limits').textContent = `${exact(s.snapshot.retained_calls)} retained calls · ${exact(s.snapshot.counts.evicted_calls)} evicted · ${exact(s.rejected_batches)} rejected batches · ${exact(s.rejected_records)} rejected records · ${exact(s.ignored_records)} other telemetry records. In-memory evidence; quiet does not mean the client disconnected.`;
  }
  async function load() {
    if (!source) return;
    pending?.abort(); pending = new AbortController(); const own = ++serial;
    const p = new URLSearchParams();
    for (const [id, key] of [['trace-q','q'],['trace-plugin','plugin'],['trace-tool','tool'],['trace-execution','execution'],['trace-outcome','status']]) if ($(id).value.trim()) p.set(key, $(id).value.trim());
    if ($('trace-since').value) p.set('since', String(now() - Number($('trace-since').value)));
    if (frozen) { p.set('as_of', frozen.asOf); p.set('epoch', frozen.epoch); }
    if (cursor) p.set('cursor', cursor);
    try {
      const r = await fetch(`/api/traces/calls?${p}`, { signal: pending.signal });
      if (!r.ok) throw new Error(r.status === 410 ? 'This snapshot expired. Select Latest calls to load retained evidence.' : 'Calls unavailable. The displayed evidence has been kept.');
      const data = await r.json(); if (own !== serial) return; page = data;
      const focusId = $('trace-rows').contains(document.activeElement) ? document.activeElement.dataset.call : null;
      $('trace-rows').replaceChildren(...data.calls.map(c => {
        const row = el('tr'); const time = el('td', date(c.completed_ms ?? c.received_ms)); time.append(el('small', c.historical ? 'Imported replay' : c.completed_ms == null ? 'Received · completion time unavailable' : 'Completed'));
        const tool = el('td', c.plugin || (c.tool.startsWith('functions.') ? 'Client tool' : 'Unidentified plugin')); tool.append(el('small', c.tool));
        const execution = el('td', c.execution.slice(0, 8)); execution.title = c.execution;
        const action = el('td'); const b = el('button', 'Inspect'); b.dataset.call = c.id; b.setAttribute('aria-label', `Inspect call ${c.id}`); b.addEventListener('click', () => inspect(c.id)); action.append(b);
        row.append(time, tool, el('td', outcome(c)), el('td', c.duration_ms == null ? 'Unavailable' : `${c.duration_ms.toLocaleString()} ms`), execution, action); return row;
      }));
      if (focusId) [...$('trace-rows').querySelectorAll('button')].find(b => b.dataset.call === focusId)?.focus({ preventScroll: true });
      $('trace-empty').hidden = data.calls.length > 0;
      $('trace-empty').textContent = source.enabled ? 'No captured calls match these filters. Completed calls appear after the client exports them.' : 'Tool capture is disabled. The log watcher continues independently.';
      $('trace-older').disabled = !data.next_cursor;
      $('trace-page-note').textContent = `${data.calls.length} shown · ${data.matched} matches in this page range${frozen ? ' · paused snapshot' : ''}`;
    } catch (e) { if (own === serial && e.name !== 'AbortError') $('trace-page-note').textContent = e.message; }
  }
  async function inspect(id) {
    const at = context(); if (!at) return; const own = ++detailSerial; detail = null;
    $('trace-detail-fields').replaceChildren(); $('trace-detail-note').textContent = 'Loading…'; $('trace-copy-status').textContent = ''; $('trace-copy').disabled = true; $('trace-detail').showModal();
    try {
      const p = new URLSearchParams({ as_of: at.asOf, epoch: at.epoch }); const r = await fetch(`/api/traces/call/${id}?${p}`);
      if (!r.ok) throw new Error(r.status === 410 ? 'This call snapshot expired. Resume to load current evidence.' : 'Call details unavailable.');
      const c = await r.json(); if (own !== detailSerial) return; detail = c;
      $('trace-detail-note').textContent = outcome(c);
      const fields = [['Tool',c.tool],['Plugin',c.plugin || 'Unidentified'],['Attribution',c.historical ? 'Synthetic fixture label' : c.plugin ? 'Hosted provider namespace; not a manifest ID' : 'Source namespace retained; plugin mapping unverified'],['Transport server',c.server || 'Unavailable'],['Completed',date(c.completed_ms)],['Received',date(c.received_ms)],['Duration',c.duration_ms == null ? 'Unavailable' : `${c.duration_ms} ms (${c.duration_basis})`],['Timing scope','Client-reported tool duration; queue and approval scope unverified'],['Execution',c.execution],['Call / revision',`${c.id} / ${c.revision}`],['Lifecycle',`${c.start_observed ? 'Start observed' : 'Start not observed'}; ${c.completion_observed ? 'completion observed' : 'completion not observed'}`],['Provider-level outcome',c.historical ? 'Synthetic fixture only' : 'Unavailable from this adapter'],['Parent call','Unavailable'],['Evidence gap',c.gap_observed ? 'Yes' : 'None recorded; completeness is not guaranteed'],['Origin',c.historical ? 'Imported replay' : 'Live telemetry']];
      $('trace-detail-fields').replaceChildren(...fields.flatMap(([k,v]) => [el('dt',k),el('dd',v)])); $('trace-copy').disabled = false;
    } catch (e) { if (own === detailSerial) $('trace-detail-note').textContent = e.message; }
  }
  document.addEventListener('displaypause', e => {
    pending?.abort(); serial++;
    if (e.detail) { if (!frozen && source) frozen = { asOf: page?.as_of ?? source.snapshot.watermark, epoch: page?.collector_epoch ?? source.snapshot.collector_epoch, time: Date.now() }; }
    else { frozen = null; cursor = null; if (source) coverage(source); load(); }
  });
  $('trace-filters').addEventListener('submit', e => e.preventDefault());
  $('trace-filters').addEventListener('input', () => { clearTimeout(filterTimer); filterTimer = setTimeout(() => { cursor = null; load(); }, 180); });
  $('trace-clear').addEventListener('click', () => { clearTimeout(filterTimer); $('trace-filters').reset(); cursor = null; load(); });
  $('trace-latest').addEventListener('click', () => pause(false));
  $('trace-older').addEventListener('click', () => { const next = page?.next_cursor; if (!next) return; pause(true); cursor = next; load(); });
  $('trace-close').addEventListener('click', () => $('trace-detail').close());
  $('trace-detail').addEventListener('close', () => { detailSerial++; detail = null; });
  $('trace-copy').addEventListener('click', async () => { if (!detail) return; try { await navigator.clipboard.writeText(JSON.stringify({ schema_version: 1, note: 'Diagnostic metadata only; provider outcome unverified', call: detail }, null, 2)); $('trace-copy-status').textContent = 'Copied call metadata'; } catch { $('trace-copy-status').textContent = 'Clipboard unavailable; copy the visible fields.'; } });
  function receive(s) {
    if (s.schema_version !== 1 || !s.snapshot) return;
    const changed = !source || source.snapshot.watermark !== s.snapshot.watermark || source.snapshot.collector_epoch !== s.snapshot.collector_epoch;
    source = s; retry = 1000; coverage(s);
    if (!initialized) { initialized = true; if (s.enabled) chooseDefault(); }
    if (!frozen && changed) load();
  }
  function connect() {
    clearTimeout(timer); stream?.close(); if (document.hidden) return;
    const own = stream = new EventSource('/api/traces/events');
    own.addEventListener('trace_source', e => { if (own === stream) { try { receive(JSON.parse(e.data)); } catch { $('trace-status').textContent = 'Invalid trace status; retaining displayed calls'; } } });
    own.onerror = () => { if (own !== stream) return; own.close(); $('trace-status').textContent = 'Trace connection lost · retaining displayed calls'; timer = setTimeout(connect, retry); retry = Math.min(8000,retry*2); };
  }
  document.addEventListener('visibilitychange', () => { if (document.hidden) { stream?.close(); stream = null; clearTimeout(timer); pending?.abort(); serial++; } else connect(); });
  connect();
}
