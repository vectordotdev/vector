import copy
import json
from pathlib import Path
import re
import subprocess
import time

ROOT=Path(__file__).parent
# Reuse read-only helpers without executing the comparison's deployment loop.
helpers={'__file__':str(ROOT/'compare.py')}
exec((ROOT/'compare.py').read_text().split('original=json.loads')[0],helpers)
kub,cmd,apply,scrape,cpu_stat,generator_summary=(helpers[k] for k in ['kub','cmd','apply','scrape','cpu_stat','generator_summary'])
NODE=helpers['NODE']
NS=helpers['NS']
OUT=ROOT/'profiles-bulk-150000eps'
OUT.mkdir(exist_ok=True)
P=['podman','exec',NODE]
cmd(P+['mkdir','-p','/var/tmp/k8s-bulk-profile'])
results=[]
for index,variant in enumerate(['baseline','bulk','bulk','baseline'],1):
    directory=OUT/f'{index:02}-{variant}'
    directory.mkdir(exist_ok=False)
    print('START',index,variant,time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),flush=True)
    print(kub(['-n',NS,'delete','pod','vector','generator-0','generator-1','--ignore-not-found=true']),flush=True)
    collectors=json.loads((ROOT/f'comparison-bulk-150000eps/p01-{variant}/collector.json').read_text())['items']
    generators=json.loads((ROOT/'comparison-bulk-150000eps/p01-baseline/generators.json').read_text())['items']
    apply(collectors,directory/'collector.json')
    print(kub(['-n',NS,'wait','--for=condition=Ready','pod/vector','--timeout=90s']),flush=True)
    state=json.loads(kub(['-n',NS,'get','pod','vector','-o','json']))
    (directory/'collector-state.json').write_text(json.dumps(state,indent=2))
    cid=state['status']['containerStatuses'][0]['containerID'].split('://')[1]
    pid=json.loads(cmd(P+['crictl','inspect',cid]))['info']['pid']
    (directory/'elf-notes.txt').write_text(cmd(P+['readelf','-n',f'/proc/{pid}/exe']))
    (directory/'binary-sha256.txt').write_text(cmd(P+['sha256sum',f'/proc/{pid}/exe']))
    with (directory/'symbol-cache.log').open('w') as log:
        subprocess.run(P+['perf','buildid-cache','--add',f'/proc/{pid}/exe'],stdout=log,stderr=log,check=True)
    apply(generators,directory/'generators.json')
    print(kub(['-n',NS,'wait','--for=condition=Ready','pod/generator-0','pod/generator-1','--timeout=90s']),flush=True)
    time.sleep(50)
    before=scrape(directory,0,pid)
    remote=f'/var/tmp/k8s-bulk-profile/{index:02}-{variant}.data'
    command=P+['perf','record','-e','cpu-clock:u','-F','99','--call-graph','dwarf,8192','-p',str(pid),'-o',remote,'--','sleep','60']
    (directory/'perf-command.json').write_text(json.dumps(command))
    with (directory/'record.log').open('w') as log:
        subprocess.run(command,stdout=log,stderr=log,check=True)
    after=scrape(directory,1,pid)
    seconds=after['time']-before['time']
    events=after['events']-before['events']
    cpu_seconds=(after['cpu']['usage_usec']-before['cpu']['usage_usec'])/1e6
    (directory/'window.json').write_text(json.dumps(dict(before=before,after=after),indent=2))
    for label,flags in [('self',['--no-children']),('inclusive',['--children'])]:
        with (directory/f'{label}.txt').open('w') as out,(directory/f'{label}-warnings.txt').open('w') as err:
            subprocess.run(P+['perf','report','--stdio','--percent-limit','0.1','--sort','symbol','-g','none',*flags,'-i',remote],stdout=out,stderr=err,check=True)
    with (directory/'stacks.txt').open('w') as out,(directory/'unwind-warnings.txt').open('w') as err:
        subprocess.run(P+['perf','script','-i',remote],stdout=out,stderr=err,check=True)
    subprocess.run(['podman','cp',f'{NODE}:{remote}',str(directory/'perf.data')],check=True)
    blocks=[b for b in (directory/'stacks.txt').read_text().split('\n\n') if 'cpu-clock:u:' in b]
    total=own=inclusive=0
    for block in blocks:
        lines=block.splitlines()
        match=re.search(r':\s+(\d+) cpu-clock:u:',lines[0])
        assert match,lines[0]
        weight=int(match.group(1));total+=weight
        if len(lines)>1 and 'vrl::value::value::crud::insert::insert+' in lines[1]: own+=weight
        if any('vrl::value::value::crud::insert::insert+' in l for l in lines[1:]): inclusive+=weight
    result=dict(run=index,variant=variant,seconds=seconds,events=events,eps=events/seconds,cpu_cores=cpu_seconds/seconds,cpu_us_per_event=1e6*cpu_seconds/events,samples=len(blocks),sampled_user_cpu_seconds=total/1e9,crud_self_pct=100*own/total,crud_stack_union_pct=100*inclusive/total,crud_self_sampled_us_per_event=own/1000/events,crud_stack_union_sampled_us_per_event=inclusive/1000/events,unwind_warning_count=(directory/'unwind-warnings.txt').read_text().count('unsupported'))
    print('PROFILE',json.dumps(result),flush=True)
    print(kub(['-n',NS,'wait','--for=jsonpath={.status.phase}=Succeeded','pod/generator-0','pod/generator-1','--timeout=90s']),flush=True)
    expected=0
    for i in range(2):
        summary=generator_summary(i)
        (directory/f'generator-{i}.json').write_text(json.dumps(summary))
        assert 74000<=summary['events']/summary['seconds']<=76000,summary
        expected+=summary['events']+1
    drain=[]
    for i in range(37):
        value=scrape(directory,100+i,pid);drain.append(value)
        (directory/'drain.json').write_text(json.dumps(drain,indent=2))
        if value['events']>=expected:break
        if len(drain)>=7 and len({r['events'] for r in drain[-7:]})==1:break
        time.sleep(5)
    result.update(expected_events=expected,final_events=drain[-1]['events'])
    (directory/'collector.log').write_text(kub(['-n',NS,'logs','vector']))
    (directory/'summary.json').write_text(json.dumps(result,indent=2))
    results.append(result)
    (OUT/'results.json').write_text(json.dumps(results,indent=2))
    print('DONE',json.dumps(result),flush=True)
print('COMPLETE',json.dumps(results,indent=2),flush=True)
