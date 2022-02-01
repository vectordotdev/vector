## Test Results

### Test 1

```shell
toby@consigliere:~/src/vector/testing/github-11039$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/testing/github-11039$ cat five-lines
line one, woohoo
line two, yippeee
line three, oh my
line four, woooooow
live five, phew, that was a lot
toby@consigliere:~/src/vector/testing/github-11039$ cat five-lines | ./vector-v0.19.0 --config config.toml
2022-01-26T19:16:16.785776Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-26T19:16:16.785818Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-26T19:16:16.786515Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-26T19:16:16.815793Z  INFO vector::topology::running: Running healthchecks.
2022-01-26T19:16:16.815824Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-26T19:16:16.815836Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-26T19:16:16.815846Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-26T19:16:16.815876Z  INFO vector: Vector has started. debug="false" version="0.19.0" arch="x86_64" build_id="da60b55 2021-12-28"
2022-01-26T19:16:16.815886Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
2022-01-26T19:16:16.815939Z  INFO vector::shutdown: All sources have finished.
2022-01-26T19:16:16.815946Z  INFO vector: Vector has stopped.
2022-01-26T19:16:16.815944Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-26T19:16:16.817024Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-01-26T19:16:17.817876Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-01-26T19:16:18.819194Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-01-26T19:16:19.820532Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-01-26T19:16:20.105954Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/testing/github-11039$ ls -l /tmp/vector/github-11039/http_tarpit_id/
total 20
-rw-r--r-- 1 toby toby 639 Jan 26 14:16 000005.log
-rw-r--r-- 1 toby toby  16 Jan 26 14:16 CURRENT
-rw-r--r-- 1 toby toby   0 Jan 26 14:16 LOCK
-rw-rw-r-- 1 toby toby 181 Jan 26 14:16 LOG
-rw-rw-r-- 1 toby toby  60 Jan 26 14:16 LOG.old
-rw-r--r-- 1 toby toby  50 Jan 26 14:16 MANIFEST-000004

# In another window, we run `nc -l -k -p 7777` which runs netcat in listen mode on port 7777, TCP, on all interfaces.

toby@consigliere:~/src/vector/testing/github-11039$ ./vector-pr --config config.toml
2022-01-26T19:17:06.436364Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-26T19:17:06.436404Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-26T19:17:06.436926Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-26T19:17:06.470017Z  INFO vector::topology::running: Running healthchecks.
2022-01-26T19:17:06.470039Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-26T19:17:06.470059Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-26T19:17:06.470057Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-26T19:17:06.470084Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"

# Vector is waiting here for netcat to respond, which it never will, so we Ctrl-C on Vector, which then still has it waiting for netcat to respond, hence the "waiting on running component" log messages, and then we also Ctrl-C netcat which causes the retries, and we do another Ctrl-C to forcefully stop Vector.

^C2022-01-26T19:17:15.674107Z  INFO vector: Vector has stopped.
2022-01-26T19:17:15.674177Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-26T19:17:15.675245Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-01-26T19:17:20.674746Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="54 seconds left"
2022-01-26T19:17:22.625121Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: connection closed before message completed
2022-01-26T19:17:23.626436Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-01-26T19:17:24.627805Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-01-26T19:17:25.572097Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/testing/github-11039$

# Output from netcat after running `vector-pr`:

POST /foo HTTP/1.1
content-type: application/x-ndjson
user-agent: Vector/0.20.0 (x86_64-unknown-linux-gnu)
accept-encoding: identity
host: localhost:7777
content-length: 615

{"host":"consigliere","message":"line one, woohoo","source_type":"stdin","timestamp":"2022-01-26T19:16:16.815899465Z"}
{"host":"consigliere","message":"line two, yippeee","source_type":"stdin","timestamp":"2022-01-26T19:16:16.815909555Z"}
{"host":"consigliere","message":"line three, oh my","source_type":"stdin","timestamp":"2022-01-26T19:16:16.815912815Z"}
{"host":"consigliere","message":"line four, woooooow","source_type":"stdin","timestamp":"2022-01-26T19:16:16.815917085Z"}
{"host":"consigliere","message":"live five, phew, that was a lot","source_type":"stdin","timestamp":"2022-01-26T19:16:16.815920305Z"}
```
