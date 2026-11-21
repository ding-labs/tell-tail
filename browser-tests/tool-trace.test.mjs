import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { chromium, monitorMcp, monitorDesktop } from './helpers.mjs';
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
  await post([record('PRIVATE_A','false'),record('PRIVATE_B','false','mcp__codex_apps__google_drive')]);
  await page.waitForFunction(async()=> (await(await fetch('/api/traces/snapshot')).json()).snapshot.retained_calls===2);
  assert.equal(await page.locator('#trace-rows tr').count(),1);assert.match(await page.locator('#trace-rows').textContent(),/Execution succeeded/);
  await page.locator('#trace-rows button').first().click();await page.waitForFunction(()=>document.getElementById('trace-detail-note').textContent==='Execution succeeded');
  await page.locator('#trace-copy').click();const copied=await page.evaluate(()=>navigator.clipboard.readText());assert.doesNotMatch(copied,/PRIVATE_|x-tell-tail-probe-key/);assert.match(copied,/railway/);await page.keyboard.press('Escape');
  await page.locator('#trace-q').fill('railway');await page.waitForTimeout(300);assert.equal(await page.locator('#trace-rows tr').count(),1);
  await page.locator('#trace-clear').click();assert.equal(await page.locator('#pause').getAttribute('aria-pressed'),'true');
  await page.locator('#pause').click();await page.waitForFunction(()=>document.querySelectorAll('#trace-rows tr').length===2);assert.match(await page.locator('#trace-rows').textContent(),/conflict/);
  await page.locator('#trace-outcome').selectOption('failed');await page.waitForFunction(()=>document.querySelectorAll('#trace-rows tr').length===1);assert.match(await page.locator('#trace-rows').textContent(),/google_drive/);
  assert.deepEqual(await page.locator('#trace-plugin-options option').evaluateAll(nodes=>nodes.map(n=>n.value)), ['google_drive','railway']);
  await page.locator('#trace-plugin').fill('google_drive');
  await page.waitForTimeout(300);
  assert.equal(await page.locator('#trace-rows tr').count(),1);
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

test('Activity resolves exact chat titles and reveals private command input only on request', async t => {
  const dir = await mkdtemp(join(tmpdir(), 'tell-tail-inspection-'));
  t.after(() => rm(dir, {recursive:true, force:true}));
  const key = join(dir, 'key'), config = join(dir, 'otel.toml'), index = join(dir, 'index.jsonl');
  await writeFile(index, '{"id":"thread-A","thread_name":"Release checklist"}\n{"id":"thread-B","thread_name":"Second chat"}\n');
  const server = await monitorDesktop(['--source','codex-desktop','--log-dir',dir,'--trace-port','0','--trace-token-file',key,'--trace-config-out',config,'--inspect-chat-index',index]);
  t.after(() => server.close());
  const endpoint = (await readFile(config, 'utf8')).match(/endpoint = "([^"]+)"/)[1];
  const records = ['thread-A','thread-B'].map(thread => Buffer.concat(Object.entries({
    'event.name':'codex.tool_result','conversation.id':thread,call_id:'same-call',tool_namespace:'functions',
    tool_name:'exec_command',success:'true',arguments:'{"cmd":"git diff PRIVATE_FILE"}',
    output:'Chunk ID: abc\nWall time: 0.1 seconds\nProcess exited with code 7\nOutput:\nPRIVATE_OUTPUT',
  }).map(([k,v])=>attr(k,v))));
  assert.equal((await fetch(endpoint,{method:'POST',headers:{'content-type':'application/x-protobuf','x-tell-tail-probe-key':(await readFile(key,'utf8')).trim()},body:batch(records)})).status,200);
  const browser = await chromium.launch();
  t.after(() => browser.close());
  const page = await browser.newPage();
  await page.context().grantPermissions(['clipboard-read','clipboard-write'],{origin:server.url});
  await page.goto(server.url);
  const row = page.locator('.event-row').filter({hasText:'Release checklist'});
  await row.waitFor();
  assert.match(await row.textContent(),/git diff.*Exit 7/s);
  assert.doesNotMatch(await page.locator('body').textContent(),/PRIVATE_/);
  await row.click();
  await page.getByText('Show shell command',{exact:true}).click();
  await page.getByText('git diff PRIVATE_FILE',{exact:true}).waitFor();
  const id = await row.getAttribute('data-id');
  assert.equal((await fetch(server.url+'api/activity/item/'+id+'?input=1')).status,403);
  await page.locator('#copy').click();
  assert.doesNotMatch(await page.evaluate(()=>navigator.clipboard.readText()),/Release checklist|PRIVATE_|thread-A/);
  await page.getByRole('button',{name:'Show calls in this chat'}).click();
  await page.waitForFunction(()=>document.querySelectorAll('.event-row').length===1);
  assert.match(await page.locator('.event-row').textContent(),/Release checklist/);
});

