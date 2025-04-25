Added a new `extra_headers` option to `greptimedb_logs` sink configuration to set additional headers for outgoing requests.

change `greptimedb_logs` sink default content type to `application/x-ndjson` to match the default content type of `greptimedb` sink
if you use the greptimedb version v0.12 or earlier, you need to set the content type to `application/json` in the sink configuration

Example:

```yaml
sinks:
  greptime_logs:
    type: greptimedb_logs
    inputs: ["my_source_id"]
    endpoint: "http://localhost:4000"
    table: "demo_logs"
    dbname: "public"
    extra_headers:
      x-source: vector
```

```toml
[sinks.greptime_logs]
type = "greptimedb_logs"
inputs = ["my_source_id"]
endpoint = "http://localhost:4000"
table = "demo_logs"
dbname = "public"

[sinks.greptime_logs.extra_headers]
x-source = "vector"
```

authors: greptimedb
