import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,appendFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {chromium,monitor,monitorDesktop} from './helpers.mjs';

test('desktop workspaces share real input, branding and appearance without exposing raw bodies',async t=>{
 const dir=await mkdtemp(join(tmpdir(),'tell-tail-workspaces-'));t.after(()=>rm(dir,{recursive:true,force:true}));
 const file=join(dir,'desktop.log');await writeFile(file,'2026-11-19T12:00:00Z info [electron-message-handler] chatgpt_pubsub_transport_opened\nERROR PRIVATE_BODY\n');
 const server=await monitorDesktop(['--source','codex-desktop','--log-dir',dir]);t.after(()=>server.close());
 const browser=await chromium.launch();t.after(()=>browser.close());
 const page=await browser.newPage({viewport:{width:1280,height:800}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(server.url);await page.waitForFunction(()=>document.querySelector('.event-row'));
 assert.equal(await page.locator('.tt-workspaces [aria-current]').textContent(),'AI Activity');
 await page.locator('#theme').click();assert.equal(await page.locator('html').getAttribute('data-theme'),'dark');
 await page.getByRole('link',{name:'Logs',exact:true}).click();
 await page.waitForFunction(()=>document.getElementById('total-records').textContent==='2');
 assert.equal(await page.locator('.tt-workspaces [aria-current]').textContent(),'Logs');
 assert.equal(await page.locator('h1').textContent(),'Logs');
 assert.equal(await page.locator('html').getAttribute('data-theme'),'dark');
 assert.equal(await page.locator('#recent-title').isVisible(),false);
 assert.doesNotMatch(await page.locator('body').textContent(),/PRIVATE_BODY/);
 await page.locator('#theme').click();
 await page.getByRole('link',{name:'AI Activity',exact:true}).click();
 await page.waitForFunction(()=>document.querySelector('.event-row'));
 assert.equal(await page.locator('html').getAttribute('data-theme'),'light');
 await appendFile(file,'WARN another observation\n');
 await page.getByRole('link',{name:'Logs',exact:true}).click();
 await page.waitForFunction(()=>document.getElementById('total-records').textContent==='3');
 for(const width of [1440,1280,1024,768,390,320]){
  await page.setViewportSize({width,height:800});
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true,`logs overflow at ${width}`);
  assert.equal(await page.getByRole('link',{name:'AI Activity',exact:true}).isVisible(),true);
 }
 await page.goto(server.url+'mcp');await page.waitForFunction(()=>Number(document.getElementById('source-count').textContent)>0);
 assert.equal(await page.getByRole('link',{name:'Logs',exact:true}).isVisible(),true);
 assert.deepEqual(errors,[]);
});

test('file and stdin monitoring keep the original dashboard first and collection continues across workspace navigation',async t=>{
 const server=await monitor();t.after(()=>server.close());server.child.stdin.write('INFO ready\nERROR Cache unavailable\n');
 const browser=await chromium.launch();t.after(()=>browser.close());const page=await browser.newPage({viewport:{width:1280,height:800}});
 const activityRequests=[];page.on('request',r=>{if(r.url().includes('/api/activity/'))activityRequests.push(r.url());});
 await page.goto(server.url);await page.waitForFunction(()=>document.getElementById('total-records').textContent==='2');
 assert.equal(await page.locator('.tt-workspaces [aria-current]').textContent(),'Logs');
 assert.match(await page.locator('#recent-errors').textContent(),/Cache unavailable/);
 await page.getByRole('link',{name:'AI Activity',exact:true}).click();
 assert.match(await page.locator('main').textContent(),/This session is monitoring a log stream/);
 assert.match(await page.locator('main').textContent(),/tell-tail desktop --open/);
 server.child.stdin.write('WARN retry\n');
 await page.getByRole('link',{name:'Logs',exact:true}).click();
 await page.waitForFunction(()=>document.getElementById('total-records').textContent==='3');
 assert.equal(await page.locator('#recent-title').isVisible(),true);
 assert.deepEqual(activityRequests,[]);
});


test('appearance still changes when browser storage is unavailable',async t=>{
 const server=await monitor(['--demo']);t.after(()=>server.close());
 const browser=await chromium.launch();t.after(()=>browser.close());const page=await browser.newPage();
 await page.addInitScript(()=>{Storage.prototype.getItem=()=>{throw new Error('unavailable');};Storage.prototype.setItem=()=>{throw new Error('unavailable');};});
 await page.goto(server.url);await page.locator('#theme').click();
 assert.equal(await page.locator('html').getAttribute('data-theme'),'dark');
 await page.locator('#theme').click();assert.equal(await page.locator('html').getAttribute('data-theme'),'light');
});


test('restoring AI Activity from the browser cache reconnects without replacing paused evidence',async t=>{
 const dir=await mkdtemp(join(tmpdir(),'tell-tail-restore-'));t.after(()=>rm(dir,{recursive:true,force:true}));
 const file=join(dir,'desktop.log');const line='2026-11-19T12:00:00Z info [electron-message-handler] chatgpt_pubsub_transport_opened\n';await writeFile(file,line);
 const server=await monitorDesktop(['--source','codex-desktop','--log-dir',dir]);t.after(()=>server.close());
 const browser=await chromium.launch();t.after(()=>browser.close());const page=await browser.newPage();
 await page.goto(server.url+'activity');await page.waitForFunction(()=>document.querySelectorAll('.event-row').length===1);
 await page.locator('#pause').click();
 await page.evaluate(()=>window.dispatchEvent(new PageTransitionEvent('pagehide',{persisted:true})));
 await appendFile(file,line);
 const end=Date.now()+5000;while((await(await fetch(server.url+'api/activity/snapshot')).json()).retained_items<2){assert.ok(Date.now()<end);await new Promise(r=>setTimeout(r,20));}
 await page.evaluate(()=>window.dispatchEvent(new PageTransitionEvent('pageshow',{persisted:true})));
 await page.waitForFunction(()=>document.getElementById('connection').textContent==='Connected locally');
 await page.waitForFunction(()=>document.getElementById('pending').textContent.includes('Updates available'));
 assert.equal(await page.locator('.event-row').count(),1);
 await page.locator('#pending').click();await page.waitForFunction(()=>document.querySelectorAll('.event-row').length===2);
});
