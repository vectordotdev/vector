## Test Results

### Test 1

```
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

```
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

```
toby@consigliere:~/src/vector/bugs/github-10430$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 0
toby@consigliere:~/src/vector/bugs/github-10430$ cat five-lines | ./vector-v0.18.1 -c config.toml
2022-01-12T20:36:57.422921Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info"
2022-01-12T20:36:57.422965Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-12T20:36:57.424898Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-12T20:36:57.451149Z  INFO vector::topology::running: Running healthchecks.
2022-01-12T20:36:57.451180Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-12T20:36:57.451202Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-12T20:36:57.451200Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-12T20:36:57.451242Z  INFO vector: Vector has started. debug="false" version="0.18.1" arch="x86_64" build_id="c4adb60 2021-11-30"
2022-01-12T20:36:57.451252Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
2022-01-12T20:36:57.451319Z  INFO vector::shutdown: All sources have finished.
2022-01-12T20:36:57.451323Z  INFO vector: Vector has stopped.
2022-01-12T20:36:57.451320Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-12T20:36:57.452304Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-01-12T20:36:58.453055Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-01-12T20:36:59.454362Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-01-12T20:37:00.455713Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-01-12T20:37:02.452742Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="54 seconds left"
2022-01-12T20:37:02.457005Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-01-12T20:37:04.150259Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 4
drwxr-xr-x 2 toby toby 4096 Jan 12 15:36 http_tarpit_buffer
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/http_tarpit_buffer/
total 20
-rw-r--r-- 1 toby toby 639 Jan 12 15:36 000005.log
-rw-r--r-- 1 toby toby  16 Jan 12 15:36 CURRENT
-rw-r--r-- 1 toby toby   0 Jan 12 15:36 LOCK
-rw-rw-r-- 1 toby toby 181 Jan 12 15:36 LOG
-rw-rw-r-- 1 toby toby  60 Jan 12 15:36 LOG.old
-rw-r--r-- 1 toby toby  50 Jan 12 15:36 MANIFEST-000004
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-fix -c config.toml
2022-01-12T20:38:42.594989Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-12T20:38:42.595035Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-12T20:38:42.595881Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-12T20:38:42.628482Z  INFO vector::topology::running: Running healthchecks.
2022-01-12T20:38:42.628522Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-12T20:38:42.628546Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-12T20:38:42.628537Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-12T20:38:42.628582Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
2022-01-12T20:38:42.628594Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
2022-01-12T20:38:43.630549Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-01-12T20:38:44.631852Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-01-12T20:38:45.633184Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-01-12T20:38:46.888297Z  INFO vector: Vector has stopped.
2022-01-12T20:38:46.888358Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-12T20:38:46.889472Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-01-12T20:38:47.634518Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
2022-01-12T20:38:50.636117Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: error trying to connect: tcp connect error: Connection refused (os error 111)
^C2022-01-12T20:38:51.611736Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 4
drwxr-xr-x 2 toby toby 4096 Jan 12 15:38 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/http_tarpit_id/
total 20
-rw-r--r-- 1 toby toby 734 Jan 12 15:38 000007.ldb
-rw-r--r-- 1 toby toby   0 Jan 12 15:38 000010.log
-rw-r--r-- 1 toby toby  16 Jan 12 15:38 CURRENT
-rw-r--r-- 1 toby toby   0 Jan 12 15:36 LOCK
-rw-rw-r-- 1 toby toby 181 Jan 12 15:38 LOG
-rw-rw-r-- 1 toby toby 324 Jan 12 15:38 LOG.old
-rw-r--r-- 1 toby toby  89 Jan 12 15:38 MANIFEST-000009
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-fix -c config.toml
2022-01-12T20:40:38.685877Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-12T20:40:38.685926Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-12T20:40:38.686703Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-12T20:40:38.716533Z  INFO vector::topology::running: Running healthchecks.
2022-01-12T20:40:38.716572Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-12T20:40:38.716585Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-12T20:40:38.716596Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-12T20:40:38.716629Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
2022-01-12T20:40:38.716645Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
^C2022-01-12T20:40:42.383044Z  INFO vector: Vector has stopped.
2022-01-12T20:40:42.383108Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-12T20:40:42.384231Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
^C2022-01-12T20:40:42.814978Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/bugs/github-10430$

# from window where netcat is running when we run `vector-fix` the second time:
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

```
toby@consigliere:~/src/vector/bugs/github-10430$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 0
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-fix -c config.toml
2022-01-12T20:42:10.501348Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-12T20:42:10.501398Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-12T20:42:10.502197Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-12T20:42:10.534476Z  INFO vector::topology::running: Running healthchecks.
2022-01-12T20:42:10.534514Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-12T20:42:10.534547Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-12T20:42:10.534545Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-12T20:42:10.534578Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
2022-01-12T20:42:10.534589Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
^C2022-01-12T20:42:12.197565Z  INFO vector: Vector has stopped.
2022-01-12T20:42:12.197615Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-12T20:42:12.198763Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 4
drwxr-xr-x 2 toby toby 4096 Jan 12 15:42 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$ cat five-lines | ./vector-v0.18.1 -c config.toml
2022-01-12T20:42:23.893572Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info"
2022-01-12T20:42:23.893620Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-12T20:42:23.895540Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-12T20:42:23.928619Z  INFO vector::topology::running: Running healthchecks.
2022-01-12T20:42:23.928650Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-12T20:42:23.928666Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-12T20:42:23.928670Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-12T20:42:23.928715Z  INFO vector: Vector has started. debug="false" version="0.18.1" arch="x86_64" build_id="c4adb60 2021-11-30"
2022-01-12T20:42:23.928724Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
2022-01-12T20:42:23.928780Z  INFO vector::shutdown: All sources have finished.
2022-01-12T20:42:23.928783Z  INFO vector: Vector has stopped.
2022-01-12T20:42:23.928783Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-12T20:42:23.929889Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
2022-01-12T20:42:24.930727Z  WARN sink{component_kind="sink" component_id=http_tarpit component_type=http component_name=http_tarpit}:request{request_id=0}: vector::sinks::util::retries: Retrying after error. error=Failed to make HTTP(S) request: channel closed
2022-01-12T20:42:28.930282Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="54 seconds left"
^C2022-01-12T20:42:30.817071Z  INFO vector: Vector has quit.
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 8
drwxr-xr-x 2 toby toby 4096 Jan 12 15:42 http_tarpit_buffer
drwxr-xr-x 2 toby toby 4096 Jan 12 15:42 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-fix -c config.toml
2022-01-12T20:42:49.559612Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-12T20:42:49.559657Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-12T20:42:49.560448Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-12T20:42:49.573613Z  WARN buffers::disk: 
this may indicate that you upgraded to 0.19.x prior to a regression being fixed which deals with disk buffer directory names.  see https://github.com/vectordotdev/vector/issues/10430 for more information about this situation. existing_record_count=5 existing_byte_size=565
2022-01-12T20:42:49.596575Z  INFO vector::topology::running: Running healthchecks.
2022-01-12T20:42:49.596612Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-12T20:42:49.596624Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-12T20:42:49.596639Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-12T20:42:49.596668Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
2022-01-12T20:42:49.596678Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
^C2022-01-12T20:43:14.242192Z  INFO vector: Vector has stopped.
2022-01-12T20:43:14.242285Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-12T20:43:14.243461Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 8
drwxr-xr-x 2 toby toby 4096 Jan 12 15:42 http_tarpit_buffer
drwxr-xr-x 2 toby toby 4096 Jan 12 15:42 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$
```

### Test 5

```
toby@consigliere:~/src/vector/bugs/github-10430$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 0
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-fix -c config.toml
2022-01-12T20:44:23.410817Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-12T20:44:23.410861Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-12T20:44:23.411672Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-12T20:44:23.463434Z  INFO vector::topology::running: Running healthchecks.
2022-01-12T20:44:23.463473Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-12T20:44:23.463500Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-12T20:44:23.463500Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-12T20:44:23.463530Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
2022-01-12T20:44:23.463541Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
^C2022-01-12T20:44:24.894674Z  INFO vector: Vector has stopped.
2022-01-12T20:44:24.894731Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-12T20:44:24.895894Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 4
drwxr-xr-x 2 toby toby 4096 Jan 12 15:44 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-v0.18.1 -c config.toml
2022-01-12T20:44:46.335761Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info"
2022-01-12T20:44:46.335805Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-12T20:44:46.337708Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-12T20:44:46.370628Z  INFO vector::topology::running: Running healthchecks.
2022-01-12T20:44:46.370658Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-12T20:44:46.370688Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-12T20:44:46.370684Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-12T20:44:46.370723Z  INFO vector: Vector has started. debug="false" version="0.18.1" arch="x86_64" build_id="c4adb60 2021-11-30"
2022-01-12T20:44:46.370732Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
^C2022-01-12T20:44:47.972404Z  INFO vector: Vector has stopped.
2022-01-12T20:44:47.972452Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-12T20:44:47.973543Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 8
drwxr-xr-x 2 toby toby 4096 Jan 12 15:44 http_tarpit_buffer
drwxr-xr-x 2 toby toby 4096 Jan 12 15:44 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$ ./vector-fix -c config.toml
2022-01-12T20:45:03.147077Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-12T20:45:03.147126Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-12T20:45:03.147953Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-12T20:45:03.185627Z  INFO vector::topology::running: Running healthchecks.
2022-01-12T20:45:03.185664Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-12T20:45:03.185688Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-12T20:45:03.185685Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-12T20:45:03.185723Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
2022-01-12T20:45:03.185735Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
^C2022-01-12T20:45:05.957422Z  INFO vector: Vector has stopped.
2022-01-12T20:45:05.957486Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-12T20:45:05.957573Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="stdin, http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/bugs/github-10430$ ls -l /tmp/vector/github-10430/
total 8
drwxr-xr-x 2 toby toby 4096 Jan 12 15:45 http_tarpit_buffer_old
drwxr-xr-x 2 toby toby 4096 Jan 12 15:45 http_tarpit_id
toby@consigliere:~/src/vector/bugs/github-10430$
```