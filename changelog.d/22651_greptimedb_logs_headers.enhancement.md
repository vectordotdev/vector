add `extra_headers` option to `greptimedb_logs` sink to set additional headers for the request

example:

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

author: greptimedb
