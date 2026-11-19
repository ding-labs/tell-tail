import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { chromium, monitorMcp } from './helpers.mjs';
const vi = n => { const out=[]; while(n>127){out.push((n&127)|128); n=Math.floor(n/128);} out.push(n); return Buffer.from(out); };
const msg = (n,b) => Buffer.concat([vi(n*8+2),vi(b.length),b]);
const attr = (k,v) => msg(6,Buffer.concat([msg(1,Buffer.from(k)),msg(2,msg(1,Buffer.from(v)))]));
const record = (id, success='true', ns='mcp__codex_apps__railway') => Buffer.concat(Object.entries({'event.name':'codex.tool_result',tool_name:'_search_docs',tool_namespace:ns,mcp_server:'codex_apps','conversation.id':'PRIVATE_CHAT',call_id:id,duration_ms:'123',success,arguments:'PRIVATE_ARGUMENTS',output:'PRIVATE_BALANCE'}).map(([k,v])=>attr(k,v)));
const batch = records => msg(1,msg(2,Buffer.concat(records.map(r=>msg(2,r)))));

test('tool calls flow through authenticated OTLP, freeze outcomes, filter, paginate and fit mobile', async t => {
  const dir=await mkdtemp(join(tmpdir(),'tell-tail-traces-')); t.after(()=>rm(dir,{recursive:true,force:true}));
  const config=join(dir,'otel.toml'),key=join(dir,'key');
  const server=await monitorMcp(['--source','codex-desktop','--log-dir',dir,'--trace-port','0','--trace-token-file',key,'--trace-config-out',config]);t.after(()=>server.close());
  const fragment=await readFile(config,'utf8'); const endpoint=fragment.match(/endpoint = "([^"]+)"/)[1]; const credential=(await readFile(key,'utf8')).trim();
  const post=async records=>{const r=await fetch(endpoint,{method:'POST',headers:{'Content-Type':'application/x-protobuf','x-tell-tail-probe-key':credential},body:batch(records)});assert.equal(r.status,200);};
  const browser=await chromium.launch();t.after(()=>browser.close());const page=await browser.newPage();
  await page.context().grantPermissions(['clipboard-read','clipboard-write'],{origin:server.url});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(server.url);await page.waitForFunction(()=>document.getElementById('traces-tab').getAttribute('aria-selected')==='true');
  assert.match(await page.locator('#trace-status').textContent(),/Waiting/);
  await post([record('PRIVATE_A')]);await post([record('PRIVATE_A')]);
  await page.waitForFunction(()=>document.querySelectorAll('#trace-rows tr').length===1);
  assert.match(await page.locator('#trace-rows').textContent(),/railway/);assert.match(await page.locator('#trace-rows').textContent(),/Execution succeeded/);
  assert.equal(await page.locator('#traces-tab-count').textContent(),'1');
  await page.locator('#pause').click();
  await post([record('PRIVATE_A','false'),record('PRIVATE_B','false','mcp__other')]);
  await page.waitForFunction(async()=> (await(await fetch('/api/traces/snapshot')).json()).snapshot.retained_calls===2);
  assert.equal(await page.locator('#trace-rows tr').count(),1);assert.match(await page.locator('#trace-rows').textContent(),/Execution succeeded/);
  await page.locator('#trace-rows button').first().click();await page.waitForFunction(()=>document.getElementById('trace-detail-note').textContent==='Execution succeeded');
  await page.locator('#trace-copy').click();const copied=await page.evaluate(()=>navigator.clipboard.readText());assert.doesNotMatch(copied,/PRIVATE_|x-tell-tail-probe-key/);assert.match(copied,/railway/);await page.keyboard.press('Escape');
  await page.locator('#trace-q').fill('railway');await page.waitForTimeout(300);assert.equal(await page.locator('#trace-rows tr').count(),1);
  await page.locator('#trace-clear').click();assert.equal(await page.locator('#pause').getAttribute('aria-pressed'),'true');
  await page.locator('#pause').click();await page.waitForFunction(()=>document.querySelectorAll('#trace-rows tr').length===2);assert.match(await page.locator('#trace-rows').textContent(),/conflict/);
  await page.locator('#trace-outcome').selectOption('failed');await page.waitForFunction(()=>document.querySelectorAll('#trace-rows tr').length===1);assert.match(await page.locator('#trace-rows').textContent(),/Unidentified plugin/);
  await page.locator('#trace-clear').click();
  await post(Array.from({length:130},(_,i)=>record(`PRIVATE_${i}`)));
  await page.waitForFunction(()=>document.querySelectorAll('#trace-rows tr').length===100);
  await page.locator('#trace-older').click();await page.waitForFunction(()=>document.querySelectorAll('#trace-rows tr').length===32);
  assert.equal(await page.locator('#pause').getAttribute('aria-pressed'),'true');
  await page.locator('#trace-latest').click();await page.waitForFunction(()=>document.querySelectorAll('#trace-rows tr').length===100);
  for(const width of [1440,390,320]){await page.setViewportSize({width,height:900});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true,`overflow ${width}`);}
  assert.doesNotMatch(await page.locator('body').textContent(),/PRIVATE_/);
  assert.deepEqual(errors,[]);
});

test('offline trace replay opens the tool workspace without a live receiver or client logs', async t => {
  const server=await monitorMcp(['--trace-replay','tests/fixtures/tool-trace/lifecycle.ndjson']);t.after(()=>server.close());
  const browser=await chromium.launch();t.after(()=>browser.close());const page=await browser.newPage();
  await page.goto(server.url);await page.waitForFunction(()=>document.querySelectorAll('#trace-rows tr').length===2);
  assert.match(await page.locator('.local-label').textContent(),/SYNTHETIC/);
  assert.match(await page.locator('#trace-status').textContent(),/Synthetic replay complete/);
  assert.match(await page.locator('#trace-rows').textContent(),/Imported replay/);
  assert.doesNotMatch(await page.locator('body').textContent(),/PRIVATE_/);
  const mcp=await(await fetch(server.url+'api/mcp/snapshot')).json();assert.equal(mcp.sources.length,0);
  const trace=await(await fetch(server.url+'api/traces/snapshot')).json();assert.equal(trace.mode,'synthetic_replay');assert.equal(trace.adapter,'normalized_fixture_v1');assert.equal(trace.capabilities.live_capture_verified,false);assert.equal(trace.accepted_batches,'0');
});
