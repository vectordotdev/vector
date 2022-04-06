# Test Results

## Test 1

```shell
toby@consigliere:~/src/vector/testing/github-12069$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/testing/github-12069$ ls -l /tmp/vector/github-12069/
total 0
toby@consigliere:~/src/vector/testing/github-12069$ cat five-lines-first | VECTOR_LOG="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info" ./vector-v0.20.0 -c ./config-wrong-http.toml
2022-04-06T05:04:13.849578Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info"
2022-04-06T05:04:13.849653Z  INFO vector::app: Loading configs. paths=["config-wrong-http.toml"]
2022-04-06T05:04:13.850709Z  INFO vector::sources::stdin: Capturing STDIN.
2022-04-06T05:04:13.878010Z  INFO vector::topology::running: Running healthchecks.
2022-04-06T05:04:13.878042Z  INFO vector::topology::running: Starting source. key=stdin
2022-04-06T05:04:13.878062Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-04-06T05:04:13.878061Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-04-06T05:04:13.878090Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="2a706a3 2022-02-11"
2022-04-06T05:04:13.878100Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
2022-04-06T05:04:13.878254Z  INFO vector::shutdown: All sources have finished.
2022-04-06T05:04:13.878259Z  INFO vector: Vector has stopped.
2022-04-06T05:04:13.878295Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-04-06T05:04:13.879186Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-04-06T05:04:14.881161Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
...
2022-04-06T05:04:16.884213Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-04-06T05:04:17.153507Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/testing/github-12069$ ls -l /tmp/vector/github-12069/
total 4
drwxr-xr-x 2 toby toby 4096 Apr  6 01:04 http_tarpit_id
toby@consigliere:~/src/vector/testing/github-12069$ cat five-lines-second | VECTOR_LOG="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info" ./vector-pr -c ./config-wrong-http.toml
2022-04-06T05:04:24.925006Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info"
2022-04-06T05:04:24.925046Z  INFO vector::app: Loading configs. paths=["config-wrong-http.toml"]
2022-04-06T05:04:24.925725Z  INFO vector::sources::stdin: Capturing STDIN.
2022-04-06T05:04:24.954258Z  INFO vector_buffers::variants::disk_v2::v1_migration: Detected old `disk_v1`-based buffer for the `http_tarpit` sink. Automatically migrating to `disk_v2`.
2022-04-06T05:04:24.954512Z  INFO vector_buffers::variants::disk_v2::v1_migration: Migrated 5 records in disk buffer for `http_tarpit` sink. Old disk buffer at '/tmp/vector/github-12069/http_tarpit_id' has been deleted, and the new disk buffer has been created at '/tmp/vector/github-12069/buffer/v2/http_tarpit'.
2022-04-06T05:04:24.965194Z  INFO vector::topology::running: Running healthchecks.
2022-04-06T05:04:24.965227Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-04-06T05:04:24.965266Z  INFO vector: Vector has started. debug="false" version="0.21.0" arch="x86_64" build_id="none"
2022-04-06T05:04:24.965360Z  INFO vector::shutdown: All sources have finished.
2022-04-06T05:04:24.965366Z  INFO vector: Vector has stopped.
2022-04-06T05:04:24.965390Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-04-06T05:04:24.966574Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-04-06T05:04:25.967353Z ERROR sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}:http: vector::internal_events::http_client: HTTP error. error=error trying to connect: tcp connect error: Connection refused (os error 111) error_type="request_failed" stage="processing"
...
2022-04-06T05:04:27.970443Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-04-06T05:04:28.341582Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/testing/github-12069$ ls -l /tmp/vector/github-12069/
total 4
drwxrwxr-x 3 toby toby 4096 Apr  6 01:04 buffer
toby@consigliere:~/src/vector/testing/github-12069$ cat five-lines-second | VECTOR_LOG="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info" ./vector-pr -c ./config-right-http.toml
2022-04-06T05:04:49.046963Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info"
2022-04-06T05:04:49.047005Z  INFO vector::app: Loading configs. paths=["config-right-http.toml"]
2022-04-06T05:04:49.047691Z  INFO vector::sources::stdin: Capturing STDIN.
2022-04-06T05:04:49.063511Z  INFO vector::topology::running: Running healthchecks.
2022-04-06T05:04:49.063545Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-04-06T05:04:49.063615Z  INFO vector: Vector has started. debug="false" version="0.21.0" arch="x86_64" build_id="none"
2022-04-06T05:04:49.063676Z  INFO vector::shutdown: All sources have finished.
2022-04-06T05:04:49.063686Z  INFO vector: Vector has stopped.
2022-04-06T05:04:49.063698Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-04-06T05:04:49.064697Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/testing/github-12069$ ls -l /tmp/vector/github-12069/
total 4
drwxrwxr-x 3 toby toby 4096 Apr  6 01:04 buffer
toby@consigliere:~/src/vector/testing/github-12069$

### Output from `dummyhttp`, listening on port 7777:

┌─Incoming request
│ POST /foo HTTP/1.1
│ Accept-Encoding: identity
│ Content-Length: 1904
│ Content-Type: application/x-ndjson
│ Host: localhost:7777
│ User-Agent: Vector/0.21.0 (x86_64-unknown-linux-gnu)
│ Body:
│ {"host":"consigliere","message":"ONE line one, woohoo","source_type":"stdin","timestamp":"2022-04-06T05:04:13.878220775Z"}
│ {"host":"consigliere","message":"ONE line two, yippeee","source_type":"stdin","timestamp":"2022-04-06T05:04:13.878232125Z"}
│ {"host":"consigliere","message":"ONE line three, oh my","source_type":"stdin","timestamp":"2022-04-06T05:04:13.878235815Z"}
│ {"host":"consigliere","message":"ONE line four, woooooow","source_type":"stdin","timestamp":"2022-04-06T05:04:13.878239495Z"}
│ {"host":"consigliere","message":"ONE live five, phew, that was a lot","source_type":"stdin","timestamp":"2022-04-06T05:04:13.878243155Z"}
│ {"host":"consigliere","message":"TWO line one, woohoo","source_type":"stdin","timestamp":"2022-04-06T05:04:24.965330623Z"}
│ {"host":"consigliere","message":"TWO line two, yippeee","source_type":"stdin","timestamp":"2022-04-06T05:04:24.965339593Z"}
│ {"host":"consigliere","message":"TWO line three, oh my","source_type":"stdin","timestamp":"2022-04-06T05:04:24.965342863Z"}
│ {"host":"consigliere","message":"TWO line four, woooooow","source_type":"stdin","timestamp":"2022-04-06T05:04:24.965347233Z"}
│ {"host":"consigliere","message":"TWO live five, phew, that was a lot","source_type":"stdin","timestamp":"2022-04-06T05:04:24.965350343Z"}
│ {"host":"consigliere","message":"TWO line one, woohoo","source_type":"stdin","timestamp":"2022-04-06T05:04:49.063643366Z"}
│ {"host":"consigliere","message":"TWO line two, yippeee","source_type":"stdin","timestamp":"2022-04-06T05:04:49.063651736Z"}
│ {"host":"consigliere","message":"TWO line three, oh my","source_type":"stdin","timestamp":"2022-04-06T05:04:49.063655096Z"}
│ {"host":"consigliere","message":"TWO line four, woooooow","source_type":"stdin","timestamp":"2022-04-06T05:04:49.063659046Z"}
│ {"host":"consigliere","message":"TWO live five, phew, that was a lot","source_type":"stdin","timestamp":"2022-04-06T05:04:49.063662266Z"}
┌─Outgoing response
│ HTTP/1.1 200 OK
│ Body:
│ dummyhttp
```

