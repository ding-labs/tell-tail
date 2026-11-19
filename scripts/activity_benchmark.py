#!/usr/bin/env python3
"""Finite unified activity benchmark: synthetic logs and results; no client data."""
import argparse
import http.client
import json
from pathlib import Path
import re
import statistics
import subprocess
import tempfile
import time
from urllib.parse import urlparse
from test_trace_probe_decode import attr, batch


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='tell-tail-trace-bench-') as folder:
        d = Path(folder)
        key, config = d/'key', d/'otel.toml'
        logs = d/'logs'
        logs.mkdir()
        # Exactly 16 MiB across 32 sources exercises the shared bootstrap budget.
        padding = b"unrecognized synthetic record\n"
        for i in range(32):
            (logs/f"history-{i}.log").write_bytes((padding*(524288//len(padding)+1))[:524288-1]+b"\n")
        active = logs/'activity.log'
        active.write_text('2026-11-19T12:00:00Z info [AppServerConnection] mcp_server_startup_status_updated server=quiet-demo status=ready threadId=synthetic hostId=demo\n')
        with (d/'server.log').open('w+') as log:
            proc = subprocess.Popen([str(args.binary.resolve()), 'desktop', '--output', 'web',
                                     '--source', 'codex-desktop', '--log-dir', str(logs),
                                     '--trace-port', '0', '--trace-token-file', str(key),
                                     '--trace-config-out', str(config)], stdout=subprocess.DEVNULL, stderr=log)
            try:
                deadline = time.monotonic()+5
                while time.monotonic()<deadline:
                    log.seek(0)
                    match = re.search(r'Desktop web (http://127\.0\.0\.1:\d+/)', log.read())
                    if match:
                        break
                    time.sleep(.02)
                assert match, 'collector startup timed out'
                web = urlparse(match[1])
                intake = urlparse(re.search(r'endpoint = "([^"]+)"', config.read_text())[1])
                credential = key.read_text().strip()
                def get(path):
                    c = http.client.HTTPConnection(web.hostname, web.port, timeout=3)
                    try:
                        c.request('GET', path)
                        r = c.getresponse()
                        assert r.status == 200
                        return json.loads(r.read())
                    finally:
                        c.close()
                retries = 0
                def post(first, count):
                    nonlocal retries
                    records = []
                    for n in range(first, first+count):
                        records.append(b''.join(attr(k,v) for k,v in {
                            'event.name':'codex.tool_result', 'tool_name':f'search_{n}',
                            'tool_namespace':'mcp__codex_apps__synthetic', 'call_id':f'synthetic-{n}',
                            'conversation.id':'synthetic-benchmark', 'duration_ms':'12', 'success':'true'
                        }.items()))
                    payload = batch(*records)
                    deadline = time.monotonic()+5
                    while True:
                        c = http.client.HTTPConnection(intake.hostname, intake.port, timeout=3)
                        try:
                            c.request('POST', '/v1/logs', payload, {'Content-Type':'application/x-protobuf',
                                      'x-tell-tail-probe-key':credential})
                            r=c.getresponse(); status=r.status; r.read()
                        finally:
                            c.close()
                        if status == 200: break
                        assert status == 503 and time.monotonic()<deadline, f'HTTP {status}'
                        retries += 1
                        time.sleep(.02)
                def resource():
                    fields=subprocess.check_output(['ps','-p',str(proc.pid),'-o','rss=,time='],text=True).split()
                    minutes,seconds=fields[1].split(':')
                    return int(fields[0]),int(minutes)*60+float(seconds)
                bootstrap = time.monotonic()
                while True:
                    boot = get('/api/activity/snapshot')
                    if len(boot['sources']['files']) == 33 and all(int(f['backlog_bytes']) == 0 for f in boot['sources']['files']): break
                    assert time.monotonic()-bootstrap < 15
                    time.sleep(.05)
                bootstrap_ms = (time.monotonic()-bootstrap)*1000
                rss0, cpu0 = resource(); idle_begin=time.monotonic(); time.sleep(5)
                _, cpu1=resource(); idle_cpu=(cpu1-cpu0)/(time.monotonic()-idle_begin)*100
                rss=[rss0]; latencies=[]; start=time.monotonic()
                for n in range(600):
                    wait=start+n*.1-time.monotonic()
                    if wait>0: time.sleep(wait)
                    sent=time.monotonic(); post(n*8,8)
                    with active.open('a') as out:
                        out.write('2026-11-19T12:00:01Z info [electron-message-handler] chatgpt_pubsub_transport_opened\n')
                        out.write('2026-11-19T12:00:02Z info [AppServerConnection] response_routed method=thread/read durationMs=12\n')
                    while get(f'/api/activity/items?q=search_{n*8+7}')['matched'] == 0:
                        assert time.monotonic()-sent<3
                        time.sleep(.005)
                    latencies.append((time.monotonic()-sent)*1000)
                    if n%10==0: rss.append(resource()[0])
                steady_seconds=time.monotonic()-start
                burst_start=time.monotonic()
                with active.open('a') as out:
                    out.write('2026-11-19T12:00:03Z info [electron-message-handler] chatgpt_pubsub_transport_opened\n'*10000)
                deadline=time.monotonic()+15
                while True:
                    final=get('/api/activity/snapshot')
                    if sum(int(f['records']) for f in final['sources']['files']) >= sum(int(f['records']) for f in boot['sources']['files'])+1200+10000:break
                    assert time.monotonic()<deadline
                    time.sleep(.01)
                rss.append(resource()[0]); burst_seconds=time.monotonic()-burst_start
                assert final['retained_items']<=4000
                assert final['charged_bytes']<=24*1024*1024
                quiet = get('/api/activity/items?q=quiet-demo')
                assert quiet['matched'] == 1, 'noisy connections evicted quiet MCP evidence'
                assert get('/api/activity/items?category=tools')['matched'] == 1000
                assert int(final['sources']['projection_gaps']) == 0
                trace = get('/api/traces/snapshot')
                assert int(trace['snapshot']['counts']['distinct_calls']) == 4800
                shutdown=time.monotonic();proc.terminate();proc.wait(timeout=3)
                report={'schema_version':1,'mode':'synthetic_desktop_activity','steady_observations':6000,'steady_tool_results':4800,'steady_log_records':1200,
                        'steady_seconds':round(steady_seconds,3),'target_observations_per_second':100,'bootstrap_files':33,'bootstrap_bytes':16*1024*1024,'bootstrap_drain_ms':round(bootstrap_ms,3),'quiet_mcp_retained':True,
                        'burst_connection_records':10000,'burst_seconds':round(burst_seconds,3),
                        'post_to_activity_query_p95_ms':round(statistics.quantiles(latencies,n=100)[94],3),
                        'render_latency_measured':False,'backpressure_retries':retries,'max_rss_kib':max(rss),'idle_cpu_percent':round(idle_cpu,3),
                        'shutdown_ms':round((time.monotonic()-shutdown)*1000,3),'final':final,'trace_retained_calls':trace['snapshot']['retained_calls'],
                        'extended_soaks':'not_run_optional_nonblocking'}
                args.output.write_text(json.dumps(report,indent=2)+'\n')
                print(json.dumps({k:v for k,v in report.items() if k!='final'}))
            finally:
                if proc.poll() is None:
                    proc.kill();proc.wait()


if __name__ == '__main__':
    main()
