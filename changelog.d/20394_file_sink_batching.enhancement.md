The `file` sink now batches events per destination path before writing. Events sharing the
same rendered path are accumulated into a single buffer and flushed with one write syscall
per batch, rather than one syscall per event.

This significantly reduces overhead when routing to many partitions — for example, writing
one file per Kafka topic with a path template like `/data/topics/{{ _topic }}/events.log`.
Throughput on a single file improves ~10x; high-partition workloads (64 topics) improve ~13%.

Batching is controlled by the new `batch` configuration block:

```yaml
sinks:
  file_out:
    type: file
    path: /data/topics/{{ _topic }}/events.log
    batch:
      max_bytes: 10485760   # 10 MiB (default)
      timeout_secs: 1       # flush after 1 second of inactivity (default)
```

Issue: https://github.com/vectordotdev/vector/issues/20394

authors: mbergman
