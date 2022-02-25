# Test Results

## Test 1

```shell
toby@consigliere:~/src/vector/testing/github-11072$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/testing/github-11072$ ls -l /tmp/vector/github-11072/
total 0
toby@consigliere:~/src/vector/testing/github-11072$ cat five-lines-first | timeout 5s ./vector-v0.19.0 --config config-wrong-http.toml
2022-02-25T01:37:26.452783Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-02-25T01:37:26.452827Z  INFO vector::app: Loading configs. paths=["config-wrong-http.toml"]
2022-02-25T01:37:26.453530Z  INFO vector::sources::stdin: Capturing STDIN.
2022-02-25T01:37:26.485638Z  INFO vector::topology::running: Running healthchecks.
2022-02-25T01:37:26.485669Z  INFO vector::topology::running: Starting source. key=stdin
2022-02-25T01:37:26.485699Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-02-25T01:37:26.485693Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-02-25T01:37:26.485727Z  INFO vector: Vector has started. debug="false" version="0.19.0" arch="x86_64" build_id="da60b55 2021-12-28"
2022-02-25T01:37:26.485738Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
2022-02-25T01:37:26.485878Z  INFO vector::shutdown: All sources have finished.
2022-02-25T01:37:26.485883Z  INFO vector: Vector has stopped.
2022-02-25T01:37:26.485887Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-02-25T01:37:26.486866Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-02-25T01:37:27.487708Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-02-25T01:37:28.489158Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-02-25T01:37:29.490469Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-02-25T01:37:31.445678Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/testing/github-11072$ LOG=debug timeout 5s ./vector-pr --config config-right-http.toml
2022-02-25T01:37:42.964509Z  INFO vector::app: Log level is enabled. level="debug"
2022-02-25T01:37:42.964543Z  INFO vector::app: Loading configs. paths=["config-right-http.toml"]
2022-02-25T01:37:42.965087Z  INFO vector::sources::stdin: Capturing STDIN.
2022-02-25T01:37:42.982001Z DEBUG vector_buffers::variants::disk_v1: Read 5 key(s) from database, with 585 bytes total, comprising 5 events total. first_key=Some(0) last_key=Some(4)
2022-02-25T01:37:42.997579Z  INFO vector::topology::running: Running healthchecks.
2022-02-25T01:37:42.997604Z  INFO vector::topology::running: Starting source. key=stdin
2022-02-25T01:37:42.997625Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-02-25T01:37:42.997622Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-02-25T01:37:42.997648Z  INFO vector: Vector has started. debug="false" version="0.21.0" arch="x86_64" build_id="none"
2022-02-25T01:37:47.959025Z  INFO vector: Vector has stopped.
2022-02-25T01:37:47.959100Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
toby@consigliere:~/src/vector/testing/github-11072$

### Output from `dummyhttp`, listening on port 7777:

┌─Incoming request
│ POST /foo HTTP/1.1
│ Accept-Encoding: identity
│ Content-Length: 635
│ Content-Type: application/x-ndjson
│ Host: localhost:7777
│ User-Agent: Vector/0.21.0 (x86_64-unknown-linux-gnu)
│ Body:
│ {"host":"consigliere","message":"ONE line one, woohoo","source_type":"stdin","timestamp":"2022-02-25T01:37:26.485844093Z"}
│ {"host":"consigliere","message":"ONE line two, yippeee","source_type":"stdin","timestamp":"2022-02-25T01:37:26.485859923Z"}
│ {"host":"consigliere","message":"ONE line three, oh my","source_type":"stdin","timestamp":"2022-02-25T01:37:26.485863163Z"}
│ {"host":"consigliere","message":"ONE line four, woooooow","source_type":"stdin","timestamp":"2022-02-25T01:37:26.485866083Z"}
│ {"host":"consigliere","message":"ONE live five, phew, that was a lot","source_type":"stdin","timestamp":"2022-02-25T01:37:26.485869063Z"}
┌─Outgoing response
│ HTTP/1.1 200 OK
│ Body:
│ dummyhttp
```

## Test 2