## Test 2

```shell
toby@consigliere:~/src/vector/testing/github-12069$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/testing/github-12069$ ls -l /tmp/vector/github-12069
total 0
toby@consigliere:~/src/vector/testing/github-12069$ cat five-lines-first | VECTOR_LOG="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info" ./vector-v0.20.0 -c ./config-wrong-http-disk-v2.toml
2022-04-06T14:45:58.018346Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info"
2022-04-06T14:45:58.018397Z  INFO vector::app: Loading configs. paths=["config-wrong-http-disk-v2.toml"]
2022-04-06T14:45:58.019085Z  WARN vector_buffers::config: !!!! The `disk_v2` buffer type is not yet stable.  Data loss may be encountered. !!!!
2022-04-06T14:45:58.019150Z  INFO vector::sources::stdin: Capturing STDIN.
2022-04-06T14:45:58.040008Z  INFO vector::topology::running: Running healthchecks.
2022-04-06T14:45:58.040048Z  INFO vector::topology::running: Starting source. key=stdin
2022-04-06T14:45:58.040045Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-04-06T14:45:58.040065Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-04-06T14:45:58.040089Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="2a706a3 2022-02-11"
2022-04-06T14:45:58.040097Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
2022-04-06T14:45:58.040231Z  INFO vector::shutdown: All sources have finished.
2022-04-06T14:45:58.040235Z  INFO vector: Vector has stopped.
2022-04-06T14:45:58.040260Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-04-06T14:45:58.041205Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-04-06T14:45:59.041836Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
...
2022-04-06T14:46:01.044682Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-04-06T14:46:01.168349Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/testing/github-12069$ ls -l /tmp/vector/github-12069
total 4
drwxrwxr-x 3 toby toby 4096 Apr  6 10:45 buffer
toby@consigliere:~/src/vector/testing/github-12069$ cat five-lines-second | VECTOR_LOG="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info" ./vector-pr -c ./config-wrong-http.toml
2022-04-06T14:46:10.662767Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info"
2022-04-06T14:46:10.662807Z  INFO vector::app: Loading configs. paths=["config-wrong-http.toml"]
2022-04-06T14:46:10.663483Z  INFO vector::sources::stdin: Capturing STDIN.
2022-04-06T14:46:10.682030Z  INFO vector::topology::running: Running healthchecks.
2022-04-06T14:46:10.682055Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-04-06T14:46:10.682089Z  INFO vector: Vector has started. debug="false" version="0.21.0" arch="x86_64" build_id="none"
2022-04-06T14:46:10.682162Z  INFO vector::shutdown: All sources have finished.
2022-04-06T14:46:10.682168Z  INFO vector: Vector has stopped.
2022-04-06T14:46:10.682186Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-04-06T14:46:10.683351Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-04-06T14:46:11.683874Z ERROR sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}:http: vector::internal_events::http_client: HTTP error. error=error trying to connect: tcp connect error: Connection refused (os error 111) error_type="request_failed" stage="processing"
...
2022-04-06T14:46:13.686843Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-04-06T14:46:13.998989Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/testing/github-12069$ ls -l /tmp/vector/github-12069
total 4
drwxrwxr-x 3 toby toby 4096 Apr  6 10:45 buffer
toby@consigliere:~/src/vector/testing/github-12069$ cat five-lines-second | VECTOR_LOG="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info" ./vector-pr -c ./config-right-http.toml
2022-04-06T14:46:19.954798Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info"
2022-04-06T14:46:19.954863Z  INFO vector::app: Loading configs. paths=["config-right-http.toml"]
2022-04-06T14:46:19.955816Z  INFO vector::sources::stdin: Capturing STDIN.
2022-04-06T14:46:19.977729Z  INFO vector::topology::running: Running healthchecks.
2022-04-06T14:46:19.977779Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-04-06T14:46:19.977845Z  INFO vector: Vector has started. debug="false" version="0.21.0" arch="x86_64" build_id="none"
2022-04-06T14:46:19.977909Z  INFO vector::shutdown: All sources have finished.
2022-04-06T14:46:19.977921Z  INFO vector: Vector has stopped.
2022-04-06T14:46:19.977934Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-04-06T14:46:19.978975Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/testing/github-12069$ ls -l /tmp/vector/github-12069
total 4
drwxrwxr-x 3 toby toby 4096 Apr  6 10:45 buffer
toby@consigliere:~/src/vector/testing/github-12069$

### Output from `dummyhttp`, listening on port 7777:

┌─Incoming request
│ POST /foo HTTP/1.1
│ Accept-Encoding: identity
│ Content-Length: 1904
│ Content-Type: application/x-ndjson
│ Host: localhost:7777
│ User-Agent: Vector/0.21.0 (x86_64-unknown-linux-gnu)
│ Body:
│ {"host":"consigliere","message":"ONE line one, woohoo","source_type":"stdin","timestamp":"2022-04-06T14:45:58.040195072Z"}
│ {"host":"consigliere","message":"ONE line two, yippeee","source_type":"stdin","timestamp":"2022-04-06T14:45:58.040210202Z"}
│ {"host":"consigliere","message":"ONE line three, oh my","source_type":"stdin","timestamp":"2022-04-06T14:45:58.040213862Z"}
│ {"host":"consigliere","message":"ONE line four, woooooow","source_type":"stdin","timestamp":"2022-04-06T14:45:58.040217562Z"}
│ {"host":"consigliere","message":"ONE live five, phew, that was a lot","source_type":"stdin","timestamp":"2022-04-06T14:45:58.040221152Z"}
│ {"host":"consigliere","message":"TWO line one, woohoo","source_type":"stdin","timestamp":"2022-04-06T14:46:10.682136661Z"}
│ {"host":"consigliere","message":"TWO line two, yippeee","source_type":"stdin","timestamp":"2022-04-06T14:46:10.682144171Z"}
│ {"host":"consigliere","message":"TWO line three, oh my","source_type":"stdin","timestamp":"2022-04-06T14:46:10.682146871Z"}
│ {"host":"consigliere","message":"TWO line four, woooooow","source_type":"stdin","timestamp":"2022-04-06T14:46:10.682150341Z"}
│ {"host":"consigliere","message":"TWO live five, phew, that was a lot","source_type":"stdin","timestamp":"2022-04-06T14:46:10.682153241Z"}
│ {"host":"consigliere","message":"TWO line one, woohoo","source_type":"stdin","timestamp":"2022-04-06T14:46:19.977879319Z"}
│ {"host":"consigliere","message":"TWO line two, yippeee","source_type":"stdin","timestamp":"2022-04-06T14:46:19.977887119Z"}
│ {"host":"consigliere","message":"TWO line three, oh my","source_type":"stdin","timestamp":"2022-04-06T14:46:19.977889719Z"}
│ {"host":"consigliere","message":"TWO line four, woooooow","source_type":"stdin","timestamp":"2022-04-06T14:46:19.977892039Z"}
│ {"host":"consigliere","message":"TWO live five, phew, that was a lot","source_type":"stdin","timestamp":"2022-04-06T14:46:19.977895399Z"}
┌─Outgoing response
│ HTTP/1.1 200 OK
│ Body:
│ dummyhttp
```

