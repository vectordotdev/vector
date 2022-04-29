## Test Results

### Test 1

```shell
toby@consigliere:~/src/vector/bugs/github-10430$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 0
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-v0.19.0 -c config.toml
2022-01-12T20:32:03.681166Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-12T20:32:03.681208Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-12T20:32:03.681884Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-12T20:32:03.714053Z  INFO vector::topology::running: Running healthchecks.
2022-01-12T20:32:03.714081Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-12T20:32:03.714110Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-12T20:32:03.714105Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-12T20:32:03.714146Z  INFO vector: Vector has started. debug="false" version="0.19.0" arch="x86_64" build_id="da60b55 2021-12-28"
2022-01-12T20:32:03.714157Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
^C2022-01-12T20:32:06.890404Z  INFO vector: Vector has stopped.
2022-01-12T20:32:06.890480Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-12T20:32:06.891601Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 4
drwxr-xr-x 2 toby toby 4096 Jan 12 15:32 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 0
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-fix -c config.toml
2022-01-12T20:32:22.317908Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-12T20:32:22.317956Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-12T20:32:22.318771Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-12T20:32:22.351782Z  INFO vector::topology::running: Running healthchecks.
2022-01-12T20:32:22.351831Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-12T20:32:22.351840Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-12T20:32:22.351863Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-12T20:32:22.351894Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
2022-01-12T20:32:22.351905Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
^C2022-01-12T20:32:23.959065Z  INFO vector: Vector has stopped.
2022-01-12T20:32:23.959111Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-12T20:32:23.960238Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 4
drwxr-xr-x 2 toby toby 4096 Jan 12 15:32 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$
```

### Test 2

```shell
toby@consigliere:~/src/vector/bugs/github-10430$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 0
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-v0.18.1 -c config.toml
2022-01-12T20:33:39.848991Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info"
2022-01-12T20:33:39.849036Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-12T20:33:39.850934Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-12T20:33:39.883507Z  INFO vector::topology::running: Running healthchecks.
2022-01-12T20:33:39.883536Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-12T20:33:39.883568Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-12T20:33:39.883564Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-12T20:33:39.883609Z  INFO vector: Vector has started. debug="false" version="0.18.1" arch="x86_64" build_id="c4adb60 2021-11-30"
2022-01-12T20:33:39.883618Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
^C2022-01-12T20:33:43.701580Z  INFO vector: Vector has stopped.
2022-01-12T20:33:43.701645Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-12T20:33:43.702750Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 4
drwxr-xr-x 2 toby toby 4096 Jan 12 15:33 http_tarpit_buffer
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/http_tarpit_buffer/
total 16
-rw-r--r-- 1 toby toby   0 Jan 12 15:33 000005.log
-rw-r--r-- 1 toby toby  16 Jan 12 15:33 CURRENT
-rw-r--r-- 1 toby toby   0 Jan 12 15:33 LOCK
-rw-rw-r-- 1 toby toby 181 Jan 12 15:33 LOG
-rw-rw-r-- 1 toby toby  60 Jan 12 15:33 LOG.old
-rw-r--r-- 1 toby toby  50 Jan 12 15:33 MANIFEST-000004
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-fix -c config.toml
2022-01-12T20:34:12.682402Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-12T20:34:12.682456Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-12T20:34:12.683278Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-12T20:34:12.716426Z  INFO vector::topology::running: Running healthchecks.
2022-01-12T20:34:12.716465Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-12T20:34:12.716486Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-12T20:34:12.716485Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-12T20:34:12.716517Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
2022-01-12T20:34:12.716530Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
^C2022-01-12T20:34:15.058151Z  INFO vector: Vector has stopped.
2022-01-12T20:34:15.058233Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-12T20:34:15.059404Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 4
drwxr-xr-x 2 toby toby 4096 Jan 12 15:34 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/http_tarpit_id/
total 16
-rw-r--r-- 1 toby toby   0 Jan 12 15:34 000009.log
-rw-r--r-- 1 toby toby  16 Jan 12 15:34 CURRENT
-rw-r--r-- 1 toby toby   0 Jan 12 15:33 LOCK
-rw-rw-r-- 1 toby toby 181 Jan 12 15:34 LOG
-rw-rw-r-- 1 toby toby 181 Jan 12 15:34 LOG.old
-rw-r--r-- 1 toby toby  50 Jan 12 15:34 MANIFEST-000008
toby@consigliere:~/src/vector/bugs/github-10430$
```

### Test 3