test('selecting a wrapper shows its own request, nested manifest and turn failures', async t => {
  const dir = await mkdtemp(join(tmpdir(),'tell-tail-turn-'));
  t.after(()=>rm(dir,{recursive:true,force:true}));
  const chat='01900000-0000-7000-b000-000000000001', key=join(dir,'key');
  const index=join(dir,'index.jsonl'), config=join(dir,'otel.toml');
  await writeFile(index,JSON.stringify({id:chat,thread_name:'Build repair'}));
  await mkdir(join(dir,'archived_sessions'));
  const event=(turn,item)=>({type:'event_msg',payload:{type:'item_completed',thread_id:chat,turn_id:turn,item}});
  const message=text=>({type:'UserMessage',content:[{type:'text',text}]});
  const metadata={turn_id:'wanted'};
  const records=[
    {type:'session_meta',payload:{id:chat}},event('other',message('OTHER_PRIVATE_REQUEST')),
    event('wanted', message('Why did cargo test exit early?')),
    {type:'response_item',payload:{type:'custom_tool_call',call_id:'wrapper',name:'exec',internal_chat_message_metadata_passthrough:metadata}},
    event('wanted',{type:'CommandExecution',id:'command',command:['sh','-c','cargo test --lib'],exit_code:101,status:'failed',aggregated_output:'test parse_request failed'}),
    {type:'response_item',payload:{type:'custom_tool_call_output',call_id:'wrapper',internal_chat_message_metadata_passthrough:{
      ...metadata,executed_tool_calls:[{name:'functions.exec_command'}],tool_calls_complete:true}}},
  ];
  await writeFile(join(dir,'archived_sessions','rollout-test-'+chat+'.jsonl'),records.map(r=>JSON.stringify(r)).join('\n'));
  const server=await monitorDesktop(['--source','codex-desktop','--log-dir',dir,'--trace-port','0','--trace-token-file',key,'--trace-config-out',config,'--inspect-chat-index',index]);
  t.after(()=>server.close());
  const url=(await readFile(config,'utf8')).match(/endpoint = "([^"]+)"/)[1];
  const fields={'event.name':'codex.tool_result','conversation.id':chat,call_id:'wrapper',tool_namespace:'functions',tool_name:'exec',success:'true'};
  const result=await fetch(url,{method:'POST',headers:{'content-type':'application/x-protobuf','x-tell-tail-probe-key':(await readFile(key,'utf8')).trim()},
    body:batch([Buffer.concat(Object.entries(fields).map(([k,v])=>attr(k,v)))])});
  assert.equal(result.status,200);
  const browser=await chromium.launch(); t.after(()=>browser.close());
  const page=await browser.newPage();
  await page.context().grantPermissions(['clipboard-read','clipboard-write'],{origin:server.url});
  await page.goto(server.url);
  await page.locator('.event-row').filter({hasText:'functions.exec'}).first().click();
  await page.getByText('Why did cargo test exit early?',{exact:true}).waitFor();
  const panel=page.locator('.turn-panel');
  assert.match(await panel.innerText(),/Tools inside this call\s+functions.exec_command/);
  assert.match(await panel.innerText(),/Failures in this turn\s+cargo test --lib\s+functions.exec_command · Exit 101/);
  assert.doesNotMatch(await panel.innerText(),/OTHER_PRIVATE/);
  await panel.getByText('Failure output').first().click();
  await panel.getByText('test parse_request failed',{exact:true}).first().waitFor();
  await panel.getByText('All tools in this turn (2)').click();
  await panel.getByText('Selected call · Completed').waitFor();
  const paths=[server.url+'api/traces/call/1/turn?source=stale',server.url+'api/traces/calls'];
  assert.equal((await fetch(paths[0])).status,403);
  assert.equal((await fetch(paths[0],{headers:{'x-tell-tail-inspect':'local-turn'}})).status,410);
  assert.doesNotMatch(await(await fetch(paths[1])).text(),/exit early|01900000|OTHER_PRIVATE/);
  await page.locator('#copy').click();
  assert.doesNotMatch(await page.evaluate(()=>navigator.clipboard.readText()),/exit early|cargo test|OTHER_PRIVATE/);
});
