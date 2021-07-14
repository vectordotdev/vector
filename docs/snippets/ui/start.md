#### Health checks

By default, Vector performs health checks on all components. Health checks ensure that the downstream service is accessible and ready to accept data. This is check is perform when the component is initialized. If the check fails, an error is logged and Vector proceeds to start.

##### Require health checks

To make Vector immediately exit when any health check fails, pass the `--require-healthy` flag when starting Vector:

```bash
vector --config /etc/vector/vector.toml --require-healthy
```

##### Disable health checks

To disable health checks, set the `healthcheck` option to `false` for each component:

```toml
[sinks.my-sink]
type = "..."
healthcheck = false
```

#### Options

See the [`start` command CLI reference][start] for a comprehensive list of command line flags and options.

[start]: /docs/reference/cli/#start