```shell
toby@consigliere:~/src/vector/bugs/github-10430$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 0
toby@consigliere:~/src/vector/bugs/github-10430$ cat five-lines | ./vector-v0.18.1 -c config.toml
2022-01-13T02:26:45.125553Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info"
2022-01-13T02:26:45.125598Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-13T02:26:45.127484Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-13T02:26:45.153710Z  INFO vector::topology::running: Running healthchecks.
2022-01-13T02:26:45.153742Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-13T02:26:45.153777Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-13T02:26:45.153773Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-13T02:26:45.153813Z  INFO vector: Vector has started. debug="false" version="0.18.1" arch="x86_64" build_id="c4adb60 2021-11-30"
2022-01-13T02:26:45.153823Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
2022-01-13T02:26:45.153889Z  INFO vector::shutdown: All sources have finished.
2022-01-13T02:26:45.153892Z  INFO vector: Vector has stopped.
2022-01-13T02:26:45.153893Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-13T02:26:45.154895Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-01-13T02:26:46.155677Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-01-13T02:26:47.156971Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-01-13T02:26:48.158228Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-01-13T02:26:48.544843Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 4
drwxr-xr-x 2 toby toby 4096 Jan 12 21:26 http_tarpit_buffer
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-fix -c config.toml
2022-01-13T02:27:07.943754Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-13T02:27:07.943797Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-13T02:27:07.944311Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-13T02:27:07.944328Z  INFO buffers::disk: Migrated old buffer data directory from '/tmp/vector/github-10430/http_tarpit_buffer' to '/tmp/vector/github-10430/http_tarpit_id' for 'http_tarpit' sink.
2022-01-13T02:27:07.976821Z  INFO vector::topology::running: Running healthchecks.
2022-01-13T02:27:07.976842Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-13T02:27:07.976852Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-13T02:27:07.976865Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-13T02:27:07.976894Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
2022-01-13T02:27:08.978664Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-01-13T02:27:09.979975Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-01-13T02:27:10.628838Z  INFO vector: Vector has stopped.
2022-01-13T02:27:10.628900Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-13T02:27:10.630025Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-01-13T02:27:10.981790Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-01-13T02:27:12.522956Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 4
drwxr-xr-x 2 toby toby 4096 Jan 12 21:27 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$

# Output from netcat if we run `vector-fix` a second time while there's something listening.
# This shows the messages actually making it out of the buffer after renaming the directory.
POST /foo HTTP/1.1
content-type: application/x-ndjson
user-agent: Vector/0.20.0 (x86_64-unknown-linux-gnu)
accept-encoding: identity
host: localhost:7777
content-length: 615

{"host":"consigliere","message":"line one, woohoo","source_type":"stdin","timestamp":"2022-01-12T20:36:57.451273579Z"}
{"host":"consigliere","message":"line two, yippeee","source_type":"stdin","timestamp":"2022-01-12T20:36:57.451286459Z"}
{"host":"consigliere","message":"line three, oh my","source_type":"stdin","timestamp":"2022-01-12T20:36:57.451291089Z"}
{"host":"consigliere","message":"line four, woooooow","source_type":"stdin","timestamp":"2022-01-12T20:36:57.451294469Z"}
{"host":"consigliere","message":"live five, phew, that was a lot","source_type":"stdin","timestamp":"2022-01-12T20:36:57.451298419Z"}
```

### Test 4

