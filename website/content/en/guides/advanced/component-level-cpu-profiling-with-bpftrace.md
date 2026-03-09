---
title: Component-Level CPU Profiling with bpftrace
description: Use bpftrace to attribute CPU samples to individual Vector components for targeted performance analysis
authors: ["connoryy"]
domain: observability
weight: 1
tags: ["profiling", "bpftrace", "cpu", "observability", "advanced", "guides", "guide"]
---

When investigating CPU usage in a Vector pipeline, standard profiling tools show
which _functions_ are hot but not which _components_ (sources, transforms,
sinks) are responsible. The `component-probes` feature solves this by tagging
each thread with the currently active component so that bpftrace can sample it
externally.

## Prerequisites

- Linux with [bpftrace](https://github.com/bpftrace/bpftrace)
- Root or `CAP_BPF`
- Vector built with `--features component-probes`

## How It Works

When `component-probes` is enabled, Vector writes the active component's ID to
a per-thread atomic on span enter and clears it on exit. Two `extern "C"`
functions serve as uprobe attachment points:

- `vector_register_thread(tid, label_ptr)` — maps a thread's TID to the
  address of its label (fired once per thread).
- `vector_register_component(group_id, name_ptr, name_len)` — maps a group
  ID to a component name (fired once per component).

## bpftrace Script

Replace `/path/to/vector` with your binary path:

```bpf
#!/usr/bin/env bpftrace

uprobe:/path/to/vector:vector_register_thread {
    @tid_to_addr[arg0] = arg1;
    @vector_pid = pid;
}

uprobe:/path/to/vector:vector_register_component {
    @names[arg0] = str(arg1, arg2);
}

profile:hz:997 {
    if (@vector_pid != 0 && pid == @vector_pid) {
        $addr = @tid_to_addr[tid];
        if ($addr != 0) {
            $group_id = *(uint32 *)$addr;
            if ($group_id != 0) {
                @stacks[@names[$group_id], ustack()] = count();
            }
        }
    }
}
```

This aggregates component-labeled stack traces directly in bpftrace. Start
bpftrace before Vector so it catches the registration uprobes during startup.

If `ustack()` is not available in your environment, replace the `@stacks`
line with a `printf` to emit raw labeled samples that can be joined with
stack traces from other tools like `perf`:

```bpf
printf("S %lld %d %s\n", nsecs, tid, @names[$group_id]);
```

## Overhead

- **Per span enter/exit**: one span extension lookup + one relaxed atomic store.
- **Per thread**: 4 bytes via `Box::leak` (never freed — bpftrace reads the
  address asynchronously with no synchronization).
- **Per component**: one uprobe call at startup.
- **Sampling**: kernel-side, not charged to Vector.

When the feature is not enabled, zero extra code is compiled.
