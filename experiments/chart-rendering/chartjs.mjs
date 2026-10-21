// Benchmark-only adapter. Keep the existing SVG grid, labels and exact-value
// table, and replace only the animated plot with Chart.js Canvas rendering.
let instance;
let lastSecond;
let lastPaint = -Infinity;
let forcePaint = false;
const targetHz = location.search.includes('chartjs30') ? 30 : location.search.includes('chartjs20') ? 20 : 0;
const paintInterval = targetHz ? 1000 / targetHz : 0;
// A 60 Hz callback often arrives after 33.3 ms rather than exactly 33.333 ms.
// Avoid accidentally dropping every second 30 Hz paint due to clock rounding.
const paintTolerance = targetHz === 30 ? .5 : 0;

export function updateCanvas(vertices, animate) {
  if (!vertices.length) return;
  const svg = document.getElementById('chart');
  if (!instance) {
    const surface = document.createElement('div');
    surface.style.position = 'relative';
    svg.before(surface);
    surface.append(svg);
    const canvas = document.createElement('canvas');
    canvas.id = 'chart-canvas';
    const { width, height } = svg.getBoundingClientRect();
    canvas.width = width;
    canvas.height = height;
    Object.assign(canvas.style, { position: 'absolute', top: '0', left: '0', width: `${width}px`, height: `${height}px`, pointerEvents: 'none' });
    surface.append(canvas);
    for (const id of ['chart-line', 'chart-area', 'chart-dots']) document.getElementById(id).style.visibility = 'hidden';
    instance = new Chart(canvas, {
      type: 'line',
      data: { datasets: [{ data: [], borderColor: '#eac377', backgroundColor: 'rgba(234,195,119,0.08)',
        borderWidth: 2 * width / 760, pointRadius: 2.5 * width / 760, pointBackgroundColor: '#eac377', pointBorderWidth: 0,
        cubicInterpolationMode: 'monotone', fill: 'origin' }] },
      options: {
        responsive: false, maintainAspectRatio: false, devicePixelRatio: window.devicePixelRatio,
        parsing: false, normalized: true, events: [],
        animation: { duration: 1000, easing: 'linear', onComplete({ chart }) {
          forcePaint = true;
          chart.draw();
          forcePaint = false;
        } },
        layout: { autoPadding: false, padding: { left: width * 60 / 760, right: width * 12 / 760,
          top: height * 20 / 224, bottom: height * 46 / 224 } },
        plugins: { legend: { display: false }, tooltip: { enabled: false } },
        scales: { x: { type: 'linear', display: false }, y: { type: 'linear', display: false, min: 0 } },
      },
      plugins: [{ id: 'benchmark-paints', beforeDraw() {
        const now = performance.now();
        if (!forcePaint && now - lastPaint + paintTolerance < paintInterval) return false;
        lastPaint = now;
      }, afterDraw() { window.benchPaint?.(); } }],
    });
  }
  const data = instance.data.datasets[0].data;
  if (!animate) instance.stop();
  const end = vertices.at(-1).second;
  // Preserve point identity across the rolling window. Replacing all 60 items
  // would morph counts in place instead of scrolling each second leftward.
  if (animate && end === lastSecond + 1 && data.length === vertices.length) {
    data.shift();
    const p = vertices.at(-1);
    data.push({ x: p.second, y: p.value });
  } else {
    instance.data.datasets[0].data = vertices.map(p => ({ x: p.second, y: p.value }));
  }
  lastSecond = end;
  instance.options.scales.x.min = vertices[0].second;
  instance.options.scales.x.max = end;
  instance.options.scales.y.max = Math.max(1, ...vertices.map(p => p.value));
  forcePaint = !animate;
  instance.update(animate ? undefined : 'none');
  forcePaint = false;
  window.benchCoordinates = () => instance.getDatasetMeta(0).data.map(p => ({ x: p.x, y: p.y }));
}
