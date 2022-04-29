## Test Results

### Test 1

```shell
toby@consigliere:~/src/vector/testing/github-10895$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/testing/github-10895$ ls -l /tmp/vector/github-10895
total 0
toby@consigliere:~/src/vector/testing/github-10895$ ./vector-v0.18.1 --config config.toml
2022-01-26T22:20:16.979995Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info"
2022-01-26T22:20:16.980039Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-26T22:20:16.981919Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-26T22:20:17.010091Z  INFO vector::topology::running: Running healthchecks.
2022-01-26T22:20:17.010121Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-26T22:20:17.010157Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-26T22:20:17.010155Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-26T22:20:17.010203Z  INFO vector: Vector has started. debug="false" version="0.18.1" arch="x86_64" build_id="c4adb60 2021-11-30"
2022-01-26T22:20:17.010213Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
^C2022-01-26T22:20:17.627876Z  INFO vector: Vector has stopped.
2022-01-26T22:20:17.627922Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-26T22:20:17.627952Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="stdin, http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/testing/github-10895$ ls -l /tmp/vector/github-10895
total 4
drwxr-xr-x 2 toby toby 4096 Jan 26 17:20 http_tarpit_buffer
toby@consigliere:~/src/vector/testing/github-10895$ ./vector-v0.19.1 --config config.toml
2022-01-26T22:20:28.399808Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-26T22:20:28.399854Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-26T22:20:28.400532Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-26T22:20:28.400543Z  INFO buffers::disk: Migrated old buffer data directory from '/tmp/vector/github-10895/http_tarpit_buffer' to '/tmp/vector/github-10895/http_tarpit_id' for 'http_tarpit' sink.
2022-01-26T22:20:28.430796Z  INFO vector::topology::running: Running healthchecks.
2022-01-26T22:20:28.430824Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-26T22:20:28.430847Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-26T22:20:28.430849Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-26T22:20:28.430879Z  INFO vector: Vector has started. debug="false" version="0.19.1" arch="x86_64" build_id="3cf70cf 2022-01-25"
2022-01-26T22:20:28.430891Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
^C2022-01-26T22:20:29.294516Z  INFO vector: Vector has stopped.
2022-01-26T22:20:29.294605Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-26T22:20:29.295668Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/testing/github-10895$ ls -l /tmp/vector/github-10895
total 4
drwxr-xr-x 2 toby toby 4096 Jan 26 17:20 http_tarpit_id
toby@consigliere:~/src/vector/testing/github-10895$
```

### Test 2

```shell
toby@consigliere:~/src/vector/testing/github-10895$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/testing/github-10895$ ls -l /tmp/vector/github-10895
total 0
toby@consigliere:~/src/vector/testing/github-10895$ ./vector-v0.18.1 --config config.toml
2022-01-26T22:20:57.412384Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info"
2022-01-26T22:20:57.412428Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-26T22:20:57.414343Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-26T22:20:57.447157Z  INFO vector::topology::running: Running healthchecks.
2022-01-26T22:20:57.447189Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-26T22:20:57.447211Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-26T22:20:57.447245Z  INFO vector: Vector has started. debug="false" version="0.18.1" arch="x86_64" build_id="c4adb60 2021-11-30"
2022-01-26T22:20:57.447255Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
2022-01-26T22:20:57.447245Z  INFO vector::topology::builder: Healthcheck: Passed.
^C2022-01-26T22:20:58.292801Z  INFO vector: Vector has stopped.
2022-01-26T22:20:58.292844Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-26T22:20:58.293936Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/testing/github-10895$ ls -l /tmp/vector/github-10895
total 4
drwxr-xr-x 2 toby toby 4096 Jan 26 17:20 http_tarpit_buffer
toby@consigliere:~/src/vector/testing/github-10895$ ./vector-pr --config config.toml
2022-01-26T22:21:06.698724Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-26T22:21:06.698761Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-26T22:21:06.699272Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-26T22:21:06.699295Z  INFO vector_buffers::disk: Migrated old buffer data directory from '/tmp/vector/github-10895/http_tarpit_buffer' to '/tmp/vector/github-10895/http_tarpit_id' for 'http_tarpit' sink.
2022-01-26T22:21:06.728976Z  INFO vector::topology::running: Running healthchecks.
2022-01-26T22:21:06.728997Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-26T22:21:06.729017Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-26T22:21:06.729027Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-26T22:21:06.729060Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
^C2022-01-26T22:21:07.388780Z  INFO vector: Vector has stopped.
2022-01-26T22:21:07.388822Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-26T22:21:07.389894Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/testing/github-10895$ ls -l /tmp/vector/github-10895
total 4
drwxr-xr-x 2 toby toby 4096 Jan 26 17:21 http_tarpit_id
toby@consigliere:~/src/vector/testing/github-10895$
```

