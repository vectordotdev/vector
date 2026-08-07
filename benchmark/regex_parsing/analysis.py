import json, sys
from collections import defaultdict

run_dir = sys.argv[1]

# --- Throughput from lading captures ---
total_written = total_received = total_requests = 0
start_t = end_t = None
with open(f"{run_dir}/lading.captures") as f:
    for line in f:
        obj = json.loads(line)
        metric = obj.get('metric_name', '')
        comp = obj.get('component', '')
        cname = obj.get('component_name', '')
        value = obj.get('value', 0)
        t = obj.get('time', 0)
        if start_t is None or t < start_t: start_t = t
        if end_t is None or t > end_t: end_t = t
        if cname == 'http':
            if metric == 'bytes_written' and comp == 'generator':
                total_written = max(total_written, value)
            elif metric == 'bytes_received' and comp == 'blackhole':
                total_received = max(total_received, value)
            elif metric == 'requests_sent' and comp == 'generator':
                total_requests = max(total_requests, value)

duration = max((end_t - start_t) / 1000.0, 1e-9)
print(f"  Duration:    {duration:.1f}s")
print(f"  Sent in:     {total_written/1e6:7.1f} MB ({total_written/1e6/duration:6.1f} MB/s)")
print(f"  Sent out:    {total_received/1e6:7.1f} MB ({total_received/1e6/duration:6.1f} MB/s)")
print(f"  Requests/s:  {total_requests/duration:.0f}")

# --- Remap CPU breakdown ---
categories = defaultdict(int)
total = 0
with open(f"{run_dir}/sample.folded") as f:
    for line in f:
        line = line.strip()
        if not line: continue
        parts = line.rsplit(' ', 1)
        if len(parts) != 2: continue
        stack, count = parts[0], int(parts[1])
        # Only stacks doing remap work, not parked
        if 'SyncTransform::transform_all' not in stack and 'Remap' not in stack:
            continue
        if 'park_internal' in stack or '__psynch_cvwait' in stack or 'kevent' in stack:
            continue
        total += count
        leaf = stack.split(';')[-1]
        if 'regex_automata' in stack:
            if any(x in stack for x in ('get_slow', 'create_cache', 'init_cache')):
                categories['regex: cache miss/init'] += count
            else:
                categories['regex: DFA matching'] += count
        elif 'capture_regex_to_map' in stack:
            categories['capture_regex_to_map'] += count
        elif 'BTreeMap' in stack and ('clone' in stack or 'dying' in stack):
            categories['BTreeMap clone / drop'] += count
        elif 'drop_in_place' in leaf or 'drop_slow' in leaf:
            categories['Value drop/dealloc'] += count
        elif 'finish_grow' in leaf or 'nanov2' in leaf or 'malloc' in leaf.lower() or 'realloc' in leaf:
            categories['heap alloc'] += count
        elif '_free' in leaf or 'nanov2_free' in leaf or 'bzero' in leaf or 'memset' in leaf:
            categories['heap free'] += count
        elif 'tracing_subscriber' in stack and ('event' in stack.lower() or 'record' in stack):
            categories['tracing: error events'] += count
        elif 'vrl' in stack and 'resolve' in stack:
            categories['VRL interpreter'] += count
        elif 'memmove' in leaf or 'memcpy' in leaf:
            categories['memcpy/memmove'] += count
        elif 'Arc' in stack or 'drop_in_place' in stack:
            categories['Arc/refcount'] += count
        else:
            categories['other'] += count

print()
print(f"  Remap samples: {total}")
for k, v in sorted(categories.items(), key=lambda x: -x[1]):
    pct = 100.0 * v / total if total else 0
    bar = '█' * int(pct / 2)
    print(f"    {pct:5.1f}% {bar:<25} {k} ({v})")
