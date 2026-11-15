"""Report sampled CPU costs, not invocation/comparison counts."""
import json
from pathlib import Path
import re
import statistics

root = Path(__file__).parent / 'profiles-bulk-150000eps'
runs = json.loads((root / 'results.json').read_text())
assert len(runs) == 4
output = []
for run in runs:
    directory = root / f"{run['run']:02}-{run['variant']}"
    def btree(line):
        return 'alloc::collections::btree::' in line or 'alloc11collections5btree' in line
    weights = dict(total=0, memcmp_self=0, btree_self=0, btree_stack=0, memcmp_btree=0, memcmp_crud=0, memcmp_pod=0)
    for block in (directory / 'stacks.txt').read_text().split('\n\n'):
        if 'cpu-clock:u:' not in block:
            continue
        lines = block.splitlines()
        weight = int(re.search(r':\s+(\d+) cpu-clock:u:', lines[0]).group(1))
        frames = lines[1:]
        weights['total'] += weight
        is_memcmp = bool(frames and re.search(r'\bmemcmp\+', frames[0]))
        has_btree = any(btree(line) for line in frames)
        if is_memcmp: weights['memcmp_self'] += weight
        if frames and btree(frames[0]): weights['btree_self'] += weight
        if has_btree: weights['btree_stack'] += weight
        if is_memcmp and has_btree: weights['memcmp_btree'] += weight
        if is_memcmp and any('vrl::value::value::crud::insert::insert+' in line for line in frames): weights['memcmp_crud'] += weight
        if is_memcmp and any('pod_metadata_annotator' in line for line in frames): weights['memcmp_pod'] += weight
    output.append(dict(run=run['run'], variant=run['variant'], events=run['events'], cpu_us_per_event=run['cpu_us_per_event'], unwind_warning_count=run['unwind_warning_count'], metrics={name: dict(sample_pct=100*weight/weights['total'], sampled_us_per_event=weight/1000/run['events']) for name, weight in weights.items() if name != 'total'}))
means = {}
for variant in ['baseline', 'bulk']:
    selected = [r for r in output if r['variant'] == variant]
    means[variant] = dict(cpu_us_per_event=statistics.mean(r['cpu_us_per_event'] for r in selected), metrics={name: {unit: statistics.mean(r['metrics'][name][unit] for r in selected) for unit in ['sample_pct', 'sampled_us_per_event']} for name in selected[0]['metrics']})
result = dict(runs=output, means=means, caveat='Sampling estimates CPU time, not exact comparisons. Inclusive categories overlap; unwinding and inlining limit attribution.')
(root / 'comparison-cost-summary.json').write_text(json.dumps(result, indent=2))
print(json.dumps(means, indent=2))