```shell
toby@consigliere:~/src/vector/bugs/github-10430$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 0
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-fix -c config.toml
2022-01-13T02:30:09.627541Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-13T02:30:09.627589Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-13T02:30:09.628111Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-13T02:30:09.660834Z  INFO vector::topology::running: Running healthchecks.
2022-01-13T02:30:09.660859Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-13T02:30:09.660870Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-13T02:30:09.660881Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-13T02:30:09.660906Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
^C2022-01-13T02:30:10.829960Z  INFO vector: Vector has stopped.
2022-01-13T02:30:10.830030Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-13T02:30:10.831161Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 4
drwxr-xr-x 2 toby toby 4096 Jan 12 21:30 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$ cat five-lines | ./vector-v0.18.1 -c config.toml
2022-01-13T02:30:20.909087Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info"
2022-01-13T02:30:20.909134Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-13T02:30:20.911037Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-13T02:30:20.943567Z  INFO vector::topology::running: Running healthchecks.
2022-01-13T02:30:20.943602Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-13T02:30:20.943615Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-13T02:30:20.943627Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-13T02:30:20.943658Z  INFO vector: Vector has started. debug="false" version="0.18.1" arch="x86_64" build_id="c4adb60 2021-11-30"
2022-01-13T02:30:20.943667Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
2022-01-13T02:30:20.943700Z  INFO vector::shutdown: All sources have finished.
2022-01-13T02:30:20.943704Z  INFO vector: Vector has stopped.
2022-01-13T02:30:20.943701Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-13T02:30:20.944743Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-01-13T02:30:21.945480Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-01-13T02:30:22.946740Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-01-13T02:30:23.289145Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 8
drwxr-xr-x 2 toby toby 4096 Jan 12 21:30 http_tarpit_buffer
drwxr-xr-x 2 toby toby 4096 Jan 12 21:30 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-fix -c config.toml
2022-01-13T02:30:33.136334Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-13T02:30:33.136376Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-13T02:30:33.136891Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-13T02:30:33.153853Z  WARN buffers::disk: Found both old and new buffers with data for 'http_tarpit' sink. This may indicate that you upgraded to 0.19.x prior to a regression being fixed which deals with disk buffer directory names. Using new buffers and ignoring old. See https://github.com/vectordotdev/vector/issues/10430 for more information.

You can suppress this message by renaming the old buffer data directory to something else.  Current path for old buffer data directory: /tmp/vector/github-10430/http_tarpit_buffer, suggested path for renaming: /tmp/vector/github-10430/http_tarpit_buffer_old old_buffer_record_count=5 old_buffer_size=565
2022-01-13T02:30:33.177038Z  INFO vector::topology::running: Running healthchecks.
2022-01-13T02:30:33.177058Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-13T02:30:33.177080Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-13T02:30:33.177079Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-13T02:30:33.177103Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
^C2022-01-13T02:30:35.531084Z  INFO vector: Vector has stopped.
2022-01-13T02:30:35.531145Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-13T02:30:35.532265Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 8
drwxr-xr-x 2 toby toby 4096 Jan 12 21:30 http_tarpit_buffer
drwxr-xr-x 2 toby toby 4096 Jan 12 21:30 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$
```

### Test 5

```shell
toby@consigliere:~/src/vector/bugs/github-10430$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 0
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-fix -c config.toml
2022-01-13T02:32:58.303181Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-13T02:32:58.303215Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-13T02:32:58.303763Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-13T02:32:58.336135Z  INFO vector::topology::running: Running healthchecks.
2022-01-13T02:32:58.336157Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-13T02:32:58.336187Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-13T02:32:58.336182Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-13T02:32:58.336211Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
^C2022-01-13T02:32:59.845171Z  INFO vector: Vector has stopped.
2022-01-13T02:32:59.845227Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit, stdin" time_remaining="59 seconds left"
2022-01-13T02:32:59.845222Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 4
drwxr-xr-x 2 toby toby 4096 Jan 12 21:32 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-v0.18.1 -c config.toml
2022-01-13T02:33:16.321126Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info"
2022-01-13T02:33:16.321171Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-13T02:33:16.323128Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-13T02:33:16.355748Z  INFO vector::topology::running: Running healthchecks.
2022-01-13T02:33:16.355778Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-13T02:33:16.355809Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-13T02:33:16.355806Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-13T02:33:16.355850Z  INFO vector: Vector has started. debug="false" version="0.18.1" arch="x86_64" build_id="c4adb60 2021-11-30"
2022-01-13T02:33:16.355860Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
^C2022-01-13T02:33:18.543378Z  INFO vector: Vector has stopped.
2022-01-13T02:33:18.543427Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-13T02:33:18.544521Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 8
drwxr-xr-x 2 toby toby 4096 Jan 12 21:33 http_tarpit_buffer
drwxr-xr-x 2 toby toby 4096 Jan 12 21:32 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-fix -c config.toml
2022-01-13T02:33:23.465565Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-13T02:33:23.465605Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-13T02:33:23.466142Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-13T02:33:23.480721Z  INFO buffers::disk: Archived old buffer data directory from '/tmp/vector/github-10430/http_tarpit_buffer' to '/tmp/vector/github-10430/http_tarpit_buffer_old' for 'http_tarpit' sink.
2022-01-13T02:33:23.503920Z  INFO vector::topology::running: Running healthchecks.
2022-01-13T02:33:23.503939Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-13T02:33:23.503956Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-13T02:33:23.503962Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-13T02:33:23.504032Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
^C2022-01-13T02:33:25.593636Z  INFO vector: Vector has stopped.
2022-01-13T02:33:25.593691Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-13T02:33:25.594791Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 8
drwxr-xr-x 2 toby toby 4096 Jan 12 21:33 http_tarpit_buffer_old
drwxr-xr-x 2 toby toby 4096 Jan 12 21:33 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$
```
