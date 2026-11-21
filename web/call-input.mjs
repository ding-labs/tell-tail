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
