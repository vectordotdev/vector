## Test Results

### Test 1

```shell
toby@consigliere:~/src/vector/testing/github-11072$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/testing/github-11072$ ls -l /tmp/vector/github-11072/
total 0
toby@consigliere:~/src/vector/testing/github-11072$ cat five-lines | ./vector-v0.19.0 --config config.toml
2022-02-01T22:05:49.624703Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-02-01T22:05:49.624744Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-02-01T22:05:49.625474Z  INFO vector::sources::stdin: Capturing STDIN.
2022-02-01T22:05:49.657841Z  INFO vector::topology::running: Running healthchecks.
2022-02-01T22:05:49.657872Z  INFO vector::topology::running: Starting source. key=stdin
2022-02-01T22:05:49.657904Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-02-01T22:05:49.657901Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-02-01T22:05:49.657943Z  INFO vector: Vector has started. debug="false" version="0.19.0" arch="x86_64" build_id="da60b55 2021-12-28"
2022-02-01T22:05:49.657957Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
2022-02-01T22:05:49.658116Z  INFO vector::shutdown: All sources have finished.
2022-02-01T22:05:49.658124Z  INFO vector: Vector has stopped.
2022-02-01T22:05:49.658124Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-02-01T22:05:49.659083Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-02-01T22:05:50.661069Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-02-01T22:05:51.281241Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/testing/github-11072$ cat five-lines | ./vector-pr --config config.toml
2022-02-01T22:05:56.804345Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-02-01T22:05:56.804379Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-02-01T22:05:56.804900Z  INFO vector::sources::stdin: Capturing STDIN.
2022-02-01T22:05:56.836558Z  INFO vector::topology::running: Running healthchecks.
2022-02-01T22:05:56.836584Z  INFO vector::topology::running: Starting source. key=stdin
2022-02-01T22:05:56.836598Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-02-01T22:05:56.836604Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-02-01T22:05:56.836644Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
2022-02-01T22:05:56.836723Z  INFO vector::shutdown: All sources have finished.
2022-02-01T22:05:56.836727Z  INFO vector: Vector has stopped.
2022-02-01T22:05:56.836737Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-02-01T22:05:56.837737Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-02-01T22:05:57.838479Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-02-01T22:05:58.839992Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-02-01T22:05:59.194145Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/testing/github-11072$ ./vector-pr --config config.toml
2022-02-01T22:06:12.151337Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-02-01T22:06:12.151372Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-02-01T22:06:12.151884Z  INFO vector::sources::stdin: Capturing STDIN.
2022-02-01T22:06:12.180786Z  INFO vector::topology::running: Running healthchecks.
2022-02-01T22:06:12.180811Z  INFO vector::topology::running: Starting source. key=stdin
2022-02-01T22:06:12.180819Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-02-01T22:06:12.180831Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-02-01T22:06:12.180862Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
2022-02-01T22:06:17.852345Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: connection closed before message completed
2022-02-01T22:06:18.853830Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-02-01T22:06:19.771481Z  INFO vector: Vector has stopped.
2022-02-01T22:06:19.771542Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-02-01T22:06:19.772633Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-02-01T22:06:19.854946Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-02-01T22:06:21.323715Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/testing/github-11072$

# Output from netcat after running `vector-pr`:

POST /foo HTTP/1.1
content-type: application/x-ndjson
user-agent: Vector/0.20.0 (x86_64-unknown-linux-gnu)
accept-encoding: identity
host: localhost:7777
content-length: 1230

{"host":"consigliere","message":"line one, woohoo","source_type":"stdin","timestamp":"2022-02-01T22:05:49.658077239Z"}
{"host":"consigliere","message":"line two, yippeee","source_type":"stdin","timestamp":"2022-02-01T22:05:49.658090639Z"}
{"host":"consigliere","message":"line three, oh my","source_type":"stdin","timestamp":"2022-02-01T22:05:49.658094649Z"}
{"host":"consigliere","message":"line four, woooooow","source_type":"stdin","timestamp":"2022-02-01T22:05:49.658098299Z"}
{"host":"consigliere","message":"live five, phew, that was a lot","source_type":"stdin","timestamp":"2022-02-01T22:05:49.658101929Z"}
{"host":"consigliere","message":"line one, woohoo","source_type":"stdin","timestamp":"2022-02-01T22:05:56.836679868Z"}
{"host":"consigliere","message":"line two, yippeee","source_type":"stdin","timestamp":"2022-02-01T22:05:56.836699668Z"}
{"host":"consigliere","message":"line three, oh my","source_type":"stdin","timestamp":"2022-02-01T22:05:56.836704718Z"}
{"host":"consigliere","message":"line four, woooooow","source_type":"stdin","timestamp":"2022-02-01T22:05:56.836707728Z"}
{"host":"consigliere","message":"live five, phew, that was a lot","source_type":"stdin","timestamp":"2022-02-01T22:05:56.836710798Z"}
```
