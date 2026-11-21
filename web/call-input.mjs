// Command text is fetched only while the local inspector is expanded.
export function inputDisclosure(context, url) {
  const details = document.createElement('details');
  details.className = 'detail-section';
  const label = document.createElement('summary');
  label.textContent = `Show ${context.input_kind.toLowerCase()}${context.truncated ? ' preview' : ''}`;
  const pre = document.createElement('pre');
  pre.className = 'call-input';
  details.append(label, pre);
  let generation = 0;
  details.addEventListener('toggle', async () => {
    const current = ++generation;
    pre.textContent = '';
    if (!details.open) return;
    pre.textContent = 'Loading…';
    try {
      const response = await fetch(url, {cache:'no-store', headers:{'X-Tell-Tail-Inspect':'local-input'}});
      if (!response.ok) throw new Error();
      const data = await response.json();
      if (current === generation && details.isConnected) {
        pre.textContent = data.input + (context.truncated ? '\n… (preview limit)' : '');
      }
    } catch {
      if (current === generation) pre.textContent = 'This input is unavailable in the retained view.';
    }
  });
  return details;
}

export function diagnosticField(key, value) {
  return key === 'inspection' ? undefined : value;
}

function element(tag, text, className) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = text;
  return node;
}

const outcomes = {
  observed:'Outcome unavailable', completed:'Completed', nonzero_exit:'Nonzero exit',
  failed:'Failed', denied:'Denied', interrupted:'Interrupted', running:'Running',
  incomplete:'Incomplete', conflict:'Conflicting outcomes',
};
const unsuccessful = tool => ['nonzero_exit','failed','denied','interrupted','conflict'].includes(tool.outcome);

function toolList(tools) {
  const list = element('ul', null, 'turn-tools');
  for (const tool of tools) {
    const row = element('li');
    const label = tool.operation || tool.name;
    row.append(element('strong',label));
    const outcome = tool.exit_code == null ? outcomes[tool.outcome] : 'Exit '+tool.exit_code;
    const detail = [tool.selected ? 'Selected call' : '', tool.operation ? tool.name : '', outcome].filter(Boolean);
    row.append(element('small',detail.join(' · ')));
    if (unsuccessful(tool)) row.className = 'turn-failure';
    list.append(row);
  }
  return list;
}

export function turnPanel(id, source, toolName) {
  const panel = element('section', null, 'detail-section turn-panel');
  const heading = element('h3','Chat turn');
  const body = element('div','Loading turn…');
  panel.append(heading, body);
  async function read() {
    try {
      const params = new URLSearchParams({source});
      const r = await fetch('/api/traces/call/'+encodeURIComponent(id)+'/turn?'+params, {
        cache:'no-store', headers:{'X-Tell-Tail-Inspect':'local-turn'},
      });
      if (!r.ok) throw new Error();
      const turn = await r.json();
      if (!panel.isConnected) return;
      body.replaceChildren();
      if (turn.matched) {
        body.append(element('h4','Your request'));
        for (const request of turn.requests) body.append(element('blockquote', request));
        if (!turn.requests.length) body.append(element('p','The user message is unavailable in the retained history.'));
        if (turn.requested_ms != null) body.append(element('small',new Date(turn.requested_ms).toLocaleString()));
        if (toolName === 'functions.exec') {
          body.append(element('h4','Tools inside this call'));
          if (turn.children.length) body.append(toolList(turn.children));
          if (!turn.children.length && turn.children_complete) body.append(element('p','Codex recorded zero nested tool calls.'));
          if (!turn.children_complete) body.append(element('p',turn.children.length ?
            'Additional nested calls may be missing.' : 'Codex supplied no parent link for the tools in this recording.'));
        }
        const failures = turn.tools.filter(unsuccessful);
        body.append(element('h4','Failures in this turn'));
        if (failures.length) body.append(toolList(failures));
        else body.append(element('p','No failure appears in the available tool results.'));
        const tools = element('details');
        tools.append(element('summary', 'All tools in this turn ('+turn.tools.length+')'),toolList(turn.tools));
        body.append(tools);
        if (turn.partial) body.append(element('p','Showing a bounded portion of local history.'));
      } else {
        body.append(element('p','An exact turn match is unavailable in the local chat history.'));
      }
    } catch {
      if (!panel.isConnected) return;
      body.replaceChildren(element('p','Turn evidence is currently unavailable.'));
    }
    const refresh = element('button','Refresh turn');
    refresh.type = 'button';
    refresh.addEventListener('click', () => {refresh.disabled=true; read();});
    body.append(refresh);
  }
  read();
  return panel;
}
