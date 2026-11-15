import copy
import json
import os
from pathlib import Path
import statistics
import subprocess
import time

ROOT=Path(__file__).parent
OUT=ROOT/'comparison-bulk-150000eps'
OUT.mkdir(exist_ok=True)
K=['kubectl','--context','kind-vrl-kind-benchmark','--request-timeout=20s']
NS='vrl-kind-ab'
NODE='vrl-kind-benchmark-control-plane'

def cmd(args):
    return subprocess.check_output(args,text=True)

def kub(args):
    return cmd(K+args)

def apply(objects,path):
    path.write_text(json.dumps(dict(apiVersion='v1',kind='List',items=objects),indent=2))
    print(kub(['apply','--server-side','-f',str(path)]),flush=True)

def cpu_stat(pid):
    script="import pathlib,json; p=pathlib.Path('/proc/%s/cgroup').read_text().strip().split('::')[1]; print(json.dumps(dict(l.split() for l in (pathlib.Path('/sys/fs/cgroup'+p)/'cpu.stat').read_text().splitlines())))"%pid
    return {k:int(v) for k,v in json.loads(cmd(['podman','exec',NODE,'python3','-c',script])).items()}

def scrape(directory,index,pid):
    text=kub(['get','--raw',f'/api/v1/namespaces/{NS}/services/vector:9598/proxy/metrics'])
    (directory/f'metrics-{index:03}.prom').write_text(text)
    n=next(float(l.split('} ')[1].split()[0]) for l in text.splitlines() if l.startswith('vector_component_sent_events_total{') and 'component_id="discard"' in l)
    return dict(time=time.monotonic(),events=n,cpu=cpu_stat(pid))

def generator_summary(i):
    state=json.loads(kub(['-n',NS,'get','pod',f'generator-{i}','-o','json']))
    uid=state['metadata']['uid']
    script='''import pathlib,gzip,json
root=pathlib.Path(%r)
found=[]
for p in root.glob('*'):
    if not p.is_file(): continue
    opener=gzip.open if p.suffix=='.gz' else open
    with opener(p,'rt') as f:
        for line in f:
            if '"summary": true' in line:
                found.append(json.loads(line.split(' ',3)[3]))
assert found, 'Generator summary absent from retained CRI logs'
print(json.dumps(found[-1]))
'''%f'/var/log/pods/{NS}_generator-{i}_{uid}/generator'
    return json.loads(cmd(['podman','exec',NODE,'python3','-c',script]))

original=json.loads((ROOT/'collectors.json').read_text())['items']
pod=copy.deepcopy(next(o for o in original if o['kind']=='Pod' and o['metadata']['name']=='vector-kubernetes'))
pod['metadata'].update(name='vector',namespace=NS,labels={'app':'vector'})
pod['spec']['containers'][0]['imagePullPolicy']='Never'
pod['spec']['volumes'][0]['configMap']['name']='vector'
setup=[dict(apiVersion='v1',kind='Namespace',metadata=dict(name=NS)),dict(apiVersion='v1',kind='ServiceAccount',metadata=dict(name='vector',namespace=NS)),dict(apiVersion='rbac.authorization.k8s.io/v1',kind='ClusterRoleBinding',metadata=dict(name='vrl-kind-ab-reader'),roleRef=dict(apiGroup='rbac.authorization.k8s.io',kind='ClusterRole',name='vrl-kind-repro-reader'),subjects=[dict(kind='ServiceAccount',name='vector',namespace=NS)]),dict(apiVersion='v1',kind='Service',metadata=dict(name='vector',namespace=NS),spec=dict(selector={'app':'vector'},ports=[dict(port=9598,targetPort=9598)]))]
apply(setup,OUT/'setup.json')
generators=json.loads((ROOT/'generators.json').read_text())['items']
for o in generators:
    o['metadata']['namespace']=NS
    if o['kind']=='ConfigMap':
        o['data']['generate.py']=o['data']['generate.py'].replace('< 180:','< 120:')
    else:
        o['metadata']['labels']['benchmark']='vrl-kind-ab'
        o['spec']['activeDeadlineSeconds']=180

