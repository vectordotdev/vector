The TLS `crt_file` and `key_file` from `http` sinks are now watched when `--watch_config` is enabled and therefore changes to those files will trigger a config reload without the need to restart Vector.

authors: gllb
