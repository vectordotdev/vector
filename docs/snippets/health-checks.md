Health checks ensure that the downstream service is accessible and ready to accept data. This check is performed upon sink initialization. If the health check fails an error will be logged and Vector will proceed to start.

#### Require health checks

If you'd like to exit immediately upon a health check failure, you can pass the `--require-healthy` flag:

```shell
vector --config /etc/vector/vector.toml --require-healthy
```

#### Disable health checks

If you'd like to disable health checks for this sink you can set the [`healthcheck`](#healthcheck) option to `false`.