### Test 3

```shell
toby@consigliere:~/src/vector/testing/github-10895$ ./create-clean-data-directories.sh
toby@consigliere:~/src/vector/testing/github-10895$ ls -l /tmp/vector/github-10895
total 0
toby@consigliere:~/src/vector/testing/github-10895$ ./vector-v0.19.1 --config config.toml
2022-01-26T22:21:41.620286Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-26T22:21:41.620331Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-26T22:21:41.621069Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-26T22:21:41.653647Z  INFO vector::topology::running: Running healthchecks.
2022-01-26T22:21:41.653675Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-26T22:21:41.653688Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-26T22:21:41.653697Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-26T22:21:41.653726Z  INFO vector: Vector has started. debug="false" version="0.19.1" arch="x86_64" build_id="3cf70cf 2022-01-25"
2022-01-26T22:21:41.653737Z  INFO vector::app: API is disabled, enable by setting `api.enabled` to `true` and use commands like `vector top`.
^C2022-01-26T22:21:42.482989Z  INFO vector: Vector has stopped.
2022-01-26T22:21:42.483035Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-26T22:21:42.484107Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/testing/github-10895$ ls -l /tmp/vector/github-10895
total 4
drwxr-xr-x 2 toby toby 4096 Jan 26 17:21 http_tarpit_id
toby@consigliere:~/src/vector/testing/github-10895$ ./vector-pr --config config.toml
2022-01-26T22:21:53.367043Z  INFO vector::app: Log level is enabled. level="vector=info,codec=info,vrl=info,file_source=info,tower_limit=trace,rdkafka=info,buffers=info"
2022-01-26T22:21:53.367080Z  INFO vector::app: Loading configs. paths=["config.toml"]
2022-01-26T22:21:53.367588Z  INFO vector::sources::stdin: Capturing STDIN.
2022-01-26T22:21:53.397431Z  INFO vector::topology::running: Running healthchecks.
2022-01-26T22:21:53.397453Z  INFO vector::topology::running: Starting source. key=stdin
2022-01-26T22:21:53.397473Z  INFO vector::topology::builder: Healthcheck: Passed.
2022-01-26T22:21:53.397480Z  INFO vector::topology::running: Starting sink. key=http_tarpit
2022-01-26T22:21:53.397517Z  INFO vector: Vector has started. debug="false" version="0.20.0" arch="x86_64" build_id="none"
^C2022-01-26T22:21:54.469681Z  INFO vector: Vector has stopped.
2022-01-26T22:21:54.469727Z  INFO source{component_kind="source" component_id=stdin component_type=stdin component_name=stdin}: vector::sources::stdin: Finished sending.
2022-01-26T22:21:54.470832Z  INFO vector::topology::running: Shutting down... Waiting on running components. remaining_components="http_tarpit" time_remaining="59 seconds left"
toby@consigliere:~/src/vector/testing/github-10895$ ls -l /tmp/vector/github-10895
total 4
drwxr-xr-x 2 toby toby 4096 Jan 26 17:21 http_tarpit_id
toby@consigliere:~/src/vector/testing/github-10895$
```