# Remove only collectors owned by this experiment to avoid competing for CPU.
print(kub(['-n','vrl-kind-repro','delete','pod','vector-kubernetes','vector-file','--ignore-not-found=true']),flush=True)
results=json.loads((OUT/'results.json').read_text()) if (OUT/'results.json').exists() else []
orders=[['baseline','bulk'],['bulk','baseline'],['baseline','bulk'],['bulk','baseline'],['baseline','bulk']]
for pair,order in enumerate(orders,1):
    for variant in order:
        run_id=f'p{pair:02}-{variant}'
        if any(r['pair']==pair and r['variant']==variant for r in results):
            print('PRESERVED',run_id,flush=True)
            continue
        directory=OUT/run_id
        directory.mkdir(exist_ok=False)
        print('START',run_id,time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),flush=True)
        print(kub(['-n',NS,'delete','pod','vector','generator-0','generator-1','--ignore-not-found=true']),flush=True)
        config='''data_dir: /vector-data
sources:
  logs:
    type: kubernetes_logs
    self_node_name: ${VECTOR_SELF_NODE_NAME}
    extra_label_selector: benchmark=vrl-kind-ab
    max_read_bytes: 20480
    glob_minimum_cooldown_ms: 1000
  vector_metrics:
    type: internal_metrics
    scrape_interval_secs: 1
sinks:
  discard:
    type: blackhole
    inputs: [logs]
  metrics:
    type: prometheus_exporter
    inputs: [vector_metrics]
    address: 0.0.0.0:9598
'''
        p=copy.deepcopy(pod)
        p['spec']['containers'][0]['image']=({'baseline': 'localhost/vector-vrl2044:baseline-ae18932a-minimal-musl', 'bulk': 'localhost/vector-k8s-bulk:ae18932a-minimal-musl'}[variant])
        cm=dict(apiVersion='v1',kind='ConfigMap',metadata=dict(name='vector',namespace=NS),data={'vector.yaml':config})
        apply([cm,p],directory/'collector.json')
        print(kub(['-n',NS,'wait','--for=condition=Ready','pod/vector','--timeout=90s']),flush=True)
        state=json.loads(kub(['-n',NS,'get','pod','vector','-o','json']))
        (directory/'collector-state.json').write_text(json.dumps(state,indent=2))
        cid=state['status']['containerStatuses'][0]['containerID'].split('://')[1]
        info=json.loads(cmd(['podman','exec',NODE,'crictl','inspect',cid]))
        pid=info['info']['pid']
        apply(generators,directory/'generators.json')
        print(kub(['-n',NS,'wait','--for=condition=Ready','pod/generator-0','pod/generator-1','--timeout=90s']),flush=True)
        # Generators sleep 20s before logging; then allow 30s of logging/cache warm-up.
        time.sleep(50)
        samples=[scrape(directory,0,pid)]
        for i in range(1,17):
            time.sleep(5)
            samples.append(scrape(directory,i,pid))
            (directory/'samples.json').write_text(json.dumps(samples,indent=2))
        a,b=samples[0],samples[-1]
        seconds=b['time']-a['time']
        result=dict(pair=pair,variant=variant,window_seconds=seconds,eps=(b['events']-a['events'])/seconds,cpu_cores=(b['cpu']['usage_usec']-a['cpu']['usage_usec'])/1e6/seconds,throttled_periods=b['cpu']['nr_throttled']-a['cpu']['nr_throttled'])
        print('MEASURED',json.dumps(result),flush=True)
        print(kub(['-n',NS,'wait','--for=jsonpath={.status.phase}=Succeeded','pod/generator-0','pod/generator-1','--timeout=60s']),flush=True)
        expected=0
        for i in range(2):
            summary=generator_summary(i)
            (directory/f'generator-{i}-summary.json').write_text(json.dumps(summary))
            expected+=summary['events']+1
            assert 74000<=summary['events']/summary['seconds']<=76000,summary
        drain=[]
        for i in range(37):
            value=scrape(directory,100+i,pid)
            drain.append(value)
            (directory/'drain.json').write_text(json.dumps(drain,indent=2))
            if value['events']>=expected: break
            if len(drain)>=7 and len({r['events'] for r in drain[-7:]})==1: break
            time.sleep(5)
        result.update(expected_events=expected,final_events=drain[-1]['events'],event_count_deficit=expected-drain[-1]['events'])
        (directory/'collector.log').write_text(kub(['-n',NS,'logs','vector']))
        (directory/'summary.json').write_text(json.dumps(result,indent=2))
        results.append(result)
        (OUT/'results.json').write_text(json.dumps(results,indent=2))
        print('DONE',json.dumps(result),flush=True)

pairs=[]
for pair in range(1,6):
    a=next(r for r in results if r['pair']==pair and r['variant']=='baseline')
    b=next(r for r in results if r['pair']==pair and r['variant']=='bulk')
    pairs.append(dict(pair=pair,throughput_change_pct=100*(b['eps']/a['eps']-1),cpu_per_event_change_pct=100*((b['cpu_cores']/b['eps'])/(a['cpu_cores']/a['eps'])-1)))
summary=dict(pairs=pairs,median_throughput_change_pct=statistics.median(p['throughput_change_pct'] for p in pairs),median_cpu_per_event_change_pct=statistics.median(p['cpu_per_event_change_pct'] for p in pairs))
(OUT/'paired-summary.json').write_text(json.dumps(summary,indent=2))
print('COMPLETE',json.dumps(summary,indent=2),flush=True)
