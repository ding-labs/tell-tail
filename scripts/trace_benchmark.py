#!/usr/bin/env python3
"""Finite synthetic live-receiver benchmark; never reads client logs or settings."""
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
        with (d/'server.log').open('w+') as log:
            proc = subprocess.Popen([str(args.binary.resolve()), 'mcp', '--output', 'web',
                                     '--source', 'codex-desktop', '--log-dir', str(d),
                                     '--trace-port', '0', '--trace-token-file', str(key),
                                     '--trace-config-out', str(config)], stdout=subprocess.DEVNULL, stderr=log)
            try:
                deadline = time.monotonic()+5
                while time.monotonic()<deadline:
                    log.seek(0)
                    match = re.search(r'MCP web (http://127\.0\.0\.1:\d+/)', log.read())
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
                            'event.name':'codex.tool_result', 'tool_name':'_search_docs',
                            'tool_namespace':'mcp__codex_apps__railway', 'call_id':f'synthetic-{n}',
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
                rss0, cpu0 = resource(); idle_begin=time.monotonic(); time.sleep(5)
                _, cpu1=resource(); idle_cpu=(cpu1-cpu0)/(time.monotonic()-idle_begin)*100
                rss=[rss0]; latencies=[]; start=time.monotonic()
                for n in range(600):
                    wait=start+n*.1-time.monotonic()
                    if wait>0: time.sleep(wait)
                    sent=time.monotonic(); post(n*10,10)
                    while int(get('/api/traces/snapshot')['snapshot']['counts']['distinct_calls']) < (n+1)*10:
                        assert time.monotonic()-sent<3
                        time.sleep(.005)
                    latencies.append((time.monotonic()-sent)*1000)
                    if n%10==0: rss.append(resource()[0])
                steady_seconds=time.monotonic()-start
                burst_start=time.monotonic()
                for n in range(6000,16000,500): post(n,500)
                deadline=time.monotonic()+5
                while True:
                    final=get('/api/traces/snapshot')
                    if int(final['snapshot']['counts']['distinct_calls'])==16000:break
                    assert time.monotonic()<deadline
                    time.sleep(.01)
                rss.append(resource()[0]); burst_seconds=time.monotonic()-burst_start
                assert final['snapshot']['retained_calls']<=2000
                assert final['snapshot']['charged_bytes']<=16*1024*1024
                shutdown=time.monotonic();proc.terminate();proc.wait(timeout=3)
                report={'schema_version':1,'mode':'synthetic_result_only','steady_calls':6000,
                        'steady_seconds':round(steady_seconds,3),'target_calls_per_second':100,
                        'burst_calls':10000,'burst_seconds':round(burst_seconds,3),
                        'receipt_to_query_p95_ms':round(statistics.quantiles(latencies,n=100)[94],3),
                        'render_latency_measured':False,'backpressure_retries':retries,'max_rss_kib':max(rss),'idle_cpu_percent':round(idle_cpu,3),
                        'shutdown_ms':round((time.monotonic()-shutdown)*1000,3),'final':final,
                        'extended_soaks':'not_run_optional_nonblocking'}
                args.output.write_text(json.dumps(report,indent=2)+'\n')
                print(json.dumps({k:v for k,v in report.items() if k!='final'}))
            finally:
                if proc.poll() is None:
                    proc.kill();proc.wait()


if __name__ == '__main__':
    main()