## Test 3

```shell
toby@consigliere:~/src/vector/testing/github-12069$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/testing/github-12069$ ls -l /tmp/vector/github-12069
total 0
toby@consigliere:~/src/vector/testing/github-12069$ ~/go/bin/flog -l | VECTOR_LOG="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info" ./vector-v0.20.0 -c ./config-wrong-http-big-buffer.toml
2022-04-06T17:40:40.693387Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info"
2022-04-06T17:40:40.693437Z  INFO vector::app: Loading configs. paths=["config-wrong-http-big-buffer.toml"]
2022-04-06T17:40:40.694227Z  INFO vector::sources::stdin: Capturing STDIN.
2022-04-06T17:40:40.728626Z  INFO vector::topology::running: Running healthchecks.
2022-04-06T17:40:40.728662Z  INFO vector::topology::running: Starting source. key=stdin
2022-04-06T17:40:40.728684Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-04-06T17:40:40.728681Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-04-06T17:40:40.728709Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="2a706a3 2022-02-11"
2022-04-06T17:40:40.728718Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
2022-04-06T17:40:41.077849Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
...
2022-04-06T17:41:01.103461Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-04-06T17:41:05.913348Z  INFO vector: Vector has stopped.
2022-04-06T17:41:05.914492Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit, stdin" time_remaining="59 seconds left"
^C2022-04-06T17:41:06.497154Z  INFO vector: Vector has quit.

toby@consigliere:~/src/vector/testing/github-12069$ ls -l /tmp/vector/github-12069
total 12
drwxr-xr-x 2 toby toby 12288 Apr  6 13:40 http_tarpit_id
toby@consigliere:~/src/vector/testing/github-12069$ du -h /tmp/vector/github-12069/http_tarpit_id/
929M    /tmp/vector/github-12069/http_tarpit_id/
toby@consigliere:~/src/vector/testing/github-12069$ VECTOR_LOG="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info" ./vector-pr -c ./config-wrong-http-big-buffer.toml
2022-04-06T17:41:52.772547Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info,kube=info,vector_buffers=info"
2022-04-06T17:41:52.772588Z  INFO vector::app: Loading configs. paths=["config-wrong-http-big-buffer.toml"]
2022-04-06T17:41:52.773258Z  INFO vector::sources::stdin: Capturing STDIN.
2022-04-06T17:41:53.809795Z  INFO vector_buffers::variants::disk_v2::v1_migration: Detected old `disk_v1`-based buffer for the `http_tarpit` sink. Automatically migrating to `disk_v2`.
2022-04-06T17:43:08.244317Z  INFO vector_buffers::variants::disk_v2::v1_migration: Migrated 4560522 records in disk buffer for `http_tarpit` sink. Old disk buffer at '/tmp/vector/github-12069/http_tarpit_id' has been deleted, and the new disk buffer has been created at '/tmp/vector/github-12069/buffer/v2/http_tarpit'.
2022-04-06T17:43:08.256533Z  INFO vector::topology::running: Running healthchecks.
2022-04-06T17:43:08.256573Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-04-06T17:43:08.256616Z  INFO vector: Vector has started. debug="false" version="0.21.0" arch="x86_64" build_id="none"
2022-04-06T17:43:08.523110Z ERROR sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}:http: vector::internal_events::http_client: HTTP error. error=error trying to connect: tcp connect error: Connection refused (os error 111) error_type="request_failed" stage="processing"
...
2022-04-06T17:43:15.537528Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-04-06T17:43:17.057250Z  INFO vector: Vector has stopped.
2022-04-06T17:43:17.057303Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-04-06T17:43:17.058370Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
^C2022-04-06T17:43:18.428485Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/testing/github-12069$ ls -l /tmp/vector/github-12069
total 4
drwxrwxr-x 3 toby toby 4096 Apr  6 13:41 buffer
toby@consigliere:~/src/vector/testing/github-12069$ du -h /tmp/vector/github-12069/buffer/v2/http_tarpit
1.1G    /tmp/vector/github-12069/buffer/v2/http_tarpit
toby@consigliere:~/src/vector/testing/github-12069$
```