```shell
toby@consigliere:~/src/vector/testing/github-11072$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/testing/github-11072$ ls -l /tmp/vector/github-11072/
total 0
toby@consigliere:~/src/vector/testing/github-11072$ cat five-lines-first | timeout 5s ./vector-v0.19.0 --config config-wrong-http.toml
2022-02-25T01:41:16.856753Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-02-25T01:41:16.856794Z  INFO vector::app: Loading configs. paths=["config-wrong-http.toml"]
2022-02-25T01:41:16.857483Z  INFO vector::sources::stdin: Capturing STDIN.
2022-02-25T01:41:16.890151Z  INFO vector::topology::running: Running healthchecks.
2022-02-25T01:41:16.890179Z  INFO vector::topology::running: Starting source. key=stdin
2022-02-25T01:41:16.890220Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-02-25T01:41:16.890217Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-02-25T01:41:16.890255Z  INFO vector: Vector has started. debug="false" version="0.19.0" arch="x86_64" build_id="da60b55 2021-12-28"
2022-02-25T01:41:16.890266Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
2022-02-25T01:41:16.890384Z  INFO vector::shutdown: All sources have finished.
2022-02-25T01:41:16.890388Z  INFO vector: Vector has stopped.
2022-02-25T01:41:16.890389Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-02-25T01:41:16.891394Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-02-25T01:41:17.893338Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-02-25T01:41:18.894137Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-02-25T01:41:19.895376Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-02-25T01:41:21.849652Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/testing/github-11072$ cat five-lines-second | timeout 5s ./vector-pr --config config-wrong-http.toml
2022-02-25T01:41:44.759260Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-02-25T01:41:44.759298Z  INFO vector::app: Loading configs. paths=["config-wrong-http.toml"]
2022-02-25T01:41:44.759847Z  INFO vector::sources::stdin: Capturing STDIN.
2022-02-25T01:41:44.792206Z  INFO vector::topology::running: Running healthchecks.
2022-02-25T01:41:44.792230Z  INFO vector::topology::running: Starting source. key=stdin
2022-02-25T01:41:44.792237Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-02-25T01:41:44.792249Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-02-25T01:41:44.792277Z  INFO vector: Vector has started. debug="false" version="0.21.0" arch="x86_64" build_id="none"
2022-02-25T01:41:44.792330Z  INFO vector::shutdown: All sources have finished.
2022-02-25T01:41:44.792334Z  INFO vector: Vector has stopped.
2022-02-25T01:41:44.792338Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-02-25T01:41:44.793363Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-02-25T01:41:45.794217Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-02-25T01:41:46.794700Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-02-25T01:41:47.796118Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-02-25T01:41:49.753199Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/testing/github-11072$ LOG=debug timeout 5s ./vector-pr --config config-right-http.toml
2022-02-25T01:42:58.778637Z  INFO vector::app: Log level is enabled. level="debug"
2022-02-25T01:42:58.778672Z  INFO vector::app: Loading configs. paths=["config-right-http.toml"]
2022-02-25T01:42:58.779206Z  INFO vector::sources::stdin: Capturing STDIN.
2022-02-25T01:42:58.796518Z DEBUG vector_buffers::variants::disk_v1: Read 10 key(s) from database, with 1181 bytes total, comprising 10 events total. first_key=Some(0) last_key=Some(9)
2022-02-25T01:42:58.812028Z  INFO vector::topology::running: Running healthchecks.
2022-02-25T01:42:58.812050Z  INFO vector::topology::running: Starting source. key=stdin
2022-02-25T01:42:58.812059Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-02-25T01:42:58.812070Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-02-25T01:42:58.812095Z  INFO vector: Vector has started. debug="false" version="0.21.0" arch="x86_64" build_id="none"
2022-02-25T01:43:03.772068Z  INFO vector: Vector has stopped.
2022-02-25T01:43:03.772144Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-02-25T01:43:03.773225Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/testing/github-11072$

### Output from `dummyhttp`, listening on port 7777:

┌─Incoming request
│ POST /foo HTTP/1.1
│ Accept-Encoding: identity
│ Content-Length: 1270
│ Content-Type: application/x-ndjson
│ Host: localhost:7777
│ User-Agent: Vector/0.21.0 (x86_64-unknown-linux-gnu)
│ Body:
│ {"host":"consigliere","message":"ONE line one, woohoo","source_type":"stdin","timestamp":"2022-02-25T01:41:16.890350232Z"}
│ {"host":"consigliere","message":"ONE line two, yippeee","source_type":"stdin","timestamp":"2022-02-25T01:41:16.890361362Z"}
│ {"host":"consigliere","message":"ONE line three, oh my","source_type":"stdin","timestamp":"2022-02-25T01:41:16.890364482Z"}
│ {"host":"consigliere","message":"ONE line four, woooooow","source_type":"stdin","timestamp":"2022-02-25T01:41:16.890367362Z"}
│ {"host":"consigliere","message":"ONE live five, phew, that was a lot","source_type":"stdin","timestamp":"2022-02-25T01:41:16.890370252Z"}
│ {"host":"consigliere","message":"TWO line one, woohoo","source_type":"stdin","timestamp":"2022-02-25T01:41:44.792297540Z"}
│ {"host":"consigliere","message":"TWO line two, yippeee","source_type":"stdin","timestamp":"2022-02-25T01:41:44.792307870Z"}
│ {"host":"consigliere","message":"TWO line three, oh my","source_type":"stdin","timestamp":"2022-02-25T01:41:44.792312550Z"}
│ {"host":"consigliere","message":"TWO line four, woooooow","source_type":"stdin","timestamp":"2022-02-25T01:41:44.792315949Z"}
│ {"host":"consigliere","message":"TWO live five, phew, that was a lot","source_type":"stdin","timestamp":"2022-02-25T01:41:44.792319309Z"}
┌─Outgoing response
│ HTTP/1.1 200 OK
│ Body:
│ dummyhttp
```
