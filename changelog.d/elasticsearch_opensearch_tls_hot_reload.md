The TLS `ca_file`, `crt_file` and `key_file` from `elasticsearch` and `opensearch` sinks are now watched when `--watch-config` is enabled and therefore changes to those files will trigger a config reload without the need to restart Vector.

authors: thomascellerier
