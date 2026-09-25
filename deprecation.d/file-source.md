---
what: "`file` source"
deprecated_since: "0.59.0"
---

The `file` source is deprecated in favor of [`file_v2`](https://vector.dev/docs/reference/configuration/sources/file_v2/),
a modern replacement using asynchronous reads and filesystem notifications. The `file` source remains available;
no removal version has been scheduled.

To migrate, change the source type from `file` to `file_v2` and review these differences before deploying:

- Checksum fingerprinting uses a fixed byte prefix (`fingerprint.bytes`, default `1024`) instead of lines.
  Files smaller than this prefix wait until enough bytes are available. Lower `fingerprint.bytes` for small files.
  Existing checksum fingerprints differ, so switching can replay previously ingested data. Test checkpoint and
  restart behavior with representative files before migrating production workloads.
- Remove `oldest_first`; `file_v2` shares read time across files.
- Replace `rotate_wait_secs` with `reader_idle_timeout_secs` (default `30`). This limits how long a reader remains
  open at EOF after its file is no longer discoverable. Writes after that reader closes can be lost.
- Use canonical option names such as `ignore_older_secs`, `remove_after_secs`, and `fingerprint`.
  Replace `start_at_beginning` with `read_from` and `ignore_checkpoints`, and use `multiline` instead of
  `message_start_indicator` and `multi_line_timeout`.

For example:

```yaml
sources:
  application_logs:
    type: file_v2
    include:
      - /var/log/application/*.log
    fingerprint:
      strategy: checksum
      bytes: 1024
```

The emitted `source_type` also changes from `file` to `file_v2`; update filters that depend on it.
