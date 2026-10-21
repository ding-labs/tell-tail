const { render, stopChartAnimation } = await import(`/render.mjs${location.search}`);

const fixture = await (await fetch('/fixture.json')).json();
const value = second => Math.round(80 + 55 * Math.sin(second * .31) + 20 * Math.cos(second * .83));
const paints = [];
let recording = false;
window.benchPaint = () => { if (recording) paints.push(performance.now()); };
if (!location.search.includes('chartjs')) {
  new MutationObserver(records => {
    for (const r of records) if (r.attributeName === 'd') window.benchPaint();
  }).observe(document.getElementById('chart-line'), { attributes: true });
}
window.benchFrame = seconds => {
  const frame = structuredClone(fixture);
  const end = Math.floor(seconds);
  const history = Array.from({ length: 60 }, (_, i) => value(end - 60 + i));
  const total = Array.from({ length: end }, (_, i) => value(i)).reduce((a,b) => a+b, 0);
  Object.assign(frame.metrics, { state: 'reading', kind: 'snapshot', session_id: 'controlled',
    elapsed_seconds: seconds, history: history.map(String), bytes_read: String(total * 200) });
  frame.metrics.totals.records = String(total);
  frame.metrics.totals.levels.info = String(total);
  frame.metrics.window.observed_seconds = 60 + (seconds % .1);
  frame.metrics.window.records_per_second = history.reduce((a,b)=>a+b,0)/60;
  frame.metrics.window.bytes_per_second = frame.metrics.window.records_per_second*200;
  render(frame);
};
window.benchStartRecording = () => { paints.length = 0; recording = true; };
window.benchStopRecording = () => { recording = false; return paints; };
window.benchPause = () => { clearInterval(window.benchTimer); stopChartAnimation(); };
window.benchFrame(60);
const start = performance.now();
window.benchTimer = setInterval(() => window.benchFrame(60 + (performance.now() - start) / 1000), 100);
window.benchReady = true;
