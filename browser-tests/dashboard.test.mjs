import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { chromium, monitor } from './helpers.mjs';

const fixture = JSON.parse(await readFile(new URL('../tests/fixtures/output/empty-web-v1.json', import.meta.url), 'utf8'));

test('recent error review is safe, frozen, filterable and bounded under a flood', async t => {
  const server = await monitor();
  t.after(() => server.close());
  const browser = await chromium.launch();
  t.after(() => browser.close());
  const page = await browser.newPage();
  server.child.stdin.write('INFO ready\nERROR first\nFATAL <img src=x onerror="window.unsafe=true">\n');
  await page.goto(server.url);
  await page.waitForFunction(() => document.querySelectorAll('#recent-errors li').length === 2);
  assert.match(await page.locator('#recent-errors li').first().textContent(), /FATAL <img/);
  assert.equal(await page.locator('#recent-errors img').count(), 0);
  assert.equal(await page.evaluate(() => window.unsafe), undefined);
  await page.locator('#fatal-only').click();
  assert.equal(await page.locator('#recent-errors li').count(), 1);
  await page.locator('#freeze').click();
  server.child.stdin.write('ERROR later\n');
  await page.waitForFunction(async () => (await (await fetch('/api/snapshot')).json()).metrics.totals.records === '4');
  await page.locator('#fatal-only').click();
  assert.equal(await page.locator('#recent-errors li').count(), 2, 'filter reviews the frozen sample');
  assert.doesNotMatch(await page.locator('#recent-errors').textContent(), /later/);
  await page.locator('#freeze').click();
  await page.waitForFunction(() => document.querySelectorAll('#recent-errors li').length === 3);
  server.child.stdin.end(Array.from({ length: 25 }, (_, i) => `ERROR flood ${i} ${'é'.repeat(400)}\n`).join(''));
  await page.waitForFunction(() => document.getElementById('input-state').textContent === 'complete');
  assert.equal(await page.locator('#recent-errors li').count(), 20);
  assert.match(await page.locator('#recent-errors li').first().textContent(), /entry 29.*ERROR flood 24/s);
  assert.match(await page.locator('#recent-errors li').last().textContent(), /ERROR flood 5/);
  assert.match(await page.locator('#recent-errors li').first().textContent(), /Preview shortened/);
  await page.setViewportSize({ width: 320, height: 900 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  await page.locator('#fatal-only').click();
  assert.equal(await page.locator('#recent-errors').isVisible(), false);
  assert.match(await page.locator('#recent-empty').textContent(), /No fatal records/);
});

test('embedded dashboard renders real final metrics, diagnostics and responsive layout', async t => {
  const server = await monitor();
  t.after(() => server.close());
  const browser = await chromium.launch();
  t.after(() => browser.close());
  const page = await browser.newPage({ viewport: { width: 1440, height: 1100 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  const requests = [];
  page.on('request', request => requests.push(request.url()));
  server.child.stdin.end('INFO ready\nWARN slow\nERROR failed\n{bad}\n');
  await page.goto(server.url);
  await page.waitForFunction(() => document.getElementById('input-state').textContent === 'complete');
  assert.equal(await page.locator('#total-records').textContent(), '4');
  assert.equal(await page.locator('#warning-ratio').textContent(), '25%');
  assert.equal(await page.locator('#error-ratio').textContent(), '25%');
  await page.locator('.diagnostics summary').click();
  assert.equal(await page.locator('#parse-failures').textContent(), '1');
  assert.equal(await page.locator('#backlog').textContent(), '—');
  assert.match(await page.locator('#window-note').textContent(), /warming up/);
  for (const width of [1440, 768, 390, 320]) {
    await page.setViewportSize({ width, height: 1100 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, `overflow at ${width}px`);
  }
  assert.equal(requests.every(url => url.startsWith(server.url)), true, 'all requests stay on loopback');
  assert.deepEqual(errors, []);
  assert.equal(server.stdout, '');
});

test('null values, u64 counters and untrusted sources render safely', async t => {
  const server = await monitor();
  t.after(() => server.close());
  const browser = await chromium.launch();
  t.after(() => browser.close());
  const page = await browser.newPage();
  const frame = structuredClone(fixture);
  frame.metrics.source = '<img src=x onerror="window.unsafe=true"> / 日誌.log';
  frame.metrics.elapsed_seconds = 62.9;
  frame.metrics.history = ['1', '2', '18446744073709551615'];
  frame.metrics.totals.records = '18446744073709551615';
  frame.metrics.totals.levels.info = frame.metrics.totals.records;
  await page.route('**/api/events', route => route.fulfill({ contentType: 'text/event-stream', body: `event: snapshot\ndata: ${JSON.stringify(frame)}\n\n` }));
  await page.goto(server.url);
  await page.waitForFunction(() => document.getElementById('input-state').textContent === 'complete');
  assert.equal(await page.locator('#source').textContent(), frame.metrics.source);
  assert.equal(await page.locator('#source img').count(), 0);
  assert.equal(await page.evaluate(() => window.unsafe), undefined);
  assert.equal(await page.locator('#total-records').textContent(), '18,446,744,073,709,551,615');
  assert.equal(await page.locator('#warning-ratio').textContent(), '—');
  assert.equal(await page.locator('#record-rate').textContent(), '0');
  await page.locator('.chart-data summary').click();
  assert.equal(await page.locator('#chart-values tr').count(), 3);
  assert.match(await page.locator('#chart-values').textContent(), /18,446,744,073,709,551,615/);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
});

test('chart curves stay bounded and animate between seconds with motion controls', async t => {
  const server = await monitor();
  t.after(() => server.close());
  const browser = await chromium.launch();
  t.after(() => browser.close());
  const page = await browser.newPage();
  await page.route('**/api/events', route => route.fulfill({ contentType: 'text/event-stream', body: `event: snapshot\ndata: ${JSON.stringify(fixture)}\n\n` }));
  await page.goto(server.url);
  await page.waitForFunction(() => document.getElementById('input-state').textContent === 'complete');
  await page.clock.install();
  await page.clock.pauseAt(new Date());
  const update = async (end, history, session = 'motion', state = 'reading') => page.evaluate(async ({ fixture, end, history, session, state }) => {
    const { render } = await import('/render.mjs');
    const frame = structuredClone(fixture);
    Object.assign(frame.metrics, { elapsed_seconds: end, history, session_id: session, state });
    render(frame);
  }, { fixture, end, history, session, state });
  const path = () => page.locator('#chart-line').getAttribute('d');
  await update(60, Array.from({ length: 60 }, (_, i) => i % 2 ? '100' : '0'));
  assert.match(await path(), / C/);
  const bounds = await page.locator('#chart-line').evaluate(el => {
    const length = el.getTotalLength();
    const ys = Array.from({ length: 1001 }, (_, i) => el.getPointAtLength(length * i / 1000).y);
    return [Math.min(...ys), Math.max(...ys)];
  });
  assert.ok(bounds[0] >= 19.99 && bounds[1] <= 178.01, 'curve never invents negative counts or overshoots peaks');
  await page.evaluate(() => {
    window.chartPaints = 0;
    window.chartObserver = new MutationObserver(records => {
      window.chartPaints += records.filter(record => record.attributeName === 'd').length;
    });
    window.chartObserver.observe(document.getElementById('chart-line'), { attributes: true });
  });
  await update(61, Array.from({ length: 60 }, (_, i) => i % 2 ? '0' : '100'));
  const dotX = () => page.locator('#chart-dots circle[data-second="30"]').evaluate(el => el.getBoundingClientRect().x);
  const atStart = await dotX();
  const animationTime = time => page.evaluate(time => {
    const animations = document.getElementById('chart-motion').getAnimations();
    if (animations.length !== 1) throw new Error('expected one native scroll animation');
    animations[0].pause();
    animations[0].currentTime = time;
  }, time);
  const before = await path();
  await animationTime(250);
  const during = await dotX();
  await animationTime(500);
  assert.ok(await dotX() < during && during < atStart, 'native motion continues between incoming samples');
  assert.equal(await path(), before, 'scrolling reuses SVG geometry');
  const beforeInterrupt = await dotX();
  await update(62, Array.from({ length: 60 }, (_, i) => i % 2 ? '100' : '0'));
  assert.ok(Math.abs(await dotX() - beforeInterrupt) < .5, 'a new sample continues from the visible scroll position');
  await animationTime(500);
  const relativeDotX = () => page.locator('#chart-dots circle[data-second="30"]').evaluate(el => {
    const dot = el.getBoundingClientRect(), chart = document.getElementById('chart').getBoundingClientRect();
    return (dot.x + dot.width / 2 - chart.x) / chart.width;
  });
  const relativeBefore = await relativeDotX();
  await page.setViewportSize({ width:390, height:1100 });
  assert.ok(Math.abs(await relativeDotX() - relativeBefore) < .001, 'responsive resize preserves the native animation position');
  await page.setViewportSize({ width:1280, height:720 });
  await animationTime(1000);
  const paints = await page.evaluate(() => {
    window.chartObserver.disconnect();
    return window.chartPaints;
  });
  assert.ok(paints >= 2 && paints <= 4, `successive scrolls avoid per-frame SVG updates: ${paints}`);
  assert.equal(await page.locator('#chart-values tr').count(), 60);
  await update(63, Array(60).fill('25'));
  const rescaling = await path();
  await page.clock.runFor(100);
  assert.notEqual(await path(), rescaling, 'changed values and scales still interpolate geometry');
  await page.clock.runFor(880);
  await update(64, Array(60).fill('25'));
  assert.equal(await page.locator('#chart-motion').evaluate(el => el.getAnimations().length), 1,
    'a sample just before the last scale frame returns to native scrolling');
  await page.locator('#freeze').click();
  assert.equal(await page.locator('#chart-motion').evaluate(el => el.getAnimations().length), 0);
  const frozen = await path();
  await page.clock.runFor(1200);
  assert.equal(await path(), frozen, 'freeze cancels ongoing motion');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await update(65, Array(60).fill('0'));
  const reduced = await path();
  await page.clock.runFor(500);
  assert.equal(await path(), reduced, 'reduced motion renders immediately');
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await update(66, ['2', '8', '1'], 'new-session');
  const reset = await path();
  await page.clock.runFor(500);
  assert.equal(await path(), reset, 'session resets do not interpolate unrelated history');
  await update(67, ['8', '1', '4'], 'new-session', 'complete');
  const final = await path();
  await page.clock.runFor(1200);
  assert.equal(await path(), final, 'final metrics render immediately');
});

test('interrupted chart scrolling keeps offscreen geometry bounded', async t => {
  const server = await monitor();
  t.after(() => server.close());
  const browser = await chromium.launch();
  t.after(() => browser.close());
  const page = await browser.newPage();
  await page.route('**/api/events', route => route.fulfill({ contentType: 'text/event-stream', body: `event: snapshot\ndata: ${JSON.stringify(fixture)}\n\n` }));
  await page.goto(server.url);
  await page.waitForFunction(() => document.getElementById('input-state').textContent === 'complete');
  const result = await page.evaluate(async fixture => {
    const { render } = await import('/render.mjs');
    const frame = structuredClone(fixture);
    Object.assign(frame.metrics, { history: Array(60).fill('1'), session_id: 'bounded', state: 'reading' });
    let maxSegments = 0;
    for (let end = 60; end < 100; end++) {
      frame.metrics.elapsed_seconds = end;
      render(frame);
      const animations = document.getElementById('chart-motion').getAnimations();
      if (end > 60 && animations.length !== 1) throw new Error('expected native scrolling');
      if (animations.length) {
        animations[0].pause();
        animations[0].currentTime = 900;
      }
      maxSegments = Math.max(maxSegments, (document.getElementById('chart-line').getAttribute('d').match(/ C/g) || []).length);
    }
    return { maxSegments, dots: document.querySelectorAll('#chart-dots circle').length };
  }, fixture);
  assert.ok(result.maxSegments <= 61, `at most two outgoing vertices are retained: ${result.maxSegments} segments`);
  assert.equal(result.dots, 60);
});
