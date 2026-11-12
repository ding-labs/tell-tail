const $ = id => document.getElementById(id);
const hosts = { 'codex-desktop': 'Codex Desktop', 'claude-desktop': 'Claude Desktop' };
const reasons = { reauthentication: 'OAuth reauthentication required', authentication: 'Authentication required or rejected', timeout: 'Host reported a timeout', missing_executable: 'Executable or file not found', transport: 'Connection or transport failure', other: 'Failure reported by host' };
const failures = new Set(['failed', 'discovery_failed', 'unexpected_close', 'diagnostic_failed']);
let latest, displayed, paused = false, pauseUntil, cursor, nextCursor, stream, retryTimer, retryDelay = 1000, searchTimer, queryAbort, detailEvent, renderKey = '', requestNumber = 0;
const number = n => Number(n || 0).toLocaleString();
const date = ms => ms == null ? 'Timestamp unavailable' : new Date(ms).toLocaleString();
function age(ms) { if (ms == null) return 'unknown'; const s = Math.max(0, (Date.now() - ms) / 1000); return s < 60 ? `${Math.floor(s)}s ago` : s < 3600 ? `${Math.floor(s / 60)}m ago` : s < 86400 ? `${Math.floor(s / 3600)}h ago` : `${Math.floor(s / 86400)}d ago`; }
function node(tag, text, cls) { const n = document.createElement(tag); if (text != null) n.textContent = text; if (cls) n.className = cls; return n; }
function connection(text, tone) { $('connection').textContent = text; $('connection').dataset.tone = tone; }
function preserveFocus(container, nodes) { const id = container.contains(document.activeElement) ? document.activeElement?.dataset?.event : null; container.replaceChildren(...nodes); if (id) [...container.querySelectorAll('button')].find(b => b.dataset.event === id)?.focus({ preventScroll: true }); }
function inspectButton(id) { const b = node('button', 'Inspect'); b.dataset.event = String(id); b.setAttribute('aria-label', `Inspect event ${id}`); b.addEventListener('click', () => inspect(id)); return b; }
function options(snapshot) { const chosen = $('server').value; const names = [...new Set([...snapshot.events, ...snapshot.servers, ...snapshot.issues].map(e => e.server))].sort(); if (chosen && !names.includes(chosen)) names.push(chosen); const signature = names.join('\n'); if ($('server').dataset.signature === signature) return; $('server').dataset.signature = signature; $('server').replaceChildren(new Option('All servers', ''), ...names.map(n => new Option(n, n))); $('server').value = chosen; }
function renderSources(snapshot) {
  $('roots').replaceChildren(...snapshot.roots.map(r => node('p', `${hosts[r.host]} · ${r.location} · ${r.status}`, 'root-path')));
  if (!snapshot.roots.length) $('roots').append(node('p', 'Automatic discovery supports macOS desktop logs. Use --source and --log-dir for an explicit directory.'));
  $('sources').replaceChildren(...snapshot.sources.map(s => { const n = node('article', null, 'source-card'); n.append(node('strong', `${hosts[s.host]} · ${s.name}`), node('p', `Source ${s.id} · ${s.status}`), node('p', `Last file write: ${date(s.modified_ms)} (${age(s.modified_ms)})`), node('p', `${number(s.records)} physical lines · ${number(s.backlog_bytes)} bytes pending`)); if (s.history_omitted_bytes || s.oversized || s.boundaries) n.append(node('p', `${number(s.history_omitted_bytes)} history bytes omitted · ${number(s.oversized)} oversized records · ${number(s.boundaries)} truncations`)); return n; }));
  $('coverage').textContent = `${number(snapshot.counts.unknown)} unrecognized MCP records; ${number(snapshot.counts.ignored)} other or unparsed log lines. Payloads are unavailable. Recognition measures log coverage, not service health.`;
  $('limits').textContent = `${number(snapshot.retained_events)} events retained · ${number(snapshot.counts.evicted_events)} events evicted · ${number(snapshot.counts.evicted_issues)} issue groups evicted · ${number(snapshot.counts.evicted_states)} states evicted. ${snapshot.discovery_limited ? 'Discovery limit reached; some files are omitted. ' : ''}${snapshot.rejected_files ? `${number(snapshot.rejected_files)} file entries skipped. ` : ''}${snapshot.open_failures ? `${number(snapshot.open_failures)} file-open failures. ` : ''}${snapshot.retired_sources ? `${number(snapshot.retired_sources)} sources retired. ` : ''}${snapshot.view_omitted ? `${number(snapshot.view_omitted)} overview rows omitted to fit the response budget. ` : ''}Mirror matching is limited to retained evidence.`;
}
function render(snapshot) {
  document.querySelector('.local-label').textContent = snapshot.demo ? 'SYNTHETIC DEMO · NO CLIENT LOGS READ' : 'LOCAL · READ ONLY';
  displayed = snapshot; options(snapshot);
  $('event-count').textContent = number(snapshot.counts.observations); $('issue-count').textContent = number(snapshot.retained_issues); $('mirror-count').textContent = number(snapshot.counts.mirrors); $('source-count').textContent = number(snapshot.sources.length);
  const issues = snapshot.issues.filter(i => (!$('host').value || i.host === $('host').value) && (!$('server').value || i.server === $('server').value));
  $('issues-empty').hidden = issues.length > 0;
  $('issues-note').textContent = `Showing ${issues.length} of ${number(snapshot.retained_issues)} retained groups · observations, not unique outages`;
  preserveFocus($('issues'), issues.map(i => { const n = node('article', null, `issue${i.recovered_ms ? ' recovered' : ''}`); n.append(node('h3', `${hosts[i.host]} / ${i.server}`), node('p', reasons[i.reason]), node('p', `${i.phase} · ${number(i.observations)} observations · execution ${i.scope.slice(0, 6)}`, 'issue-meta'), node('p', `First: ${date(i.first_ms)} · Last: ${date(i.last_ms)}`, 'issue-meta'), node('p', i.recovered_ms ? `Recovery observed ${date(i.recovered_ms)}` : `${i.historical ? 'Historical evidence' : 'Observed while watching'} · recovery not observed`, 'issue-meta'), node('p', i.advice, 'issue-meta'), inspectButton(i.last_event_id)); return n; }));
  $('server-states').replaceChildren(...snapshot.servers.map(s => node('li', `${hosts[s.host]} / ${s.server} · ${s.kind.replaceAll('_', ' ')} · ${date(s.timestamp_ms)} · execution ${s.scope.slice(0, 6)}${s.historical ? ' · history' : ''}`)));
  $('states-note').textContent = `Showing ${snapshot.servers.length} of ${snapshot.retained_states} retained execution states.`;
  renderSources(snapshot);
}
function timeline(events) {
  preserveFocus($('timeline'), events.map(e => { const row = node('tr'); const time = node('td', date(e.timestamp_ms)); time.append(node('small', e.historical ? 'Imported history' : 'Received while watching')); const server = node('td', e.server); server.append(node('small', hosts[e.host])); const message = node('td', e.summary, failures.has(e.kind) ? 'failure-label' : ''); message.append(node('small', `${e.method || e.kind.replaceAll('_', ' ')} · ${e.level}${e.scope_known ? '' : ' · execution scope unknown'}`)); const action = node('td'); action.append(inspectButton(e.id)); row.append(time, server, message, action); return row; }));
  $('timeline-empty').hidden = events.length > 0;
  $('timeline-empty').textContent = latest?.sources.length ? 'No observations match these filters in the retained history.' : 'Waiting for supported logs. Open your desktop client as usual; discovery continues automatically.';
}
async function history() {
  queryAbort?.abort(); queryAbort = new AbortController(); const request = ++requestNumber;
  const params = new URLSearchParams(); for (const [id, key] of [['search', 'q'], ['host', 'host'], ['server', 'server'], ['category', 'category']]) if ($(id).value) params.set(key, $(id).value);
  if ($('since').value) params.set('since', String(Date.now() - Number($('since').value)));
  if (cursor) params.set('before', String(cursor)); if (paused && pauseUntil != null) params.set('until', String(pauseUntil));
  try { const res = await fetch(`/api/mcp/history?${params}`, { signal: queryAbort.signal }); if (!res.ok) throw new Error(res.status === 410 ? 'History has been evicted. Select Latest evidence.' : 'History unavailable; retained display has been kept.'); const data = await res.json(); if (request !== requestNumber) return; timeline(data.events); nextCursor = data.next_before; $('older').disabled = !nextCursor; $('page-note').textContent = `${data.events.length} shown · ${number(data.matched)} matches in this page range · ${number(data.retained)} retained`; $('history-status').textContent = ''; }
  catch (e) { if (e.name !== 'AbortError') $('history-status').textContent = e.message; }
}
function setPaused(value) { paused = value; pauseUntil = paused ? latest?.counts.observations : undefined; $('pause').textContent = paused ? 'Resume live' : 'Pause display'; $('pause').setAttribute('aria-pressed', String(paused)); $('display-note').textContent = paused ? 'Display paused. Collection and connection status continue; filters search evidence received before this pause.' : 'Following new observations. Imported history is labeled; server state is not a health check.'; }
async function inspect(id) {
  $('detail-title').textContent = 'Event evidence'; $('detail-summary').textContent = 'Loading…'; $('detail-fields').replaceChildren(); $('copy-status').textContent = ''; $('copy').disabled = true; detailEvent = null; if (!$('detail').open) $('detail').showModal();
  try { const res = await fetch(`/api/mcp/event/${id}`); if (!res.ok) throw new Error(res.status === 410 ? 'This event has left the bounded history.' : 'Evidence is unavailable.'); const e = await res.json(); detailEvent = e; $('detail-summary').textContent = e.summary; const fields = [['Client', hosts[e.host]], ['Server', e.server], ['Recorded', date(e.timestamp_ms)], ['Received', date(e.observed_ms)], ['Origin', e.historical ? 'Imported history' : 'Appended while watching'], ['Scope', e.scope_known ? `Local execution label ${e.scope}` : 'Unknown execution scope'], ['Method', e.method || 'Not available'], ['Original severity', e.level], ['Evidence locations', e.evidence.map(p => `Source ${p.source_id}, generation ${p.generation}, byte ${p.offset}`).join('; ')]]; $('detail-fields').replaceChildren(...fields.flatMap(([k, v]) => [node('dt', k), node('dd', v)])); $('copy').disabled = false; }
  catch (e) { $('detail-summary').textContent = e.message; }
}
$('close-detail').addEventListener('click', () => $('detail').close());
$('copy').addEventListener('click', async () => { if (!detailEvent) return; const e = detailEvent; const summary = `${hosts[e.host]} / ${e.server}\n${e.summary}\nRecorded: ${date(e.timestamp_ms)}\n${e.historical ? 'Historical evidence' : 'Observed while watching'}\nMethod: ${e.method || 'unavailable'}\n${e.evidence.map(p => `Source ${p.source_id}, generation ${p.generation}, byte ${p.offset}`).join('\n')}\nMetadata only; payloads omitted. No continuous health check.`; try { await navigator.clipboard.writeText(summary); $('copy-status').textContent = 'Copied diagnostic metadata'; } catch { $('copy-status').textContent = 'Clipboard unavailable; select and copy the visible evidence.'; } });
$('pause').addEventListener('click', () => { setPaused(!paused); if (!paused) { cursor = undefined; if (latest) render(latest); history(); } });
$('older').addEventListener('click', () => { if (!paused) setPaused(true); cursor = nextCursor; history(); });
$('latest').addEventListener('click', () => { cursor = undefined; setPaused(false); if (latest) render(latest); history(); });
$('filters').addEventListener('submit', e => e.preventDefault());
$('filters').addEventListener('input', () => { clearTimeout(searchTimer); searchTimer = setTimeout(() => { cursor = undefined; if (displayed) render(displayed); history(); }, 180); });
function receive(snapshot) {
  if (snapshot.schema_version !== 1 || !Array.isArray(snapshot.events) || !Array.isArray(snapshot.sources)) { connection('Unsupported watcher data; refresh after updating.', 'error'); return; }
  latest = snapshot; retryDelay = 1000; $('retry').hidden = true;
  connection(snapshot.running ? 'Watcher connected · reading local logs' : 'Watcher stopped · retaining observations', snapshot.running ? 'live' : 'warning');
  const key = JSON.stringify([snapshot.sequence, snapshot.sources, snapshot.roots]);
  if (!paused && key !== renderKey) { renderKey = key; render(snapshot); history(); }
}
function connect() {
  clearTimeout(retryTimer); stream?.close(); if (document.hidden) return;
  const own = stream = new EventSource('/api/mcp/events');
  own.addEventListener('snapshot', e => { if (stream !== own) return; try { receive(JSON.parse(e.data)); } catch { connection('Invalid watcher data; keeping the last display.', 'error'); } });
  own.onerror = async () => { if (stream !== own) return; own.close(); connection('Disconnected · displaying retained observations', 'warning'); $('retry').hidden = false; try { const res = await fetch('/api/mcp/snapshot'); if (res.ok && res.headers.get('x-tell-tail-streams-available') === '0') { connection('Live stream capacity reached · close another tab, then reconnect', 'warning'); return; } } catch {} if (stream === own && !document.hidden) { retryTimer = setTimeout(connect, retryDelay); retryDelay = Math.min(8000, retryDelay * 2); } };
}
$('retry').addEventListener('click', connect);
document.addEventListener('visibilitychange', () => { if (document.hidden) { stream?.close(); stream = undefined; clearTimeout(retryTimer); connection('Live connection paused while this tab is hidden', 'warning'); } else connect(); });
connect();
