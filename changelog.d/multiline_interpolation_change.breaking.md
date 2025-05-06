The env var (and secrets) resolution now happens after the config string is parsed into a TOML table.
As a side effect, this fixes a bug where comment lines referring to env vars (or secrets) that don't exist caused a config build error.

This change breaks existing behavior. Injecting whole blocks now results in error e.g.

```shell
export SOURCES_BLOCK="sources:\"
  demo:
    type: demo_logs
    format: json
    interval: 1
```

```yaml
${SOURCES_BLOCK}
```

The config above will fail to load.

authors: pront
