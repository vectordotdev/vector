package metadata

releases: "0.47.0": {
	date:     "2025-05-20"
	codename: ""

	whats_next: []

	description: """
		The Vector team is excited to announce version `0.47.0`!

		Release highlights:

		- The `opentelemetry` source now supports metrics ingestion.
		- A new `window` transform has been introduced which enables log noise reduction by filtering out events when the system is in a healthy state.
		- A new `mqtt` source is now available, enabling ingestion from MQTT brokers.
		- The `datadog_logs` sink now supports a new `conforms_as_agent` option to format logs like the Datadog Agent, ensuring compatibility with reserved fields.
		"""

	vrl_changelog: """
		VRL was updated to `v0.24.0`. This includes the following changes:

		#### Enhancements

		- The `encode_gzip`, `decode_gzip`, `encode_zlib`, and `decode_zlib` methods now use the [zlib-rs](https://github.com/trifectatechfoundation/zlib-rs) backend.
			which is much faster than the previous backend `miniz_oxide`.

		- The `decode_base64`, `encode_base64`, and `decode_mime_q` functions now use the SIMD backend.
			which is faster than the previous backend.

		#### Fixes

		- Add BOM stripping logic to the parse_json function.
		"""

	changelog: [
		{
			type: "feat"
			description: """
				The `opentelemetry` source now supports metrics ingestion.
				"""
			contributors: ["cmcmacs"]
		},
		{
			type: "feat"
			description: """
				Add a new `window` transform, a variant of ring buffer or backtrace logging implemented as a sliding window.
				Allows for reduction of log volume by filtering out logs when the system is healthy, but preserving detailed
				logs when they are most relevant.
				"""
			contributors: ["ilinas"]
		},
		{
			type: "feat"
			description: """
				Add a new `mqtt` source enabling Vector to receive logs from a MQTT broker.
				"""
			contributors: ["mladedav", "pront", "StormStake"]
		},
		{
			type: "feat"
			description: """
				Add support for rendering to Mermaid format in `vector graph`
				"""
			contributors: ["Firehed"]
		},
		{
			type: "fix"
			description: """
				Fix a Vector crash that occurred when the internal metrics generated too many groups by increasing groups max limit from 128 to 256.
				"""
			contributors: ["triggerhappy17"]
		},
		{
			type: "feat"
			description: """
				Allow users to specify AWS authentication and the AWS service name for HTTP sinks to support AWS API endpoints that require SigV4.
				"""
			contributors: ["johannesfloriangeiger"]
		},
		{
			type: "feat"
			description: """
				Add support for fluentd forwarding over a Unix socket
				"""
			contributors: ["tustvold"]
		},
		{
			type: "enhancement"
			description: """
				Zlib compression and decompression are now more efficient by using [zlib-rs](https://github.com/trifectatechfoundation/zlib-rs).
				"""
			contributors: ["JakubOnderka"]
		},
		{
			type: "feat"
			description: """
				Add ACK support to message buffering feature of `websocket_server` sink, allowing this component to cache latest received messages per client.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "chore"
			description: """
				Add a new `extra_headers` option to `greptimedb_logs` sink configuration to set additional headers for outgoing requests.
				
				Change `greptimedb_logs` sink default content type to `application/x-ndjson` to match the default content type of `greptimedb` sink.
				If you use the greptimedb version v0.12 or earlier, you need to set the content type to `application/json` in the sink configuration.
				
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
				"""
			contributors: ["greptimedb"]
		},
		{
			type: "feat"
			description: """
				Introduce a configuration option in the StatsD source: `convert_to` of type `ConversionUnit`. By default, timing values in milliseconds (`ms`) are converted to seconds (`s`). Users can set `convert_to` to `milliseconds` to preserve the original millisecond values.
				"""
			contributors: ["devkoriel"]
		},
		{
			type: "fix"
			description: """
				Fix `file` source bug where known small files were not deleted after the specified `remove_after_secs`.
				"""
			contributors: ["linw1995"]
		},
		{
			type: "fix"
			description: """
				Fix an AWS authentication bug where `region` was missing from the `STS` authentication endpoint.
				"""
			contributors: ["cahartma"]
		},
		{
			type: "fix"
			description: """
				Increase the max event size for `aws_cloudwatch_logs` sink to ~1MB.
				"""
			contributors: ["cahartma"]
		},
		{
			type: "feat"
			description: """
				The `address` field is now available within VRL scripts when using the `auth.strategy.custom` authentication method.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: """
				Add support for the `Sec-WebSocket-Protocol` header in the `websocket_server` sink to better accommodate clients that require it.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "enhancement"
			description: """
				Reduce unnecessary buffer reallocation when using `framing.method = length_delimited` in sinks for significantly improved performance with large (more than 10MB) batches.
				"""
			contributors: ["Ilmarii"]
		},
		{
			type: "feat"
			description: """
				The redis sink now supports any input event type that the configured encoding supports. It previously only supported log events.
				"""
			contributors: ["ynachi"]
		},
		{
			type: "fix"
			description: """
				Fix a `kubernetes source` bug where `use_apiserver_cache=true` but there is no `resourceVersion=0` parameter in list request. Per [this issue](https://github.com/kube-rs/kube/issues/1743), when `resourceVersion =0` and `!page_size.is_none` in`ListParams`, the parameter `resourceVersion=0` will be ignored by `kube-rs` sdk. If no parameter `resourceVersion` passed to the apiserver, the apiserver will list pods from ETCD instead of in memory cache.
				"""
			contributors: ["xiaozongyang"]
		},
		{
			type: "feat"
			description: """
				Add `timeout` config option to the `healthcheck` sink configuration. Previously it was hardcoded to 10 seconds across all components, but now it can be configured per component.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "enhancement"
			description: """
				The [enrichment functions](https://vector.dev/docs/reference/vrl/functions/#enrichment-functions) now support bounded date range filtering using optional `from` and `to` parameters. There are no changes to the function signatures.
				"""
			contributors: ["nzxwang"]
		},
		{
			type: "feat"
			description: """
				Add `wildcard_matching` global config option to set wildcard matching mode for inputs. Relaxed mode allows configurations with wildcards that do not match any inputs to be accepted without causing an error.
				
				Example config:
				
				```yaml
				wildcard_matching: relaxed
				
				sources:
				  stdin:
				    type: stdin
				
				# note - no transforms
				
				sinks:
				  stdout:
				    type: console
				    encoding:
				      codec: json
				    inputs:
				      - "runtime-added-transform-*"
				
				```
				"""
			contributors: ["simplepad"]
		},
		{
			type: "fix"
			description: """
				Fix a bug that allows DNS records with an IPv6 prefix length greater than 128 to be transmitted; invalid prefixes are now rejected during parsing.
				"""
			contributors: ["wooffie"]
		},
		{
			type: "fix"
			description: """
				Add checks to prevent invalid timestamps operations during DNS tap parsing; such operations are now validated to ensure correctness.
				"""
			contributors: ["wooffie"]
		},
		{
			type: "enhancement"
			description: """
				Files specified in the `file` and `files` fields of `remap` transforms are now watched when `--watch-config` is enabled. Changes to these files automatically trigger a configuration reload, so there's no need to restart Vector.
				"""
			contributors: ["nekorro"]
		},
		{
			type: "enhancement"
			description: """
				The `amqp` sink now supports setting the `priority` for messages. The value can be templated to an integer 0-255 (inclusive).
				"""
			contributors: ["aramperes"]
		},
		{
			type: "fix"
			description: """
				Add an option in the `datadog_logs` sink to allow Vector to mutate the record to conform to the
				protocol used by the Datadog Agent itself. To enable, use the `conforms_as_agent` option or have the
				appropriate agent header (`DD-PROTOCOL: agent-json`) within the additional HTTP Headers list.
				
				Any top-level fields that use Datadog-reserved keywords are moved into a new object named `message`. If `message` doesn’t exist, it is created first. For example:
				
				```json
				{
				  "key1": "value1",
				  "key2": { "key2-1" : "value2" },
				  "message" : "Hello world",
				  ... rest of reserved fields
				}
				```
				
				will be modified to:
				
				```json
				{
				  "message" : {
				    "message" : "Hello world",
				    "key1": "value1",
				    "key2": { "key2-1" : "value2" }
				  },
				  ... rest of reserved fields
				}
				```
				"""
			contributors: ["graphcareful"]
		},
		{
			type: "fix"
			description: """
				Fix a bug in the `datadog_logs` sink where the content of the log message is dropped when logs namespacing is enabled.
				"""
			contributors: ["graphcareful"]
		},
		{
			type: "fix"
			description: """
				Fix misleading error message for invalid field name in gelf encoder.
				"""
			contributors: ["mprasil"]
		},
		{
			type: "enhancement"
			description: """
				Add `deferred.max_age_secs` and `deferred.queue_url` options to the `aws_s3` and `aws_sqs` sinks, to automatically
				route older event notifications to a separate queue, allowing prioritized processing of recent files.
				"""
			contributors: ["akutta"]
		},
	]

	commits: [
		{sha: "37803453653444ce1b210b66cdc1e64e500a970f", date: "2025-04-07 17:59:45 UTC", description: "Bump indexmap from 2.8.0 to 2.9.0", pr_number: 22814, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 27, deletions_count: 27},
		{sha: "bb3680636d9ae058d608999f0c4424e3e7f1d9f7", date: "2025-04-07 18:28:41 UTC", description: "Bump smallvec from 1.14.0 to 1.15.0", pr_number: 22817, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "1547c15f4c7bebae56d0e8d595ece5789be3b54e", date: "2025-04-07 22:43:10 UTC", description: "Bump openssl from 0.10.71 to 0.10.72 in the cargo group", pr_number: 22802, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "e74b60cd8d468b19c683d7152699067d1c043781", date: "2025-04-07 23:11:34 UTC", description: "Bump crossterm from 0.28.1 to 0.29.0", pr_number: 22815, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 59, deletions_count: 25},
		{sha: "92e2a2f6a9b701e8e6d501cb2452a5850d5e19f5", date: "2025-04-07 19:43:58 UTC", description: "update tokio to fix RUSTSEC-2025-0023", pr_number: 22820, scopes: ["deps"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "081b3044ea8e11763374e49958208e618ee54408", date: "2025-04-08 01:06:44 UTC", description: "Bump vrl from `7020ba2` to `048253d`", pr_number: 22812, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 68, deletions_count: 48},
		{sha: "b0823b6364c7d6702a84cae1698b5b6b51b0664d", date: "2025-04-07 21:20:10 UTC", description: "update interval changed to 'monthly'", pr_number: 22822, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "5d9297b35bd9d132292663bc6623c9c34c3efef2", date: "2025-04-08 12:41:58 UTC", description: "add documentation for Lz4 encode and decode features in vrl", pr_number: 22702, scopes: ["external"], type: "docs", breaking_change: false, author: "James Lamb", files_count: 2, insertions_count: 62, deletions_count: 0},
		{sha: "5ccca8ae3098c1aefc65e7222e60ea4819f0b152", date: "2025-04-07 23:17:57 UTC", description: "update CODEOWNERS files for cue docs PRs", pr_number: 22824, scopes: ["administration"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "60048c814bffac01ac49b84045bce37ad5b4190b", date: "2025-04-08 19:55:19 UTC", description: "fix website branch name", pr_number: 22830, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 14, deletions_count: 13},
		{sha: "cf9185f1e47bad13ae6ffbe14f2f2cbcb0c051f4", date: "2025-04-09 04:15:11 UTC", description: "Use zlib-rs for much faster zlib decoding/encoding", pr_number: 22533, scopes: ["deps"], type: "enhancement", breaking_change: false, author: "Jakub Onderka", files_count: 6, insertions_count: 32, deletions_count: 9},
		{sha: "e1491d84812a1892053d57482bc322bb061bad90", date: "2025-04-09 00:03:21 UTC", description: "Refactor reduce transform logic", pr_number: 22829, scopes: ["reduce transform"], type: "enhancement", breaking_change: false, author: "ArunPiduguDD", files_count: 2, insertions_count: 40, deletions_count: 26},
		{sha: "7d9242aaa13814b2eaf9157b3858ce6bc5d59572", date: "2025-04-09 00:26:01 UTC", description: "update homebrew repo link", pr_number: 22832, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "1de7e9d6703487c23e74312bafd130ff324dd3ff", date: "2025-04-09 00:57:41 UTC", description: "#22827 missing region error when using AWS credentials file auth", pr_number: 22831, scopes: ["auth"], type: "fix", breaking_change: false, author: "Casey Hartman", files_count: 2, insertions_count: 21, deletions_count: 1},
		{sha: "6a59a873a520bbe736f23d09108f019ba29a6d88", date: "2025-04-09 05:10:57 UTC", description: "Bump sysinfo from 0.32.1 to 0.34.2", pr_number: 22816, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 15, deletions_count: 8},
		{sha: "c2e082052c1c637d66ecdc8b5b2a5e30b2749d77", date: "2025-04-09 01:39:38 UTC", description: "cherry pick release prep commit", pr_number: 22836, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 30, insertions_count: 398, deletions_count: 96},
		{sha: "050743b0134ecc2e88436f903c705aa31b8b6cf1", date: "2025-04-09 02:25:51 UTC", description: "cargo vdev build manifests", pr_number: 22833, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 18, insertions_count: 37, deletions_count: 37},
		{sha: "668caf89b64c5c412631909bfbf971098d5747ed", date: "2025-04-10 00:04:10 UTC", description: "add ACK support for message buffering", pr_number: 22540, scopes: ["websocket_server sink"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 6, insertions_count: 652, deletions_count: 47},
		{sha: "906a9bd900e7df402417a5544392b8792bf34c86", date: "2025-04-09 18:19:35 UTC", description: "PR template and CONTRIBUTING.md enhancements", pr_number: 22788, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 43, deletions_count: 4},
		{sha: "6a39e6a42042761586aa115975926a65538fb76f", date: "2025-04-09 22:44:14 UTC", description: "add known issue to v0.46.0", pr_number: 22842, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 7, deletions_count: 0},
		{sha: "238e8b8c26718ab19f4c5d97ef9dc45e3c6f1e06", date: "2025-04-11 12:17:53 UTC", description: "fix dead links to configuration reference", pr_number: 22846, scopes: ["external"], type: "docs", breaking_change: false, author: "Shin Seunghun", files_count: 6, insertions_count: 15, deletions_count: 15},
		{sha: "c1ad3421e1215d83a376c5541412c0359682752a", date: "2025-04-12 00:58:38 UTC", description: "properly display client count in websocket_server debug logs", pr_number: 22855, scopes: ["websocket_server sink"], type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 1, insertions_count: 12, deletions_count: 2},
		{sha: "eb60aa05d6e3d5c390de0f581e24cebaf31cca9c", date: "2025-04-12 07:42:50 UTC", description: "Bump OpenDAL to v0.53.0", pr_number: 21493, scopes: ["deps"], type: "chore", breaking_change: false, author: "Xuanwo", files_count: 5, insertions_count: 45, deletions_count: 31},
		{sha: "0a613370046c1f25ed747a829bbf3f665282a41d", date: "2025-04-12 04:13:29 UTC", description: "Bump maxminddb from 0.25.0 to 0.26.0 ", pr_number: 22804, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jakub Onderka", files_count: 4, insertions_count: 13, deletions_count: 12},
		{sha: "ee6dc973ed600e1ba4e657a0de1dd65a7f56020a", date: "2025-04-12 07:19:26 UTC", description: "add support for websocket subprotocol", pr_number: 22854, scopes: ["websocket_server sink"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 4, insertions_count: 105, deletions_count: 2},
		{sha: "9dfa36bafbeefe6841d443f0430473ee4216fb87", date: "2025-04-12 05:55:27 UTC", description: "Bump tokio from 1.44.1 to 1.44.2 in the cargo group", pr_number: 22861, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 18, deletions_count: 18},
		{sha: "b30a36cf42c283ef8128c5720b6fc1315dad6eb5", date: "2025-04-15 04:09:56 UTC", description: "add ability to generate JUnit reports", pr_number: 22857, scopes: ["unit tests"], type: "feat", breaking_change: false, author: "simplepad", files_count: 4, insertions_count: 98, deletions_count: 0},
		{sha: "07b4c71b4d35746664fcb5bf8dca7d941fef65c8", date: "2025-04-14 21:43:30 UTC", description: "enhancements to the patch release template", pr_number: 22871, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "c8b57ded8591ff70dd0bd65f0ba3d77f55675dd0", date: "2025-04-15 00:47:28 UTC", description: "update Cargo.lock", pr_number: 22873, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 34, deletions_count: 0},
		{sha: "cb62efe251e78470c7e3f7e7b718f2c87d45db27", date: "2025-04-14 23:50:12 UTC", description: "enhancements to the patch release template", pr_number: 22872, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "f35398a7c2689f5cc4f5bb6c0545f0bb2cc7940c", date: "2025-04-15 05:15:54 UTC", description: "adding metric ingestion to opentelemetry source", pr_number: 22746, scopes: ["opentelemetry source"], type: "feat", breaking_change: false, author: "Kirill Mikhailov", files_count: 16, insertions_count: 1833, deletions_count: 292},
		{sha: "7cfc9c54bffba9a603d58eb01134305b50ef5f4a", date: "2025-04-15 01:06:11 UTC", description: "add v0.46.1 patch commits", pr_number: 22874, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 27, deletions_count: 1},
		{sha: "2c038c3b2600c24520be6d8e63be0f6cb57de9c0", date: "2025-04-15 17:44:48 UTC", description: "fix rendering of 0.46.1.cue", pr_number: 22882, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "3f7026f05c7bd485297ef6469fcd6d512d6c8518", date: "2025-04-15 23:12:18 UTC", description: "bump darling version", pr_number: 22883, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 19, deletions_count: 18},
		{sha: "8029c109fbaf8af5822dce72a678c54db02810c1", date: "2025-04-16 06:17:20 UTC", description: "support all formats: YAML,TOML,JSON", pr_number: 22864, scopes: ["config provider"], type: "enhancement", breaking_change: false, author: "Kirill Nazarov", files_count: 3, insertions_count: 39, deletions_count: 8},
		{sha: "dc614442826c829acf45a554c9cace766a9f5a31", date: "2025-04-15 23:29:45 UTC", description: "build manifests - 0.46.1", pr_number: 22887, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "3e1327890e27b9f62ac7631fa72198fe11c8922a", date: "2025-04-15 23:44:39 UTC", description: "increase cloudwatch logs max event size to match new aws limit of 1MB", pr_number: 22886, scopes: ["aws_cloudwatch_logs sink"], type: "fix", breaking_change: false, author: "Casey Hartman", files_count: 2, insertions_count: 29, deletions_count: 6},
		{sha: "82ffecbf4b869922643b8ac51aecf972c05b64d1", date: "2025-04-15 20:57:59 UTC", description: "Add support for rendering to mermaid with `vector graph` command", pr_number: 22787, scopes: ["cli"], type: "feat", breaking_change: false, author: "Eric Stern", files_count: 2, insertions_count: 66, deletions_count: 0},
		{sha: "e293b574a3141deff09fb809e87d8cb66f3de608", date: "2025-04-16 22:55:25 UTC", description: "Improve performance with `length_delimited` framing and large batches", pr_number: 22877, scopes: ["codecs"], type: "enhancement", breaking_change: false, author: "Alex Savitskii", files_count: 2, insertions_count: 20, deletions_count: 5},
		{sha: "ce170d5e6062083af476277579361373e23af97a", date: "2025-04-17 00:22:11 UTC", description: "Apply agent-json header on events from agent", pr_number: 22701, scopes: ["datadog_logs sink"], type: "fix", breaking_change: false, author: "Rob Blafford", files_count: 6, insertions_count: 454, deletions_count: 107},
		{sha: "3ea8c86f9461f1e3d403c3c6820fdf19b280fe75", date: "2025-04-17 06:40:08 UTC", description: "Add new `window` transform", pr_number: 22609, scopes: ["new transform"], type: "feat", breaking_change: false, author: "Linas Zvirblis", files_count: 13, insertions_count: 1142, deletions_count: 0},
		{sha: "42caf99f7bb027a02ed9fef622f2d5a7a1767816", date: "2025-04-19 00:13:45 UTC", description: "add access to client address in custom VRL auth", pr_number: 22850, scopes: ["sources"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 5, insertions_count: 82, deletions_count: 32},
		{sha: "8057e633eb0b97f1286848d6f3f393524ae4c643", date: "2025-04-19 00:57:52 UTC", description: "Add support for forwarding over unix socket", pr_number: 22212, scopes: ["fluent source"], type: "enhancement", breaking_change: false, author: "Raphael Taylor-Davies", files_count: 5, insertions_count: 339, deletions_count: 93},
		{sha: "3c10200690febf375cc80c57cecbcac77a19186d", date: "2025-04-22 08:39:59 UTC", description: "Remove known small files based on remove_after_secs", pr_number: 22786, scopes: ["file source"], type: "fix", breaking_change: false, author: "林玮 (Jade Lin)", files_count: 3, insertions_count: 54, deletions_count: 18},
		{sha: "8861538469ce8f002493ff4699dae64376d62ded", date: "2025-04-22 11:50:43 UTC", description: "add `convert_to` option for controlling timing unit conversion", pr_number: 22716, scopes: ["statsd source"], type: "feat", breaking_change: false, author: "Jinsoo Heo", files_count: 7, insertions_count: 165, deletions_count: 29},
		{sha: "d7c27eb3ac131b7631391330a402bb7797a2ebba", date: "2025-04-21 23:38:52 UTC", description: "optionally configure source to defer or delete older notifications", pr_number: 22691, scopes: ["aws_s3 source"], type: "enhancement", breaking_change: false, author: "Andrew Kutta", files_count: 4, insertions_count: 317, deletions_count: 7},
		{sha: "ca9fa2e61e26243b12c54ea0a9aaf6997acadd31", date: "2025-04-23 04:43:17 UTC", description: "`AllocationGroupId::register` groups max limit increased to 256", pr_number: 22897, scopes: ["internal"], type: "fix", breaking_change: false, author: "triggerh@ppy", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "18d6867a64ec0884f9d3a5f9af6ae25a06041fb2", date: "2025-04-23 03:47:28 UTC", description: "add a config option for setting the healthcheck timeout", pr_number: 22922, scopes: ["config"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 7, insertions_count: 58, deletions_count: 15},
		{sha: "e3d7ad090af6b34f46a88f2f774ab715813253e5", date: "2025-04-23 01:48:37 UTC", description: "Remove datadog specific telemetry", pr_number: 22927, scopes: ["datadog service"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 12},
		{sha: "997beb8919284bbbfa630ec51ac28788ab20a1cc", date: "2025-04-23 20:44:13 UTC", description: "guide page improvements", pr_number: 22936, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 9, insertions_count: 45, deletions_count: 39},
		{sha: "b0bac1e1ac2ef5dfae7a2a39d6d9b542ba3556e3", date: "2025-04-23 19:25:01 UTC", description: "fix typo in CONTRIBUTING.md", pr_number: 22930, scopes: ["external"], type: "docs", breaking_change: false, author: "Nick Wang", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "861075f2a69d8c6213e358527e9ab90b84763341", date: "2025-04-25 03:23:10 UTC", description: "Implement components selection in diff.rs and tests", pr_number: 22678, scopes: ["config"], type: "enhancement", breaking_change: false, author: "Guillaume Le Blanc", files_count: 10, insertions_count: 172, deletions_count: 98},
		{sha: "1e7da76fc0f178284d2a4e0291128fc8ddce67c3", date: "2025-04-25 10:03:29 UTC", description: "add headers field to greptimedb_logs config", pr_number: 22651, scopes: ["greptimedb_logs sink"], type: "enhancement", breaking_change: false, author: "localhost", files_count: 5, insertions_count: 71, deletions_count: 2},
		{sha: "e2f84ea86ffe29a01f919dcab4bbfa6df7d4744f", date: "2025-04-29 02:37:27 UTC", description: "Allows users to specify AWS authentication and the AWS service name for HTTP sinks to support AWS API endpoints that require SigV4.", pr_number: 22744, scopes: ["http sink"], type: "feat", breaking_change: false, author: "Johannes Geiger", files_count: 19, insertions_count: 1390, deletions_count: 27},
		{sha: "d771ab1b753544a2b4e382c09bb13b7e58ce29be", date: "2025-04-28 23:01:36 UTC", description: "add priority property", pr_number: 22243, scopes: ["amqp sink"], type: "feat", breaking_change: false, author: "Aram Peres", files_count: 8, insertions_count: 529, deletions_count: 16},
		{sha: "96bc5942ac5a0d082e14ba706133631d094f3061", date: "2025-04-29 01:08:52 UTC", description: "updates concepts/traces page", pr_number: 22955, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 19, deletions_count: 5},
		{sha: "4b4b16e31cfdf9fc7dac67ecf48192939067742a", date: "2025-04-30 01:01:56 UTC", description: "fix reduce transfrom examples", pr_number: 22962, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 15, deletions_count: 4},
		{sha: "4ad3fdc54c2c3b56db94a8613417783458a96111", date: "2025-04-30 18:53:50 UTC", description: "Remove `_collisions` field in agent normalization routine", pr_number: 22956, scopes: ["datadog_logs sink"], type: "fix", breaking_change: false, author: "Rob Blafford", files_count: 1, insertions_count: 16, deletions_count: 136},
		{sha: "f4c15e33b6fccdf5c7ce44f05dc2782790c9a9d2", date: "2025-05-01 17:37:03 UTC", description: "Bump vrl from `d802b30` to `7f8ed50`", pr_number: 22977, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "b6b9c677ac8e7ee158ba5f210750fd3ce97ff88b", date: "2025-05-01 17:37:32 UTC", description: "Bump smallvec from 1.14.0 to 1.15.0", pr_number: 22982, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "b70e555a1e2c563de826683eb1942ca8d88cb1bb", date: "2025-05-01 18:26:48 UTC", description: "Bump lru from 0.13.0 to 0.14.0", pr_number: 22981, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "4f3aad5b6ad68fc4a7514d5959575d96da167552", date: "2025-05-01 22:27:20 UTC", description: "Bump bstr from 1.11.3 to 1.12.0", pr_number: 22980, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "db929085e7b7ffb6adf4b16b6fd0009c4afe1b25", date: "2025-05-01 22:27:40 UTC", description: "Bump data-encoding from 2.8.0 to 2.9.0", pr_number: 22979, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0cf34ee74b9868a8362e90f8b0d48fb8d7846305", date: "2025-05-01 22:29:22 UTC", description: "Bump openssl-src from 300.4.2+3.4.1 to 300.5.0+3.5.0", pr_number: 22978, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "f84c36c8be42352f74221aa8c9d66309a3047c9b", date: "2025-05-01 23:32:50 UTC", description: "Bump the patches group with 26 updates", pr_number: 22973, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 433, deletions_count: 311},
		{sha: "e5726e1250d2fbbc524a76110999d49517317507", date: "2025-05-02 10:12:44 UTC", description: "remove limit if use apiserver cache", pr_number: 22921, scopes: ["kubernetes_logs source"], type: "fix", breaking_change: false, author: "Xiao", files_count: 3, insertions_count: 18, deletions_count: 2},
		{sha: "b8c24c9bebcf898cfeed257db6ca95e4e0664ba6", date: "2025-05-02 02:58:36 UTC", description: "Bump docker/build-push-action from 6.15.0 to 6.16.0", pr_number: 22984, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "61c418f88a83bad665529cab82462e15034df730", date: "2025-05-05 18:46:17 UTC", description: "introduce log namespace guide", pr_number: 22985, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 196, deletions_count: 3},
		{sha: "9b824f97516ea92c3a7a894e4bedddb9ad9b59f1", date: "2025-05-06 01:44:29 UTC", description: "support input based on encoding type", pr_number: 22989, scopes: ["redis sink"], type: "feat", breaking_change: false, author: "Yao Noel Achi", files_count: 4, insertions_count: 201, deletions_count: 6},
		{sha: "9a3d2e83a904c36972e2aa7e5457a65454b7bc36", date: "2025-05-07 08:21:27 UTC", description: "fix reduce transfrom examples", pr_number: 22992, scopes: ["external"], type: "docs", breaking_change: false, author: "Pomin Wu", files_count: 1, insertions_count: 6, deletions_count: 7},
		{sha: "9daa00cbec230ba0d89ae85ab3b001b15bc9db97", date: "2025-05-07 20:07:34 UTC", description: "run markdownlint on git files only", pr_number: 23004, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 29, deletions_count: 15},
		{sha: "91245a4dd9ac2be20acb9ce53d2d59905e3513bb", date: "2025-05-10 01:05:49 UTC", description: "add `wildcard_matching` config option", pr_number: 23011, scopes: ["config"], type: "feat", breaking_change: false, author: "simplepad", files_count: 9, insertions_count: 156, deletions_count: 13},
		{sha: "9129d95c56ad530989c238dd0d380de6a0c95ca8", date: "2025-05-13 15:31:55 UTC", description: "Remove slash from max_bytes text", pr_number: 23035, scopes: ["sinks"], type: "docs", breaking_change: false, author: "Jed Laundry", files_count: 47, insertions_count: 47, deletions_count: 47},
		{sha: "a39d60acbfb60a3b740c3da45a3fadfc3e1052a4", date: "2025-05-14 20:42:31 UTC", description: "Initial MQTT Source, #19931", pr_number: 22752, scopes: ["new source"], type: "feat", breaking_change: false, author: "StormStake", files_count: 18, insertions_count: 1296, deletions_count: 103},
		{sha: "689a65d51a2f8034f220afcf5e65e1b924019687", date: "2025-05-14 22:45:15 UTC", description: "e2e dd logs failure", pr_number: 23038, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 8, insertions_count: 50, deletions_count: 31},
		{sha: "822ed0d3f6a752637a33c60b983bd158ad27cce6", date: "2025-05-15 00:48:00 UTC", description: "revert deps bump", pr_number: 23039, scopes: ["azure service"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 60, deletions_count: 114},
		{sha: "346b1ba993f7cdb4e1608fcfb730267c90a85496", date: "2025-05-15 03:46:54 UTC", description: "gelf encoder error message", pr_number: 23021, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Miro Prasil", files_count: 2, insertions_count: 11, deletions_count: 1},
		{sha: "1a1050dbe585bf4a79c37b2183d075ac49bbf254", date: "2025-05-15 00:10:33 UTC", description: "request ARG in every build stage", pr_number: 23049, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8f10b37a38daaec7bf48f91f00ebea6afa4eb183", date: "2025-05-15 16:37:20 UTC", description: "disable E2E datadog-logs test", pr_number: 23055, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 9, deletions_count: 8},
		{sha: "c1cf664ecbbdf5abe945a0b0c74001829a25fea5", date: "2025-05-15 16:54:10 UTC", description: "temporarily disable failing AMQP tests", pr_number: 23057, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "f2b4e29191e5cbdd6d94111dec1122dc3f9da5d7", date: "2025-05-15 17:13:27 UTC", description: "remove redundant rustup prefix", pr_number: 23059, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4643acd78cdaf051c3088f7fc46be1488c109563", date: "2025-05-15 17:22:23 UTC", description: "provide default RUST_VERSION if one is not specified", pr_number: 23060, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 3, deletions_count: 3},
		{sha: "84e5fe9bec1221c58e663602a2aef10e90c146d5", date: "2025-05-15 17:57:56 UTC", description: "bump nextest to latest version", pr_number: 23058, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "2c63454e394cdd18451bd3432b87f1b65d516ede", date: "2025-05-16 00:07:01 UTC", description: "Handling wrong timestamps in dnstap-parser", pr_number: 23048, scopes: ["parsing"], type: "fix", breaking_change: false, author: "Burkov Egor", files_count: 2, insertions_count: 52, deletions_count: 5},
		{sha: "ace0dd21b542cfd522a18f3f3943a643a91e0e79", date: "2025-05-16 00:42:26 UTC", description: "Handle underflow in A6 parsing", pr_number: 23047, scopes: ["parsing"], type: "fix", breaking_change: false, author: "Burkov Egor", files_count: 2, insertions_count: 20, deletions_count: 0},
		{sha: "9cdab60b320ad14c06214b4cf7b7e0f8a441f373", date: "2025-05-16 01:26:16 UTC", description: "make `--watch-config` watch external VRL files in `remap` transforms", pr_number: 23010, scopes: ["remap transform"], type: "feat", breaking_change: false, author: "nekorro", files_count: 5, insertions_count: 35, deletions_count: 2},
		{sha: "914d1568dd1363d2d95b9e8dc4b2b1a808885d12", date: "2025-05-16 20:59:37 UTC", description: "install mold (no need to build it)", pr_number: 23064, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 11},
		{sha: "1d232fb5057786b704b5ac953aa0fd7a74493c26", date: "2025-05-16 18:53:31 UTC", description: "add from and to date search to enrichment tables", pr_number: 22926, scopes: ["enriching"], type: "enhancement", breaking_change: false, author: "Nick Wang", files_count: 5, insertions_count: 177, deletions_count: 3},
		{sha: "c9791e3db80659a5f763e51b4ad0824e366cbf1f", date: "2025-05-17 01:15:42 UTC", description: "path resolution for docker compose files", pr_number: 23066, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 30, deletions_count: 33},
	]
}
