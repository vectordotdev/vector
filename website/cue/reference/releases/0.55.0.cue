package metadata

releases: "0.55.0": {
	date:     "2026-04-20"
	codename: ""

	whats_next: []

	description: """
		The [COSE team](https://opensource.datadoghq.com/about/#the-community-open-source-engineering-team) is excited to announce version `0.55.0`!

		## Release highlights

		- New `windows_event_log` source that collects logs from Windows Event Log channels using the
		  native Windows Event Log API, with pull-mode subscriptions, bookmark-based checkpointing, and
		  configurable field filtering.
		- The `aws_s3` sink now supports Apache Parquet batch encoding. Events can be written as
		  Parquet columnar files with either an auto-generated native schema or a supplied `.schema`
		  file, and configurable compression (Snappy, ZSTD, GZIP, LZ4, or none).
		- The `azure_blob` sink re-gains first-class [Azure authentication](https://learn.microsoft.com/en-us/azure/storage/blobs/authorize-access-azure-active-directory):
		  Azure CLI, Managed Identity, Workload Identity, and Managed Identity-based Client Assertion
		  credential kinds are all supported again.
		- The `datadog_metrics` sink now defaults to the Series v2 endpoint (`/api/v2/series`) and
		  uses zstd compression for Series v2 and Sketches, which should yield smaller payloads
		  and more efficient batching and intake. A new `series_api_version` option (`v1` or `v2`)
		  is available to opt back to the legacy v1 endpoint; Series v1 continues to use zlib.
		- `vector top` is more trustworthy: per-output events for components with multiple output
		  ports are now shown in the correct `Events Out` column, and the `Memory Used` column
		  now reports `disabled` when the target Vector instance was started without
		  `--allocation-tracing` instead of a misleading `0`.
		- Better internal metrics for capacity planning and alerting:
		  - New source-send latency distributions (`source_send_latency_seconds`,
		    `source_send_batch_latency_seconds`) surface backpressure close to the source.
		  - Task-transform `utilization` no longer counts time spent waiting on downstream
		    components, giving a more representative view of transform saturation.
		  - Fixed a regression in buffer utilization metric tracking around underflow.
		- Fixed a performance regression in the `file` and `kubernetes_logs` sources that could
		  cause unexpectedly high CPU usage, introduced in 0.50.0.

		## Breaking Changes

		See the [0.55 upgrade guide](/highlights/2026-04-20-0-55-0-upgrade-guide/) for full details
		and migration steps. At a glance, you are affected if you:

		- query or tail the Vector observability API in any way: the API has moved from
		  GraphQL to gRPC. This includes `vector top`, `vector tap`, and anything that talked to
		  `/graphql` or the `/playground`. The HTTP `GET /health` endpoint is unchanged and continues
		  to serve Kubernetes HTTP probes as before.
		- set the top-level `headers` option on the `http` or `opentelemetry` sinks: it has been
		  removed.
		- use the `azure_logs_ingestion` sink with Client Secret credentials: `azure_credential_kind`
		  must now be set explicitly.
		"""

	changelog: [
		{
			type: "enhancement"
			description: #"""
				`vector` source: Implement standard gRPC health checking protocol (`grpc.health.v1.Health`)
				alongside the existing custom health check endpoint. This enables compatibility with standard
				tools like `grpc-health-probe` for Kubernetes and other orchestration systems.
				
				Issue: https://github.com/vectordotdev/vector/issues/23657
				"""#
			contributors: ["jpds"]
		},
		{
			type: "feat"
			description: #"""
				Added websocket support to the `nats` source and sink. The `url` field now supports both `ws://` and
				`wss://` protocols.
				"""#
			contributors: ["gedemagt"]
		},
		{
			type: "enhancement"
			description: #"""
				The `geoip` enrichment table now includes a `network` field containing the CIDR network associated with the lookup result, available for all database types (City, ISP/ASN, Connection-Type, Anonymous-IP).
				"""#
			contributors: ["naa0yama"]
		},
		{
			type: "enhancement"
			description: #"""
				The `opentelemetry` source now supports independent configuration of OTLP decoding for logs, metrics, and traces. This allows more granular
				control over which signal types are decoded, while maintaining backward compatibility with the existing boolean configuration.
				
				## Simple boolean form (applies to all signals)
				
				```yaml
				use_otlp_decoding: true  # All signals preserve OTLP format
				# or
				use_otlp_decoding: false # All signals use Vector native format (default)
				```
				
				## Per-signal configuration
				
				```yaml
				use_otlp_decoding:
				  logs: false     # Convert to Vector native format
				  metrics: false  # Convert to Vector native format
				  traces: true    # Preserve OTLP format
				```
				"""#
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				Fixed log message ordering on shutdown where `Vector has stopped.` was logged before components had finished draining, causing confusing output interleaved with `Waiting on running components` messages.
				
				A new `VectorStopping` event was added in the place of the `VectorStopped` event.
				"""#
			contributors: ["tronboto"]
		},
		{
			type: "enhancement"
			description: #"""
				Adds new fields to parsed dnstap data: `requestMessageSize` and `responseMessageSize`. It represents the size of the DNS message.
				"""#
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: #"""
				`graph.edge_attributes` can now be added to transforms and sinks to add attributes to edges in graphs generated using `vector graph`. Memory enrichment tables are also considered for graphs, because they can have inputs and outputs.
				"""#
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "enhancement"
			description: #"""
				`opentelemetry` source: Implemented header enrichment for OTLP metrics and traces. Unlike logs, which support enriching
				the event itself or its metadata, depending on `log_namespace` settings, for metrics and traces this setting is ignored
				and header values are added to the event metadata.
				
				Issue: https://github.com/vectordotdev/vector/issues/24619
				"""#
			contributors: ["ozanichkovsky"]
		},
		{
			type: "feat"
			description: #"""
				The `kafka` sink now supports trace events.
				"""#
			contributors: ["pstalmach"]
		},
		{
			type: "fix"
			description: #"""
				Fixed utilization for task transforms to not account for time spent when downstream
				is not polling. If the transform is frequently blocked on downstream components,
				the reported utilization should be lower.
				"""#
			contributors: ["gwenaskell"]
		},
		{
			type: "fix"
			description: #"""
				The `opentelemetry` source now logs an error if it fails to start up or during runtime.
				This can happen when the configuration is invalid, for example trying to bind to the wrong
				IP or when hitting the open file limit.
				"""#
			contributors: ["fbs"]
		},
		{
			type: "feat"
			description: #"""
				Re-introduced Azure authentication support to `azure_blob`, including Azure CLI, Managed Identity, Workload Identity, and Managed Identity-based Client Assertion authentication types.
				"""#
			contributors: ["jlaundry"]
		},
		{
			type: "chore"
			description: #"""
				If using the `azure_logs_ingestion` sink (added in Vector 0.54.0) with Client Secret credentials, add `azure_credential_kind = "client_secret_credential"` to your sink config (this was previously the default, and now must be explicitly configured).
				"""#
			contributors: ["jlaundry"]
		},
		{
			type: "fix"
			description: #"""
				Fixed regression in buffer utilization metric tracking around underflow.
				"""#
			contributors: ["bruceg"]
		},
		{
			type: "fix"
			description: #"""
				Reduced the memory usage in the `aggregate` transform where previous values were being held
				even if `mode` was not set to `Diff`.
				"""#
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Fixed `vector top` displaying per-output sent events in the wrong column (Bytes In instead of Events Out) for components with multiple output ports.
				"""#
			contributors: ["pront"]
		},
		{
			type: "enhancement"
			description: #"""
				The `datadog_metrics` sink now uses zstd compression when submitting metrics to the Series v2 (`/api/v2/series`) and Sketches endpoints. Series v1 continues to use zlib (deflate).
				"""#
			contributors: ["vladimir-dd"]
		},
		{
			type: "fix"
			description: #"""
				Fixed an issue in the `file`/`kubernetes_logs` source that could cause unexpectedly high CPU usage after the async file server migration.
				"""#
			contributors: ["fcfangcc"]
		},
		{
			type: "feat"
			description: #"""
				Add Apache Parquet batch encoding support for the `aws_s3` sink with flexible schema definitions.
				
				Events can now be encoded as Parquet columnar files with multiple schema input options:
				
				- **Native Parquet schema** — automatically generate a schema or supply `.schema` file
				- **Configurable compression** - (Snappy, ZSTD, GZIP, LZ4, None).
				
				Enable the `codecs-parquet` feature and configure `batch_encoding` with `codec = "parquet"` in the S3 sink configuration.
				"""#
			contributors: ["szibis", "petere-datadog"]
		},
		{
			type: "enhancement"
			description: #"""
				Bumped `kube` dependency from 0.93.0 to 3.0.1 and `k8s-openapi` from 0.22.0 to 0.27.0, adding support for Kubernetes API versions up to v1.35.
				"""#
			contributors: ["hligit"]
		},
		{
			type: "enhancement"
			description: #"""
				Added support for the ClickHouse `UUID` type in the ArrowStream format for the `clickhouse` sink. UUID columns are now automatically mapped to Arrow `Utf8` and cast by ClickHouse on insert.
				"""#
			contributors: ["benjamin-awd"]
		},
		{
			type: "fix"
			description: #"""
				`datadog_agent` source: Preserve `device` as a plain tag when decoding v2 series metrics,
				instead of incorrectly prefixing it as `resource.device`. This matches the v1 series behavior
				and fixes tag remapping for disk, SNMP, and other integrations that use the `device` resource type.
				"""#
			contributors: ["lisaqvu"]
		},
		{
			type: "enhancement"
			description: #"""
				The `datadog_metrics` sink now defaults to the Datadog series v2 endpoint (`/api/v2/series`) and
				exposes a new `series_api_version` configuration option (`v1` or `v2`) to control which endpoint is
				used. Set `series_api_version: v1` to fall back to the legacy v1 endpoint if needed.
				"""#
			contributors: ["vladimir-dd"]
		},
		{
			type: "fix"
			description: #"""
				Fixed the Datadog sink healthcheck endpoint computation to preserve site prefixes (e.g. `us3.`, `us5.`, `ap1.`) when deriving the API URL from intake endpoints. Previously, the healthcheck for site-specific endpoints like `https://http-intake.logs.us3.datadoghq.com` would incorrectly call `https://api.datadoghq.com` instead of `https://api.us3.datadoghq.com`, causing unintended cross-site egress traffic.
				"""#
			contributors: ["vladimir-dd"]
		},
		{
			type: "fix"
			description: #"""
				Fixed an incorrect source_lag_time_seconds measurement in sources that use `send_batch` with large event batches. When a batch was split into multiple chunks, the reference timestamp used to compute lag time was re-captured on each chunk send, causing the lag time for later chunks to be overstated by the amount of time spent waiting for the channel to accept earlier chunks. The reference timestamp is now captured once before iteration and shared across all chunks.
				"""#
			contributors: ["gwenaskell"]
		},
		{
			type: "chore"
			description: #"""
				The Vector observability API has been migrated from GraphQL to gRPC for improved
				performance, efficiency and maintainability. The `vector top` and `vector tap`
				commands continue to work as before, as they have been updated to use the new
				gRPC API internally. The gRPC service definition is available in
				[`proto/vector/observability.proto`](https://github.com/vectordotdev/vector/blob/master/proto/vector/observability.proto).
				
				Note: `vector top` and `vector tap` from version 0.55.0 or later are not
				compatible with Vector instances running earlier versions.
				
				- Remove the `api.graphql` and `api.playground` fields from your config. Vector
				  now rejects configs that contain them.
				
				- If you use `vector top` or `vector tap` with an explicit `--url`, remove the
				  `/graphql` path suffix:
				
				```bash
				# Old
				vector top --url http://localhost:8686/graphql
				
				# New (the gRPC API listens at the root)
				vector top --url http://localhost:8686
				```
				
				- The GraphQL API (HTTP endpoint `/graphql`, WebSocket subscriptions, and the
				  GraphQL Playground at `/playground`) has been removed. You can interact with
				  the new gRPC API using tools like
				  [grpcurl](https://github.com/fullstorydev/grpcurl):
				
				```bash
				# Check health (standard gRPC health check, compatible with Kubernetes gRPC probes)
				grpcurl -plaintext localhost:8686 grpc.health.v1.Health/Check
				
				# List components
				grpcurl -plaintext localhost:8686 vector.observability.v1.ObservabilityService/GetComponents
				
				# Stream events (tap) — limit and interval_ms are required and must be >= 1
				grpcurl -plaintext \
				  -d '{"outputs_patterns": ["*"], "limit": 100, "interval_ms": 500}' \
				  localhost:8686 vector.observability.v1.ObservabilityService/StreamOutputEvents
				```
				"""#
			contributors: ["pront"]
		},
		{
			type: "chore"
			description: #"""
				The `headers` option has been removed from the `http` and `opentelemetry` sinks.
				Use `request.headers` instead. On the `opentelemetry` sink, `request` is nested under
				`protocol`; see the [0.55 upgrade guide](/highlights/2026-04-20-0-55-0-upgrade-guide/)
				for examples. This option has been deprecated since v0.33.0.
				"""#
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				Sources now record the distribution metrics `source_send_latency_seconds` (measuring the time spent
				blocking on a single events chunk send operation on the output) and `source_send_batch_latency_seconds`
				(encompassing all chunks within a received events batch).
				"""#
			contributors: ["gwenaskell"]
		},
		{
			type: "enhancement"
			description: #"""
				`vector top` terminal UI now shows `disabled` in the Memory Used column when the connected Vector instance was not started with `--allocation-tracing`, instead of displaying misleading zeros. A new `GetAllocationTracingStatus` gRPC endpoint is queried on connect to determine the status.
				"""#
			contributors: ["pront"]
		},
		{
			type: "feat"
			description: #"""
				Added a new `windows_event_log` source that collects logs from Windows Event Log channels using the native Windows Event Log API with pull-mode subscriptions, bookmark-based checkpointing, and configurable field filtering.
				"""#
			contributors: ["tot19"]
		},
		{
			type: "fix"
			description: #"""
				Fixed Windows service state checks in `vector service start`/`stop`, and made `vector service stop` wait until the service reaches `Stopped`. Added `--stop-timeout` to `vector service stop` and `vector service uninstall`.
				"""#
			contributors: ["iMithrellas"]
		},
	]

	vrl_changelog: """
		### [0.32.0 (2026-04-16)]
		
		#### New Features
		
		- Added a new `encode_csv` function that encodes an array of values into a CSV-formatted string. This is the inverse of the existing `parse_csv` function and supports an optional single-byte delimiter (defaults to `,`).
		
		authors: armleth (https://github.com/vectordotdev/vrl/pull/1649)
		- Added `to_entries` and `from_entries` with jq-compatible behavior: `to_entries` supports both objects and arrays, and `from_entries` accepts `key`/`Key`/`name`/`Name` and `value`/`Value` aliases.
		
		authors: close2code-palm (https://github.com/vectordotdev/vrl/pull/1653)
		
		#### Enhancements
		
		- Added `except` parameter to `flatten` function to exclude specific keys from being flattened.
		
		authors: benjamin-awd (https://github.com/vectordotdev/vrl/pull/1682)
		
		#### Fixes
		
		- Fixed a bug where the REPL input validator was executing programs instead of only compiling them, causing functions with side effects (e.g. `http_request`) to run twice per submission.
		
		authors: prontidis (https://github.com/vectordotdev/vrl/pull/1701)
		
		
		### [0.31.0 (2026-03-05)]
		"""

	commits: [
		{sha: "bc30368b01670480b4e83bebdd9eb09ec78bd2e3", date: "2026-03-10 01:08:29 UTC", description: "make build vrl-docs work with released VRL version", pr_number: 24877, scopes: ["vdev"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 21, deletions_count: 19},
		{sha: "3f98053f5105d0bedbd9b1cb0f15917b22819e36", date: "2026-03-10 01:42:09 UTC", description: "use cross-strip tools when packaging RPMs for non-x86_64 targets", pr_number: 24873, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 29, deletions_count: 4},
		{sha: "0d462e06d4fd1088cd7054cd9ee7c49f91fe9b65", date: "2026-03-10 19:46:12 UTC", description: "increase timeouts for vdev compilation", pr_number: 24881, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "e66270f937998848a439252d46a160ad085b17c6", date: "2026-03-10 19:52:15 UTC", description: "add retries to address choco intermittent failures", pr_number: 24880, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 26, deletions_count: 5},
		{sha: "145d333fc0fc8a53a087c64c56775ff55612d4a8", date: "2026-03-10 20:36:38 UTC", description: "remove unused compilation timings workflow", pr_number: 24882, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 77},
		{sha: "685c31fff163575019ce315b4b88991b92e3a61f", date: "2026-03-10 18:09:30 UTC", description: "pin actions to sha", pr_number: 24884, scopes: ["ci"], type: "chore", breaking_change: false, author: "StepSecurity Bot", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "40143b32740e253be37b214cf9f860851f65e6e0", date: "2026-03-10 18:31:15 UTC", description: "pin image tags in Dockerfiles", pr_number: 24885, scopes: ["ci"], type: "chore", breaking_change: false, author: "StepSecurity Bot", files_count: 7, insertions_count: 11, deletions_count: 11},
		{sha: "b1c7d7bf7133c43e472d15b4e4345e571043d63a", date: "2026-03-10 22:59:33 UTC", description: "fix broken links", pr_number: 24886, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 15, insertions_count: 18, deletions_count: 18},
		{sha: "1cc1c25147bab5ead963ff8065086d3b5daa1b2f", date: "2026-03-11 00:13:05 UTC", description: "minor fixes to release template", pr_number: 24887, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 13, deletions_count: 13},
		{sha: "e1ecad3d67361dd38f074e71da45d4d90b3578bb", date: "2026-03-11 00:37:56 UTC", description: "add docs::warnings macro support and warn about no auth", pr_number: 24866, scopes: ["website"], type: "feat", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 15, deletions_count: 1},
		{sha: "909b08318dd309c8a59e1c40241be42f46e67db8", date: "2026-03-11 00:53:15 UTC", description: "0.54.0 post release steps", pr_number: 24888, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 51, insertions_count: 512, deletions_count: 144},
		{sha: "df396d18b6101f1af5a75bbc918accb9291a7a74", date: "2026-03-11 18:01:29 UTC", description: "minor fixes to release changelog", pr_number: 24893, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 4},
		{sha: "356409dba6cc78cdaa0d070941efd9c0d52b1579", date: "2026-03-11 23:53:27 UTC", description: "test_udp_syslog can overflow default size receive buffer", pr_number: 24878, scopes: ["unit tests"], type: "fix", breaking_change: false, author: "strophy", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "901a571712d7714f5ad38defb60b01dbabe43617", date: "2026-03-11 19:11:14 UTC", description: "show top level api configuration in API reference page", pr_number: 24894, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 1, deletions_count: 3},
		{sha: "202b65612ee4cc27a338fc16a94d880c01b8ace6", date: "2026-03-11 19:34:07 UTC", description: "fix docker warning when publishing", pr_number: 24895, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "2958f982b221060b68dcb63837823837d4c0600a", date: "2026-03-11 22:57:01 UTC", description: "fetch full history in changelog workflow so origin/master is available", pr_number: 24898, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "d112a1b6826d0a35ff27be40d949d09ee66b6a7e", date: "2026-03-11 23:17:02 UTC", description: "Support per-signal OTLP decoding config", pr_number: 24822, scopes: ["opentelemetry source"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 472, deletions_count: 24},
		{sha: "ff7164a43992728f3490e38dbf72e5125b162f50", date: "2026-03-12 00:55:04 UTC", description: "bump protoc version", pr_number: 24902, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "71d597f894f59ff6190913b1091a1c568fedba63", date: "2026-03-12 17:15:23 UTC", description: "remove unused Hugo shortcodes", pr_number: 24903, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 8, insertions_count: 0, deletions_count: 296},
		{sha: "40f94ee88a453a6a900a00606fc940d990c5143e", date: "2026-03-12 19:20:22 UTC", description: "bump VRL and resolve RUSTSEC-2021-0139", pr_number: 24908, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 11, insertions_count: 59, deletions_count: 94},
		{sha: "478d7c4561a1a8b2390e97973ec9315963dd0dfb", date: "2026-03-13 10:54:17 UTC", description: "bump kube from 0.93.0 to 3.0.1 and k8s-openapi from 0.22.0 to 0.27.0", pr_number: 24787, scopes: ["deps"], type: "chore", breaking_change: false, author: "Haitao Li", files_count: 8, insertions_count: 146, deletions_count: 140},
		{sha: "958ff6357a6ad700d592aabf53c6b161f5fbb9a0", date: "2026-03-12 21:54:25 UTC", description: "replace netlink-* crates with procfs", pr_number: 24765, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 148, deletions_count: 369},
		{sha: "d141e60bfcc923a6d2df34c16f49db5fc0734e3c", date: "2026-03-12 23:40:12 UTC", description: "deprecate azure_monitor_logs sink", pr_number: 24910, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "01c86e826a83175002b609b9a404325931f11cc1", date: "2026-03-13 21:24:47 UTC", description: "bump async-executor to 1.14.0", pr_number: 24919, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 8, deletions_count: 5},
		{sha: "891643edcb177b9cc1e8db8fb753b36ad07f9182", date: "2026-03-14 10:54:10 UTC", description: "Add Windows Event Log source", pr_number: 24305, scopes: ["new source"], type: "feat", breaking_change: false, author: "tot19", files_count: 26, insertions_count: 10298, deletions_count: 14},
		{sha: "4e958028e836a62080d70ac1099d7db635d5b739", date: "2026-03-14 02:38:46 UTC", description: "implement standard gRPC health checking protocol", pr_number: 24916, scopes: ["vector source"], type: "fix", breaking_change: false, author: "Jonathan Davies", files_count: 6, insertions_count: 139, deletions_count: 7},
		{sha: "2064a15aa1029aec49dd2b4c8b3a09a4b76ff9e7", date: "2026-03-13 22:55:11 UTC", description: "update dd-rust-license-tool from 1.0.5 to 1.0.6", pr_number: 24920, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 6, deletions_count: 1},
		{sha: "60cd0e1347d4ae680da806401a6c89be1a2c0536", date: "2026-03-14 00:09:55 UTC", description: "Bump rmp from 0.8.14 to 0.8.15 and resolve RUSTSEC-2024-0436", pr_number: 24922, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 3, deletions_count: 13},
		{sha: "da85874bd56ee88699778fb3ed3f4f847915581c", date: "2026-03-14 00:46:55 UTC", description: "bump aws crates and resolve RUSTSEC-2026-0002", pr_number: 24909, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 5, insertions_count: 277, deletions_count: 178},
		{sha: "59f53e2bc72caae239970b355b4edab14176cd17", date: "2026-03-14 01:11:33 UTC", description: "fix typos in rustdoc comments", pr_number: 24923, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 4, deletions_count: 4},
		{sha: "89bc15fc1815572000d1fd2e9e20c4b98116d1b2", date: "2026-03-14 05:17:33 UTC", description: "Bump lapin from 2.5.3 to 4.3.0", pr_number: 23316, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 183, deletions_count: 156},
		{sha: "51aebc8445fece5e0e7daf62603bf004b9dfb8ce", date: "2026-03-16 22:18:15 UTC", description: "bump undici from 7.18.2 to 7.24.1 in /website", pr_number: 24925, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "e3225205fa88d04c4d76d0e485f27dc771900693", date: "2026-03-17 06:31:01 UTC", description: "add UUID type support for ArrowStream", pr_number: 24856, scopes: ["clickhouse sink"], type: "enhancement", breaking_change: false, author: "Benjamin Dornel", files_count: 4, insertions_count: 43, deletions_count: 4},
		{sha: "7390fe0bd4922b6f804b7c59e56dac36747af86d", date: "2026-03-16 19:08:19 UTC", description: "document Cargo feature placement rules", pr_number: 24933, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 25, deletions_count: 0},
		{sha: "c27f6d6dd8acc01f57e93c53af4fad0138a07aa1", date: "2026-03-16 20:05:44 UTC", description: "update heim fork to remove unmaintained crates", pr_number: 24924, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 71, deletions_count: 214},
		{sha: "0d8bb9bea3418bd6993e08b456fc495050a42930", date: "2026-03-17 01:07:42 UTC", description: "bump pulsar from 6.3.1 to 6.7.0", pr_number: 24746, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 24, deletions_count: 23},
		{sha: "fd766a0738802e9dee2eae1e5d6f243ed9c69229", date: "2026-03-16 21:10:58 UTC", description: "make rdkafka use gssapi-vendored when publishing/testing", pr_number: 24912, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 18, deletions_count: 10},
		{sha: "d38425ad1302489d7420a9d24447224559a5a446", date: "2026-03-17 01:16:13 UTC", description: "bump regex from 1.11.2 to 1.12.3", pr_number: 24796, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 13, deletions_count: 13},
		{sha: "c2fc606306e10e02263d2982f20c8c17f69a143b", date: "2026-03-16 22:05:29 UTC", description: "check Cargo.lock for changes after cargo check/clippy", pr_number: 24935, scopes: ["vdev"], type: "feat", breaking_change: false, author: "Thomas", files_count: 6, insertions_count: 57, deletions_count: 12},
		{sha: "0573297e8ffcc1ad7a096f50f012b2efd0518c27", date: "2026-03-17 03:54:36 UTC", description: "Added feature flag for websockets in async-nats", pr_number: 24291, scopes: ["nats source", "nats sink"], type: "feat", breaking_change: false, author: "Jesper", files_count: 3, insertions_count: 6, deletions_count: 1},
		{sha: "3905b4f12067fb2508f0bcb031607da84623f17f", date: "2026-03-16 23:07:53 UTC", description: "Update README.md and cleanup unused commands", pr_number: 24936, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 7, insertions_count: 43, deletions_count: 95},
		{sha: "1eca246d5bf297afcc153b555c19ac5f9e9ff372", date: "2026-03-17 00:49:29 UTC", description: "replace derivative macro with standard Default derives on enums", pr_number: 24938, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 31, insertions_count: 74, deletions_count: 119},
		{sha: "a8730894c366cd5232353530fef4a4970cd6c307", date: "2026-03-17 19:00:09 UTC", description: "bump lz4_flex from 0.11.5 to 0.11.6", pr_number: 24939, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "9dc3d3e9f7d486ce363975f21b1e1b3b537cb46d", date: "2026-03-17 19:11:36 UTC", description: "reduce memory when not in `Diff` mode", pr_number: 24943, scopes: ["aggregate transform"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 89, deletions_count: 24},
		{sha: "42b844e76882fa5cbf1fd5cfafcf653101dc9ad0", date: "2026-03-17 19:13:06 UTC", description: "remove unused fs package", pr_number: 24945, scopes: ["website deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 0, deletions_count: 6},
		{sha: "218aa4d29b7b0922f604b7f6d8a3ebd0cc8e0931", date: "2026-03-17 20:01:18 UTC", description: "Add osv-scanner.toml and ignore MAL-2025-3174", pr_number: 24946, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "83bcf4cb297f85525f88a3cff1870ab8fbddd47b", date: "2026-03-17 20:23:49 UTC", description: "bump quinn-proto from 0.11.9 to 0.11.14", pr_number: 24926, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 12, deletions_count: 4},
		{sha: "dc9d8de0c8f2ff7a04a7373820f1ef652ed30520", date: "2026-03-18 03:29:20 UTC", description: "switch to v2 endpoint", pr_number: 24842, scopes: ["datadog_metrics sink"], type: "chore", breaking_change: false, author: "Vladimir Zhuk", files_count: 32, insertions_count: 382, deletions_count: 98},
		{sha: "1444b2bac009aa51343820dba7a36ce119dce56f", date: "2026-03-17 22:44:09 UTC", description: "Bump rdkafka from 0.38.0 to 0.39.0", pr_number: 24602, scopes: ["deps"], type: "chore", breaking_change: false, author: "zapdos26", files_count: 4, insertions_count: 22, deletions_count: 5},
		{sha: "089db6775500eb6cbdfa5f4b7e6069185a8d61c5", date: "2026-03-18 11:45:28 UTC", description: "add `network` CIDR field to lookup results", pr_number: 24576, scopes: ["enrichment tables"], type: "feat", breaking_change: false, author: "Naoki Aoyama", files_count: 5, insertions_count: 40, deletions_count: 8},
		{sha: "5011c5dfd20a0200c37a42a6c3001ae3df94ef4d", date: "2026-03-17 23:16:34 UTC", description: "bump docker/build-push-action from 6.18.0 to 6.19.2", pr_number: 24734, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "6527346294203cec7fb0d8b86c7fc98b26c14ff0", date: "2026-03-18 03:17:07 UTC", description: "bump aws-actions/configure-aws-credentials from 5.1.1 to 6.0.0", pr_number: 24733, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "b1a43a4ba24b0e76e57736953a0f6f9d0ee58328", date: "2026-03-18 04:52:26 UTC", description: "bump sysinfo from 0.37.2 to 0.38.2", pr_number: 24816, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 61, deletions_count: 39},
		{sha: "cfee4fb9d1853a5ae7db85e0bd5647ca7a7b57b0", date: "2026-03-18 00:54:19 UTC", description: "upgrade mold linker from 1.2.1/2.0.0 to 2.40.4", pr_number: 24947, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 3, deletions_count: 3},
		{sha: "fbb1e4b501d6081d72f864da891a98464e60d097", date: "2026-03-18 02:05:08 UTC", description: "Improve buffer utilization metric tracking", pr_number: 24911, scopes: ["buffers"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 66, deletions_count: 95},
		{sha: "95f439420ae1dea0df7fbd295e7f354b144360c8", date: "2026-03-18 18:25:48 UTC", description: "place per-output sent events in Events Out column", pr_number: 24951, scopes: ["api"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 8, deletions_count: 3},
		{sha: "30936678f92a03c827f9fd8506fa02693332f59b", date: "2026-03-19 03:34:43 UTC", description: "add support for edge_attributes in graph configuration", pr_number: 24593, scopes: ["cli"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 7, insertions_count: 505, deletions_count: 125},
		{sha: "a8f3a9b45e6d2f6a897995bce85e3ad9a9a13442", date: "2026-03-19 03:39:25 UTC", description: "expose message size when parsing dnstap data", pr_number: 24552, scopes: ["dnstap source"], type: "enhancement", breaking_change: false, author: "Ensar Sarajčić", files_count: 4, insertions_count: 51, deletions_count: 0},
		{sha: "f34e8d32140ab25024ff51863ed9b5038defdc80", date: "2026-03-19 17:13:57 UTC", description: "regenerate VRL docs with message size fields", pr_number: 24960, scopes: ["dnstap source"], type: "docs", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "483db702e3745f4b823edac33e1d76bf7060f964", date: "2026-03-19 22:16:35 UTC", description: "add libsasl2-2 runtime dep to Debian-based images", pr_number: 24962, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Vladimir Zhuk", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "5f0f03e181a031ee51a71e2bd9fcf0698f073a5b", date: "2026-03-19 23:13:14 UTC", description: "bump fakedata_generator from 0.5.0 to 0.7.1", pr_number: 24800, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 5, deletions_count: 5},
		{sha: "3c69a8b8a668c6d20ea57d6883a3771803893648", date: "2026-03-19 19:43:24 UTC", description: "instruct agents to use PR template when creating PRs", pr_number: 24965, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "f02424389cfa4abf8e1168b5b63fb227499c9bd4", date: "2026-03-19 19:54:48 UTC", description: "add Datadog static analysis workflow", pr_number: 24966, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 26, deletions_count: 0},
		{sha: "a7fae1f7ad17b51e3960850300b45955ac0952e0", date: "2026-03-19 20:27:12 UTC", description: "add code coverage workflow with Datadog upload", pr_number: 24964, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 54, deletions_count: 2},
		{sha: "37a48ae53eb0e5930f4f4506903eb5b10c613789", date: "2026-03-20 05:44:04 UTC", description: "bump cargo-lock from 10.1.0 to 11.0.1", pr_number: 24748, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 11, deletions_count: 37},
		{sha: "97dfb6c6a870a162d07827706e85f5ee2fe3b3a9", date: "2026-03-20 17:53:12 UTC", description: "bump const-str from 1.0.0 to 1.1.0", pr_number: 24815, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "18898e0149f43924a07554303cce3d0a4595eb5b", date: "2026-03-20 21:22:10 UTC", description: "bump quickcheck to 1.1.0 and use workspace dep", pr_number: 24970, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 8, insertions_count: 218, deletions_count: 31},
		{sha: "2097fc35d7dc8d3caee9607d563165374e312e21", date: "2026-03-21 03:12:17 UTC", description: "bump criterion from 0.7.0 to 0.8.2", pr_number: 24803, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 31, deletions_count: 9},
		{sha: "2c334d223578ea17d7ca5bbbf305f21c9b68a7e2", date: "2026-03-21 05:07:16 UTC", description: "Add support for traces in kafka sink", pr_number: 24639, scopes: ["kafka sink"], type: "enhancement", breaking_change: false, author: "Piotr Stalmach", files_count: 4, insertions_count: 160, deletions_count: 12},
		{sha: "c748d6f9e19632cb64f666058794aa2d9004926f", date: "2026-03-21 02:28:58 UTC", description: "use cargo nextest in coverage workflow to prevent test pollution", pr_number: 24973, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 4, deletions_count: 1},
		{sha: "4d8ca64dbbbc2434bd4a69a3d46705c3fad6a56c", date: "2026-03-23 21:53:30 UTC", description: "pin localstack image to SHA256 digest", pr_number: 24988, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "6ef3752d12be1e8de38449df30f953f75301f64f", date: "2026-03-23 20:52:03 UTC", description: "remove futures features and resolve RUSTSEC-2026-0058", pr_number: 24975, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 124, deletions_count: 166},
		{sha: "415059f727929adee4d88801ca760be2ac0b967a", date: "2026-03-23 23:19:24 UTC", description: "bump async-nats from 0.42.0 to 0.46.0", pr_number: 24974, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 6, deletions_count: 16},
		{sha: "05f7b4338b06ef7c8731cd8f391ca4e62f875b43", date: "2026-03-23 20:45:40 UTC", description: "add missing Windows source files to integration_windows filter", pr_number: 24992, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "f62a9ffa8729fb1e7e6d074d4d115c96f7f2740a", date: "2026-03-23 21:25:16 UTC", description: "ignore RUSTSEC-2026-0049 (rustls-webpki) until rustls can be upgraded", pr_number: 24986, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "aedd2a66cb27b26621fff2c6d7dbd53a199fe0fa", date: "2026-03-23 21:26:02 UTC", description: "add security warning to path template field", pr_number: 24983, scopes: ["file sink"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 4, deletions_count: 0},
		{sha: "93a9771b1824c848c8efdf7166638226f01f42d2", date: "2026-03-24 14:22:00 UTC", description: "replace GraphQL observability API with gRPC, remove async-graphql dependencies", pr_number: 24364, scopes: ["api"], type: "chore", breaking_change: true, author: "Pavlos Rontidis", files_count: 128, insertions_count: 3020, deletions_count: 16042},
		{sha: "f1d8a93402168a2c5daf1eab2f4584aa5762bac4", date: "2026-03-25 00:52:15 UTC", description: "correct stop/start state checks and add configurable stop timeout", pr_number: 24772, scopes: ["windows platform"], type: "fix", breaking_change: false, author: "iMithrellas", files_count: 3, insertions_count: 100, deletions_count: 32},
		{sha: "0122cf6ec8fe7cce20a2bb694db137e3d5a9038a", date: "2026-03-24 20:04:14 UTC", description: "remove deprecated `headers` option from http and opentelemetry sinks", pr_number: 24994, scopes: ["sinks"], type: "chore", breaking_change: true, author: "Thomas", files_count: 9, insertions_count: 5, deletions_count: 43},
		{sha: "8bac1dbdcd27550444ea3b73e9dd4ef36172b2d4", date: "2026-03-24 23:00:40 UTC", description: "switch series v2 and sketches to zstd compression", pr_number: 24956, scopes: ["datadog_metrics sink"], type: "chore", breaking_change: false, author: "Vladimir Zhuk", files_count: 8, insertions_count: 629, deletions_count: 277},
		{sha: "753cf9a7c03a5cef85a29f3561666d94e9084d8b", date: "2026-03-24 23:57:50 UTC", description: "consolidate reference documentation into detailed docs table in AGENTS.md", pr_number: 25036, scopes: ["internal docs"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 15, deletions_count: 22},
		{sha: "47b5b02b9fdd13b67294df65175e9eca99c4cd8b", date: "2026-03-25 00:36:30 UTC", description: "split deny check into optional (all) and required (licenses)", pr_number: 24990, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 38, deletions_count: 6},
		{sha: "ca9b29bc52b7c7ba857c337b25278edbad1c5e66", date: "2026-03-25 21:17:07 UTC", description: "bump serial_test from 3.2.0 to 3.4.0", pr_number: 25022, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 6},
		{sha: "a2918e6ac4b874dc7c67b558a1e2bc9fd2b4ae81", date: "2026-03-25 21:58:38 UTC", description: "bump quick-junit from 0.5.2 to 0.6.0", pr_number: 25032, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "209b252deed214144fd2a6d4ed6c5ea3a3e08821", date: "2026-03-25 23:11:04 UTC", description: "enforce and fix markdownlint rules", pr_number: 25038, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 82, insertions_count: 415, deletions_count: 407},
		{sha: "7dc3ce3f34de6b79c0f09b4e44fe0e9d87cc09c2", date: "2026-03-26 02:32:34 UTC", description: "add links to design issues", pr_number: 25046, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 3, deletions_count: 0},
		{sha: "4fba57e9c4d6561c126851df8c4065221b44e992", date: "2026-03-26 07:51:02 UTC", description: "allow to add headers to metadata for OpenTelemetry metrics and traces", pr_number: 24942, scopes: ["opentelemetry source"], type: "feat", breaking_change: false, author: "Oleksandr Zanichkovskyi", files_count: 6, insertions_count: 463, deletions_count: 47},
		{sha: "498d1daa38432a329d168a2eec97a212922e71bc", date: "2026-03-26 04:51:47 UTC", description: "improve CLA instructions", pr_number: 25039, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 12, deletions_count: 13},
		{sha: "bc1e6bdde6b07f51efd4764c65d75c32d056499c", date: "2026-03-26 23:10:51 UTC", description: "bump uuid from 1.18.1 to 1.22.0", pr_number: 25025, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 37, deletions_count: 15},
		{sha: "072f6284c83279cd920177b32114f10f8fd5164c", date: "2026-03-26 23:15:29 UTC", description: "bump snafu from 0.8.9 to 0.9.0", pr_number: 25029, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 34, deletions_count: 13},
		{sha: "041769359ba0a9d68017e10e6e84cb7a83b3df27", date: "2026-03-26 23:22:32 UTC", description: "bump picomatch from 2.3.1 to 2.3.2 in /website", pr_number: 25047, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "d0c6cea89661577079d3eec790e4c054772de7d6", date: "2026-03-26 23:25:49 UTC", description: "bump yaml from 1.10.2 to 1.10.3 in /website", pr_number: 25043, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "5fcf1516151f659b5832fc433a4e10a219e31c7b", date: "2026-03-26 23:26:25 UTC", description: "bump ipnet from 2.11.0 to 2.12.0", pr_number: 25030, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "09629f02d60614a38b20b49cb10acbce47a9f6cc", date: "2026-03-26 23:49:33 UTC", description: "bump docker/setup-buildx-action from 3.12.0 to 4.0.0", pr_number: 25003, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "3255b66b2e8a2fb7ec3412904b856a8349a236ab", date: "2026-03-26 23:49:57 UTC", description: "bump actions/cache from 5.0.3 to 5.0.4", pr_number: 25002, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9337c80bb9d2491a8d617df408c20a88385d719a", date: "2026-03-26 23:50:14 UTC", description: "bump docker/login-action from 3.7.0 to 4.0.0", pr_number: 25001, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "84abc68df859d58b2a7681757e52aca7a8094ad2", date: "2026-03-27 04:24:22 UTC", description: "bump bollard from 0.19.2 to 0.20.2", pr_number: 25026, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 9, deletions_count: 13},
		{sha: "6267fcc78d8cd347f6405ab03ca100b97944a5ef", date: "2026-03-27 06:31:32 UTC", description: "log error", pr_number: 24708, scopes: ["opentelemetry source"], type: "fix", breaking_change: false, author: "bas smit", files_count: 2, insertions_count: 10, deletions_count: 2},
		{sha: "e8526b2a3ef7ac257d0fb816f611cddbc494a478", date: "2026-03-27 06:32:43 UTC", description: "bump deadpool from 0.12.2 to 0.13.0", pr_number: 25014, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 23, deletions_count: 6},
		{sha: "af15a5a595f342038fd3368854fa75c74c31e932", date: "2026-03-27 05:09:12 UTC", description: "vanila `cargo build` should run on windows", pr_number: 24991, scopes: ["dev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 13, deletions_count: 7},
		{sha: "4e6b49d6951b1f6ed9e2851aec674577ccc4c2b2", date: "2026-03-27 20:47:55 UTC", description: "deduct downstream utilization on task transforms", pr_number: 24731, scopes: ["metrics"], type: "fix", breaking_change: false, author: "Yoenn Burban", files_count: 3, insertions_count: 221, deletions_count: 18},
		{sha: "a7450cdbba3eb86782d67bea39b81e5b641067d9", date: "2026-03-27 18:50:50 UTC", description: "bump tempfile from 3.23.0 to 3.27.0", pr_number: 25016, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 14, deletions_count: 14},
		{sha: "7066adccecc24fe005f1fb7c934bbda4b557ea6c", date: "2026-03-27 18:51:39 UTC", description: "bump proptest from 1.10.0 to 1.11.0", pr_number: 25021, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 7, deletions_count: 7},
		{sha: "3ca3374c5b0359a3839f6c8091e2c3632faec719", date: "2026-03-27 23:02:57 UTC", description: "bump rust_decimal from 1.39.0 to 1.40.0", pr_number: 24799, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c1f34ef803fe5365a5be54f46877590bc8c91ba6", date: "2026-03-28 00:44:05 UTC", description: "bump brace-expansion from 1.1.12 to 1.1.13 in /website", pr_number: 25056, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "d29a524ea05dcbb6f823121ffdff449ecd960673", date: "2026-03-28 00:49:17 UTC", description: "bump security-framework from 3.5.1 to 3.6.0", pr_number: 24763, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "89e5c343c631e36090d2543106b88685d7a1c908", date: "2026-03-28 00:55:57 UTC", description: "bump juliangruber/read-file-action from 1.1.7 to 1.1.8", pr_number: 25000, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "55be10e5860afd47656a67a08335bb17a1d641c8", date: "2026-03-30 20:14:44 UTC", description: "add missing `contents: read` permission to integration review jobs", pr_number: 25067, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "d906ba19487c0552be7557d49a3f1dbfcd2405d9", date: "2026-03-31 01:12:33 UTC", description: "upgrade rustls from 0.23.23 to 0.23.37", pr_number: 25075, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 25, deletions_count: 13},
		{sha: "6a50bd50c9d482511f5a2a21466a3427dd752263", date: "2026-03-31 01:27:02 UTC", description: "update security policy structure and content", pr_number: 25074, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 43, deletions_count: 59},
		{sha: "122cca4678172bbc0c067c65fcc909c6b7aa9ce6", date: "2026-03-31 01:51:55 UTC", description: "update GHCR cleanup workflow", pr_number: 25037, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 18, deletions_count: 14},
		{sha: "87ed519cc40011224208914f1dd79b7240f7e1d7", date: "2026-03-31 17:36:52 UTC", description: "consolidate advisory ignores with issue links", pr_number: 25076, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 4, deletions_count: 12},
		{sha: "84c7af2264deacb1afc0529671d53c8c6792bf88", date: "2026-03-31 17:41:49 UTC", description: "install vdev via setup action in ci-integration-review workflow", pr_number: 25077, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 10, deletions_count: 4},
		{sha: "d732259aaa6b5e156b001672ccf0e59be5e84802", date: "2026-03-31 18:08:30 UTC", description: "pin GitHub actions to full sha", pr_number: 25080, scopes: ["ci"], type: "chore", breaking_change: false, author: "StepSecurity Bot", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "0943dd938f247b0a1963e6bcb1a464ae49392c5f", date: "2026-03-31 22:54:02 UTC", description: "migrate from markdownlint-cli to markdownlint-cli2", pr_number: 25081, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 6, insertions_count: 13, deletions_count: 19},
		{sha: "50345d5929ecb45d74e1355e612f044c24ff4a93", date: "2026-03-31 22:27:15 UTC", description: "skip test runner pull, datadog-ci install, and checkout when int tests won't run", pr_number: 25068, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 80, deletions_count: 74},
		{sha: "24d8db42e01e28b590e5484fede9279908805901", date: "2026-04-01 02:04:57 UTC", description: "remove unused publish-homebrew workflow", pr_number: 25085, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 36},
		{sha: "df00c7eec91ecb35eb0a8db0249701562de7dc87", date: "2026-04-01 23:37:50 UTC", description: "pin Pulsar integration test image to 4.1.3", pr_number: 25097, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "639c924c733442bcc7443817d16ea0dd737ded54", date: "2026-04-02 04:24:01 UTC", description: "bump debian from `1d3c811` to `26f98cc` in /distribution/docker/debian in the docker-images group across 1 directory", pr_number: 24996, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "9dbdb0662b668867ac8db6e444ac710d4346a037", date: "2026-04-02 04:25:08 UTC", description: "bump distroless/static from `28efbe9` to `47b2d72` in /distribution/docker/distroless-static in the docker-images group across 1 directory", pr_number: 24998, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "3c879836d586695701e9cf60fed3e7ef4b408f2e", date: "2026-04-02 04:25:30 UTC", description: "bump debian from `1d3c811` to `26f98cc` in /distribution/docker/distroless-libc in the docker-images group across 1 directory", pr_number: 24997, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9f484c2b9c1c219f44c2fd5289c40e3a4b63f6e3", date: "2026-04-02 03:26:12 UTC", description: "prevent script injection in integration review workflow", pr_number: 25106, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 18, deletions_count: 2},
		{sha: "9055388bc0c89a56177f6df60de7a7e09fe74de1", date: "2026-04-02 17:48:09 UTC", description: "add repo-wide prettier config for YAML, JS, TS, and JSON", pr_number: 25082, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 226, insertions_count: 4028, deletions_count: 2779},
		{sha: "b59c9a25b899223b6633776eeafe2e67fe418640", date: "2026-04-02 19:32:25 UTC", description: "Add http_server source to semantic PR scope list", pr_number: 25078, scopes: ["ci"], type: "chore", breaking_change: false, author: "steveduan-IDME", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "1a78c93520ab25377b7162c9aa5756f29492f433", date: "2026-04-02 21:57:21 UTC", description: "fix Pulsar TLS integration tests with latest image", pr_number: 25099, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 5, deletions_count: 3},
		{sha: "dedb9667511691731c30c05235c319867d96580c", date: "2026-04-03 19:19:53 UTC", description: "pin npm transitive deps via package-lock.json and npm ci", pr_number: 25115, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 3449, deletions_count: 24},
		{sha: "04f3b782e95f044fc7307fe4f6230dc48793800b", date: "2026-04-03 18:23:45 UTC", description: "Reverse performance regression in buffer metrics", pr_number: 24995, scopes: ["buffers"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 215, deletions_count: 114},
		{sha: "51030d2aade15deae23297a1ed1994cb36c530e9", date: "2026-04-03 20:36:36 UTC", description: "make file source tests not flaky", pr_number: 24957, scopes: ["dev"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 17, deletions_count: 6},
		{sha: "2fe515ccd2dfc23d98b84300e4fd87bee27b35fa", date: "2026-04-03 22:13:12 UTC", description: "use helm-charts develop branch for K8s E2E tests", pr_number: 25118, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 44, deletions_count: 24},
		{sha: "b428aba5dd4f26285ce907212acae648bdb6d8df", date: "2026-04-03 23:39:14 UTC", description: "resolve build.rs HEAD path in worktrees", pr_number: 25120, scopes: ["dev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 18, deletions_count: 1},
		{sha: "738c1d7758024a074f4fbe9ba9fa6d6110b316d7", date: "2026-04-04 05:14:48 UTC", description: "run K8s E2E suite on a weekly schedule", pr_number: 25119, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "703590bff3c2e49aa722c493329c753b1943c4ff", date: "2026-04-06 17:25:55 UTC", description: "Migrate smp regression workflow auth from static secrets to oidc", pr_number: 25112, scopes: ["ci"], type: "chore", breaking_change: false, author: "Caleb Metz", files_count: 1, insertions_count: 27, deletions_count: 12},
		{sha: "2d6fea2dfe7536202c83305cff5021bfadc90442", date: "2026-04-07 09:29:11 UTC", description: "update check-spelling", pr_number: 25124, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jed Laundry", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "1152614bc73e74df27986d1bff7864a376025544", date: "2026-04-06 17:32:37 UTC", description: "collect K8s diagnostics on E2E test failure", pr_number: 25114, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 32, deletions_count: 1},
		{sha: "cab8fab16b2da5478d495cd37091b525f527d8ba", date: "2026-04-06 23:37:04 UTC", description: "bump the artifact group across 1 directory with 2 updates", pr_number: 24999, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 17, deletions_count: 17},
		{sha: "4b2353a35dd54b888cdee7d1ca70c8723e12873c", date: "2026-04-06 23:46:52 UTC", description: "bump nick-fields/retry from 3.0.2 to 4.0.0", pr_number: 25102, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 7, deletions_count: 7},
		{sha: "9e897388fbf772a23c48df93de923a05b2d3ecbe", date: "2026-04-07 04:05:11 UTC", description: "bump fast-xml-parser from 5.3.7 to 5.5.9 in /scripts/environment/npm-tools in the npm_and_yarn group across 1 directory", pr_number: 25127, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 113, deletions_count: 81},
		{sha: "0c5fd58228bfab0dd9f6e81a1dc651da2ea8e670", date: "2026-04-07 20:14:58 UTC", description: "improve dashboard column layout", pr_number: 25135, scopes: ["api top"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 8, deletions_count: 8},
		{sha: "02f281d6ad2786f8bbfad9ece820b8d9f4925571", date: "2026-04-07 23:38:29 UTC", description: "show \"disabled\" in Memory Used when allocation tracing is off", pr_number: 25138, scopes: ["api top"], type: "enhancement", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 65, deletions_count: 2},
		{sha: "256db9c12951987ea235a0c5a4a88898d5187642", date: "2026-04-08 19:32:50 UTC", description: "avoid unused-mut warning on Windows builds", pr_number: 25142, scopes: ["api top"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 3},
		{sha: "711b039fe7a8ddac2296dfe1e78685bb901795e6", date: "2026-04-08 21:30:51 UTC", description: "bump lodash from 4.17.23 to 4.18.1 in /website", pr_number: 25113, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "dcf98a7ccf54080a74626e9277ac5b2f8340ded8", date: "2026-04-08 21:37:57 UTC", description: "fix spell check CI failures", pr_number: 25144, scopes: [], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 3, deletions_count: 3},
		{sha: "30d9a58669556a9bc014ba0b2ea4f33f8e372d98", date: "2026-04-09 13:43:50 UTC", description: "Expand support for Azure authentication types", pr_number: 24729, scopes: ["azure_blob sink"], type: "feat", breaking_change: false, author: "Jed Laundry", files_count: 28, insertions_count: 1542, deletions_count: 342},
		{sha: "985f669ffae07c1104ae9cec1b097e74130e0341", date: "2026-04-09 09:50:38 UTC", description: "high CPU usage after async file server migration", pr_number: 25064, scopes: ["file source"], type: "fix", breaking_change: false, author: "fcfangcc", files_count: 5, insertions_count: 212, deletions_count: 11},
		{sha: "9c4abf31a91d888d0b4ca204643b0e6e1a559466", date: "2026-04-09 05:26:19 UTC", description: "emit VectorStopped after topology drains", pr_number: 25083, scopes: ["shutdown"], type: "fix", breaking_change: false, author: "tronboto", files_count: 3, insertions_count: 32, deletions_count: 10},
		{sha: "26eef13087e1a463efa4bdcc9d3919b13936e06c", date: "2026-04-09 00:26:27 UTC", description: "preserve `device` tag from v2 series resources", pr_number: 25146, scopes: ["datadog_agent source"], type: "fix", breaking_change: false, author: "Lisa Vu", files_count: 3, insertions_count: 69, deletions_count: 0},
		{sha: "b57d8b0ef0ec12a1645b556a6b1bf080dc66e50b", date: "2026-04-09 04:53:19 UTC", description: "bump basic-ftp from 5.2.0 to 5.2.1 in /scripts/environment/npm-tools in the npm_and_yarn group across 1 directory", pr_number: 25147, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "51f6fce6d850dc4c6833d910013a189119068734", date: "2026-04-09 21:35:17 UTC", description: "fix typo in 24532 changelog fragment", pr_number: 25155, scopes: ["shutdown"], type: "chore", breaking_change: false, author: "Jonathan Davies", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a7f6487c94b98766bdff802aac0cd1372b0311f6", date: "2026-04-13 21:07:40 UTC", description: "bump basic-ftp from 5.2.1 to 5.2.2 in /scripts/environment/npm-tools in the npm_and_yarn group across 1 directory", pr_number: 25170, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "5ab0ba0d27392c70995a3d5e182a8307e65c641f", date: "2026-04-13 18:37:29 UTC", description: "bump axios from 1.13.5 to 1.15.0 in /website", pr_number: 25172, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 8, deletions_count: 8},
		{sha: "480ca58f52a019454a36e9c9b754791224fcee57", date: "2026-04-13 19:22:13 UTC", description: "extract vector-vrl-doc-builder into unpublished crate", pr_number: 25034, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 10, insertions_count: 114, deletions_count: 58},
		{sha: "3112033384d47b0eae9fa36a1297dab32cac1f88", date: "2026-04-13 20:08:25 UTC", description: "convert TOML config examples to YAML", pr_number: 25163, scopes: ["website"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 68, insertions_count: 2096, deletions_count: 1829},
		{sha: "43f620435a353b048c40661f59a2d2d18f2419ed", date: "2026-04-13 20:24:37 UTC", description: "fix panic in allocation tracing dealloc path", pr_number: 25136, scopes: ["observability"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 192, deletions_count: 13},
		{sha: "c6574fd66c328ef57fba27d2ad5422666c3e18eb", date: "2026-04-13 20:26:51 UTC", description: "replace custom Health RPC with standard gRPC health service", pr_number: 25139, scopes: ["api"], type: "enhancement", breaking_change: true, author: "Pavlos Rontidis", files_count: 16, insertions_count: 119, deletions_count: 77},
		{sha: "44637c3056c8850752e6f1c6aef8ab5ead810b0f", date: "2026-04-14 19:04:29 UTC", description: "bump datadog-ci from 5.12.0 to 5.13.0", pr_number: 25187, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 145, deletions_count: 2278},
		{sha: "16458a84ba60660bbdc4258e3245981102da2921", date: "2026-04-14 19:48:09 UTC", description: "bump follow-redirects from 1.15.11 to 1.16.0 in /website", pr_number: 25189, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "e32661adb9138ff721d882f059f35eaf792d355c", date: "2026-04-15 02:53:04 UTC", description: "add Parquet encoder with schema_file and auto infer schema support", pr_number: 25156, scopes: ["aws_s3 sink"], type: "feat", breaking_change: false, author: "Peter Ehikhuemen", files_count: 21, insertions_count: 1818, deletions_count: 43},
		{sha: "51121047d3201394c2958e4a66696f3aef2d3f11", date: "2026-04-15 21:25:02 UTC", description: "bump rsa from 0.9.3 to 0.9.10", pr_number: 25198, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 10, deletions_count: 9},
		{sha: "dac7c31501defdf451f9422b6a569358fb5ec19e", date: "2026-04-15 21:53:42 UTC", description: "bump rustls-webpki 0.103.10 to 0.103.12 (RUSTSEC-2026-0099)", pr_number: 25200, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 5, deletions_count: 3},
		{sha: "6818b9c348cd4c3edd39f9f303f66362676e47a3", date: "2026-04-15 22:22:40 UTC", description: "add work-in-progress label workflow for docs PRs", pr_number: 24950, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 68, deletions_count: 0},
		{sha: "2d14034bf8115764c52636a59ec298d7444a4d28", date: "2026-04-15 22:52:21 UTC", description: "override smol-toml to 1.6.1 for markdownlint-cli2", pr_number: 25202, scopes: ["deps"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 8, deletions_count: 3},
		{sha: "08c9f1e6b54cf1b30e88d536f3545e74ff193c0f", date: "2026-04-15 23:48:46 UTC", description: "bump VRL and add feature for enable_crypto_functions", pr_number: 25205, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 2, deletions_count: 1},
		{sha: "a1469590bed6996b1eb7001248232b0aa48cd777", date: "2026-04-16 00:06:08 UTC", description: "bump rand 0.10.0 to 0.10.1 and 0.9.2 to 0.9.4", pr_number: 25204, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 33, deletions_count: 32},
		{sha: "5ba8405bf48cd64509bf02388c7901451bbe67f4", date: "2026-04-16 00:20:05 UTC", description: "bump version to 0.3.1", pr_number: 25206, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "ce651754d4e2eb134ee9ab2fe85a8a3a5ed923a0", date: "2026-04-16 17:16:53 UTC", description: "add source latency metric and fix source lag time on large batches", pr_number: 24987, scopes: ["sources"], type: "feat", breaking_change: false, author: "Yoenn Burban", files_count: 7, insertions_count: 122, deletions_count: 28},
		{sha: "8138c4593455dd284475ed4ed61fe55ee176bd34", date: "2026-04-16 19:27:11 UTC", description: "update DD RUM/Logs config to cose site", pr_number: 25213, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 9, deletions_count: 2},
		{sha: "61b4563fc60211feba19dcbf9296872b8640ec94", date: "2026-04-16 20:19:11 UTC", description: "fix secrets in static analysis workflow", pr_number: 25208, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 6, deletions_count: 3},
		{sha: "467ef876a286df316ba273e4ad1779f1b08ba392", date: "2026-04-17 03:18:13 UTC", description: "preserve site prefix when computing API endpoint", pr_number: 25211, scopes: ["datadog_common sink"], type: "fix", breaking_change: false, author: "Vladimir Zhuk", files_count: 2, insertions_count: 32, deletions_count: 1},
		{sha: "9efe09d254fdad82f5d1239e3eae28348390ddfa", date: "2026-04-17 04:34:41 UTC", description: "modernize cross ubuntu bootstrap script", pr_number: 25215, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 43, deletions_count: 24},
		{sha: "8790fa07177e93242395746b8328e2cd2e88b440", date: "2026-04-17 21:32:34 UTC", description: "split VERSION-dependent packaging targets into Makefile.packaging", pr_number: 25101, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 135, deletions_count: 108},
		{sha: "35351b9cc9ae7dffdf1a461203137905f987a8b5", date: "2026-04-18 01:22:12 UTC", description: "replace native encoding fixture patch files with cfg-gated code", pr_number: 24971, scopes: ["codecs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 14, insertions_count: 130, deletions_count: 278},
		{sha: "604866d560deea7a14cbfa767a8ca99eee07089a", date: "2026-04-20 22:58:40 UTC", description: "use parent site for RUM to resolve intake host", pr_number: 25224, scopes: ["website"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "bdcf7476c55141ecf535cec6caafea7b2cc3d16d", date: "2026-04-20 23:43:03 UTC", description: "bump dorny/paths-filter from 3.0.2 to 4.0.1", pr_number: 25105, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "c2022541ee792988c465ca9a76b93a0f54ea4e00", date: "2026-04-20 23:43:44 UTC", description: "add greptimedb v0 support deprecation entry", pr_number: 25226, scopes: ["deprecations"], type: "docs", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "0f7574ad33ce07243ab8f9d6399c5e6c2f451af1", date: "2026-04-21 00:49:34 UTC", description: "fix panic in allocation tracing dealloc path", pr_number: 25222, scopes: ["observability"], type: "revert", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 13, deletions_count: 192},
		{sha: "3ca3e61ec8b8cd89be836b47346343069b229398", date: "2026-04-21 01:11:31 UTC", description: "use CUE raw multi-line strings in release generator", pr_number: 25228, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 5, deletions_count: 2},
	]
}
