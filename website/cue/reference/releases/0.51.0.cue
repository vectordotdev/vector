package metadata

releases: "0.51.0": {
	date:     "2025-11-04"
	codename: ""

	whats_next: []

	known_issues: [
		"""
			The newly added `basename`, `dirname` and `split_path` VRL functions are not accessible
			because they weren't properly exposed in the latest VRL release (`0.28.0`).
			""",
		"""
			The newly added `config_reload_rejected` and `config_reloaded` counters are not
			emitted. These counters will be replaced in the next patch release (`0.51.1`) in favor of
			`component_errors_total` with `error_code="reload"` and `reloaded_total` metrics,
			respectively.
			""",
		"""
			Blackhole sink periodic statistics messages are incorrectly rate limited.
			""",
		"""
			When running Vector with debug logs enabled (`VECTOR_LOG=debug`), threads panic when log
			messages are missing both a message and a rate limit tag. This is known to happen when
			the utilization debug log is emitted and in the file server (affecting the `file` and
			`kubernetes_logs` sources).
			""",
	]

	description: """
		The Vector team is excited to announce version `0.51.0`!

		Please refer to the [upgrade guide](/highlights/2025-11-04-0-51-0-upgrade-guide) for breaking changes in this release.

		## Release highlights

		- Enhanced OpenTelemetry Protocol (OTLP) support with the introduction of the `otlp` codec, enabling
		  bidirectional conversion between Vector events and OTLP format for seamless integration with
		  OpenTelemetry collectors and instrumentation.
		- Improved Vector's internal telemetry with new `config_reload_rejected` and `config_reloaded` counters,
		  and fixed issues where utilization metrics reported negative values and buffer counters underflowed.
		- Enhanced memory enrichment tables with an `expired` output for exporting expired cache items,
		  and made enrichment table outputs accessible via `vector tap`.

		## Breaking Changes

		- Environment variable interpolation in configuration files now rejects values containing newline characters. This prevents configuration
		  injection attacks where environment variables could inject malicious multi-line configurations. If you need to inject multi-line
		  configuration blocks, use a config pre-processing tool like `envsubst` instead
		  or update your configuration files so that they don't rely on block injections.

		- Vector's internal topology `debug!` and `trace!` logs now use the `component_id` field name instead of `component` or `key`.
		  If you are monitoring or filtering Vector's internal logs based on these field names, update your queries to use `component_id`.

		- The `utilization` metric is now capped at 4 decimal digit precision.

		- Support for legacy fingerprints in the `file` source was dropped. Affected users may be
		  ones that have been running Vector since version 0.14 or earlier. Consult the upgrade guide for more details.

		- Following [this announcement](https://blog.rust-lang.org/2025/09/18/Rust-1.90.0/#demoting-x86-64-apple-darwin-to-tier-2-with-host-tools), we will no longer publish `x86_64-apple-darwin` builds.
		  this means we will not be validating if Vector builds and works correctly on that platform.
		"""

	changelog: [
		{
			type: "feat"
			description: """
				Added `truncate` options to `file` sink to truncate output files after some time.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: """
				Disabled ANSI color for `vector test` when running non-interactively. Honor `--color {auto|always|never}` and `VECTOR_COLOR`; VRL diagnostics no longer include ANSI sequences when color is disabled.
				"""
			contributors: ["VanjaRo"]
		},
		{
			type: "feat"
			description: """
				Added proper support for compression of HEC indexer ack queries, using the sink's configured `compression` setting.
				"""
			contributors: ["sbalmos"]
		},
		{
			type: "feat"
			description: """
				Added `expired` output to the memory enrichment table source, to export items as they expire in the cache.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: """
				On receiving SIGHUP Vector now also reloads transform components with external VRL files.
				"""
			contributors: ["nekorro"]
		},
		{
			type: "fix"
			description: """
				The configuration watcher now collects event paths even during the delay period. These were previously ignored and prevented components from reloading.
				"""
			contributors: ["nekorro"]
		},
		{
			type: "fix"
			description: """
				Enabled unused TLS settings to perform client authentication by SSL certificate in `mqtt` sink.
				"""
			contributors: ["ValentinChernovNTQ"]
		},
		{
			type: "fix"
			description: """
				Memory enrichment tables' outputs are now visible to the `vector tap` command.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: """
				Fixed a panic in the `azure_blob` sink by enabling a missing required crate feature.
				"""
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: """
				Fixed an issue where the buffer counter underflowed. This was caused by the counter not being increased before a new event was observed.
				"""
			contributors: ["sialais"]
		},
		{
			type: "chore"
			description: """
				* Dropped support for `file` source legacy checkpoints stored in the `checkpoints` folder (Vector `< 0.11`) which is located inside the `data_dir`.
				* Removed the legacy checkpoint checksum format (Vector `< 0.15`).
				* The intentionally hidden `fingerprint.bytes` option was also removed.

				### How to upgrade

				You can stop reading if you

				* have started using the `file` source on or after version `0.15`, or
				* have cleared your `data_dir` on or after version `0.15`, or
				* don't care about the file positions and don't care about current state of your checkpoints, meaning you accept that files could be read from the beginning again after the upgrade.
				  * Vector will re-read all files from the beginning if/when any `checkpoints.json` files nested inside `data_dir` fail to load due to legacy/corrupted data.

				You are only affected if your Vector version is:

				1. `>= 0.11` and `< 0.15`, then your checkpoints are using the legacy checkpoint checksum CRC format.
				2. `>= 0.11` and `< 0.15`, then the `checksum` key is present under `checkpoints.fingerprint` in your `checkpoints.json` (instead of `first_lines_checksum`).
				3. **or ever was** `< 0.11` and you are using the legacy `checkpoints` folder and/or the `unknown` key is present under `checkpoints.fingerprint` in any `checkpoints.json` files nested inside `data_dir`.

				#### If you are affected by `#1` or `#2`

				Run the `file` source with any version of Vector `>= 0.15`, but strictly before `0.51` and the checkpoints should be automatically updated.
				For example, if you’re on Vector `0.10` and want to upgrade, keep upgrading Vector until `0.14` and Vector will automatically convert your checkpoints.
				When upgrading, we recommend stepping through minor versions as these can each contain breaking changes while Vector is pre-1.0. These breaking changes are noted in their respective upgrade guides.

				Odds are the `file` source automatically converted checkpoints to the new format if you are using a recent version and you are not affected by this at all.

				#### If you are affected by `#3`

				You should manually delete the `unknown` checkpoint records from all `checkpoints.json` files nested inside `data_dir`
				and then follow the upgrade guide for `#1` and `#2`. If you were using a recent version of Vector and `unknown`
				was present it wasn't being used anyways.
				"""
			contributors: ["thomasqueirozb"]
		},
		{
			type: "feat"
			description: """
				The `journald` source now provides better error visibility by capturing and displaying stderr output from the underlying `journalctl` process as warning messages.
				"""
			contributors: ["titaneric"]
		},
		{
			type: "enhancement"
			description: """
				Added a new `split_metric_namespace` option to the `datadog_agent` source to
				optionally disable the existing default metric name split behavior.
				"""
			contributors: ["bruceg"]
		},
		{
			type: "fix"
			description: """
				Fixed a crash on configuration reload when memory enrichment tables are configured to be used as a source.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: """
				Fixed an issue in the `docker_logs` source where the `docker_host` option and `DOCKER_HOST` environment variable were ignored if they started with `unix://` or `npipe://`. In those cases the default location for the Docker socket was used
				"""
			contributors: ["titaneric"]
		},
		{
			type: "fix"
			description: """
				Fixed an issue where utilization could report negative values. This could happen if messages from components were processed too late and were accounted for wrong utilization measurement period. These messages are now moved to the current utilization period, meaning there might be some inaccuracy in the resulting utilization metric, but it was never meant to be precise.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: """
				Fixed a bug where utilization metric could be lost for changed components on configuration reload.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: """
				Improved Avro encoding error. Schema and value are now included in the message
				"""
			contributors: ["titaneric"]
		},
		{
			type: "feat"
			description: """
				Vector now emits `config_reload_rejected` and `config_reloaded` counters.
				"""
			contributors: ["suikammd"]
		},
		{
			type: "enhancement"
			description: """
				The `aws_s3` source now uses exponential backoff when retrying failed SQS `receive_message` operations. Previously, the source used a fixed 500ms delay between retries.

				The new behavior starts at 500ms and doubles with each consecutive failure, capping at 30 seconds. This prevents excessive API calls during prolonged AWS SQS outages, invalid IAM permissions, or throttling scenarios, while still being responsive when the service recovers.
				"""
			contributors: ["medzin", "pront"]
		},
		{
			type: "chore"
			description: """
				Environment variable interpolation in configuration files now rejects values containing newline characters. This prevents configuration
				injection attacks where environment variables could inject malicious multi-line configurations. If you need to inject multi-line
				configuration blocks, use a config pre-processing tool like `envsubst` instead.
				"""
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: """
				Fixed duplicate reporting of received event count in the `fluent` source.
				"""
			contributors: ["gwenaskell"]
		},
		{
			type: "chore"
			description: """
				Vector's internal topology `debug!` and `trace!` logs now use the `component_id` field name instead of `component` or `key`.
				If you are monitoring or filtering Vector's internal logs based on these field names, update your queries to use `component_id`.
				"""
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: """
				Fixed a `opentelemetry` source bug where HTTP payloads were not decompressed according to the request headers.
				This only applied when `use_otlp_decoding` (recently added) was set to `true`.
				"""
			contributors: ["pront"]
		},
		{
			type: "feat"
			description: """
				Added `otlp` codec for decoding OTLP format to Vector events, complementing the existing OTLP encoder.
				"""
			contributors: ["pront"]
		},
		{
			type: "feat"
			description: """
				Added `otlp` codec for encoding Vector events to OTLP format.
				The codec can be used with sinks that support encoding configuration.
				"""
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: """
				The `prometheus_remote_write` source now has a `metadata_conflict_strategy` option so you can determine how to handle conflicting metric metadata. By default, the source continues to reject requests with conflicting metadata (HTTP 400 error) to maintain backwards compatibility. Set `metadata_conflict_strategy` to `ignore` to align with Prometheus/Thanos behavior, which silently ignores metadata conflicts.
				"""
			contributors: ["elohmeier"]
		},
		{
			type: "feat"
			description: """
				Added `path` configuration option to `prometheus_remote_write` source to allow accepting metrics on custom URL paths instead of only the root path. This enables configuration of endpoints like `/api/v1/write` to match standard Prometheus remote write conventions.
				"""
			contributors: ["elohmeier"]
		},
		{
			type: "enhancement"
			description: """
				Added `use_json_names` option to protobuf encoding and decoding.
				When enabled, the codec uses JSON field names (camelCase) instead of protobuf field names (snake_case).
				This is useful when working with data that uses JSON naming conventions.
				"""
			contributors: ["pront"]
		},
		{
			type: "chore"
			description: """
				The `utilization` metric is now capped at 4 decimal digit precision.
				"""
			contributors: ["pront"]
		},
		{
			type: "chore"
			description: """
				Following [this announcement](https://blog.rust-lang.org/2025/09/18/Rust-1.90.0/#demoting-x86-64-apple-darwin-to-tier-2-with-host-tools), we will no longer publish `x86_64-apple-darwin` builds.
				This means we will not be validating if Vector builds and works correctly on that platform.
				"""
			contributors: ["pront"]
		},
	]

	vrl_changelog: """
		### [0.28.0 (2025-11-03)]

		#### Breaking Changes & Upgrade Guide

		- The return value of the `find` function has been changed to `null` instead of `-1` if there is no match.

		authors: titaneric (https://github.com/vectordotdev/vrl/pull/1514)

		#### New Features

		- Introduced the `basename` function to get the last component of a path.

		authors: titaneric (https://github.com/vectordotdev/vrl/pull/1531)
		- Introduced the `dirname` function to get the directory component of a path.

		authors: titaneric (https://github.com/vectordotdev/vrl/pull/1532)
		- Introduced the `split_path` function to split a path into its components.

		authors: titaneric (https://github.com/vectordotdev/vrl/pull/1533)

		#### Enhancements

		- Added optional `http_proxy` and `https_proxy` parameters to `http_request` for setting the proxies used for a request. (https://github.com/vectordotdev/vrl/pull/1534)
		- Added support for encoding a VRL `Integer` into a protobuf `double` when using `encode_proto`

		authors: thomasqueirozb (https://github.com/vectordotdev/vrl/pull/1545)

		#### Fixes

		- Fixed `parse_glog` to accept space-padded thread-id. (https://github.com/vectordotdev/vrl/pull/1515)


		### [0.27.0 (2025-09-18)]
		"""

	commits: [
		{sha: "8b25a7e918bfbd2732de8e5f7ab8de5c6becd563", date: "2025-09-19 18:09:13 UTC", description: "add timeout to component features job", pr_number: 23814, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "25f0353fa129a9318fb9f3c97cfe2b2facf89940", date: "2025-09-20 00:49:21 UTC", description: "add example for AWS Secrets Manager backend", pr_number: 23548, scopes: ["external"], type: "docs", breaking_change: false, author: "Gary Sassano", files_count: 3, insertions_count: 501, deletions_count: 0},
		{sha: "203b2bcbb0f453939fdcea7175b489c37df54400", date: "2025-09-19 21:11:02 UTC", description: "increase timeouts in file_start_position_server_restart_unfinalized", pr_number: 23812, scopes: ["tests"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "4eadd70b7444a9fa6a2dee22bd3f3c7a803dc188", date: "2025-09-19 22:03:27 UTC", description: "run IT suite once", pr_number: 23818, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 22},
		{sha: "a70eca4ea9900075ac8954c74238b2742fb244cd", date: "2025-09-19 23:09:01 UTC", description: "consolidate usage of VECTOR_LOG in tests and remove TEST_LOG", pr_number: 23804, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 12, insertions_count: 12, deletions_count: 12},
		{sha: "e1cd39c78e439a8cb054aef69782cc00524ddb11", date: "2025-09-20 00:21:33 UTC", description: "enable colors when running in nextest", pr_number: 23819, scopes: ["dev"], type: "feat", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 5, deletions_count: 0},
		{sha: "52049ad615a6c31eda3ca7c45150e2c201c309d0", date: "2025-09-22 18:32:47 UTC", description: "improve indexing for memory table docs", pr_number: 23827, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "be2dde4a0b4bcc40c5e20aa69b385bf083c1b414", date: "2025-09-22 23:01:23 UTC", description: "fix vector diagram", pr_number: 23830, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 9667, deletions_count: 1574},
		{sha: "b86a6aa199d0d38cbe86b8dd68a52bb3211c698c", date: "2025-09-23 17:50:06 UTC", description: "minor release template fixes", pr_number: 23831, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "a52a7494adc133c765b9cdcf70ce1cf8fbc504a8", date: "2025-09-24 00:28:50 UTC", description: "add options to truncate files in some conditions", pr_number: 23671, scopes: ["file sink"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 5, insertions_count: 162, deletions_count: 36},
		{sha: "8387b5e4be4abb70d90bb419646b4e512ffacabb", date: "2025-09-23 20:10:36 UTC", description: "extract homebrew publishing into a new workflow", pr_number: 23833, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 38, deletions_count: 21},
		{sha: "80fee2733787d7468bba971f5766373f2c27cf0d", date: "2025-09-23 22:32:28 UTC", description: "allow manual homebrew runs", pr_number: 23835, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 11, deletions_count: 0},
		{sha: "74380c218d9626f61686a60c10b3b7e7ef907953", date: "2025-09-23 22:44:17 UTC", description: "post release steps", pr_number: 23834, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 51, insertions_count: 554, deletions_count: 110},
		{sha: "56a7af50c8a36bb09843bc8c5b524a2c9ecf46c1", date: "2025-09-24 17:28:23 UTC", description: "fix typo", pr_number: 23841, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "592dab79583c9f7e221bb00ec38e94b41473081d", date: "2025-09-24 18:03:35 UTC", description: "spellchecker fix", pr_number: 23842, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 19, deletions_count: 10},
		{sha: "e6da13867c68dff362263006a8e6350e0bbce1f8", date: "2025-09-24 17:36:32 UTC", description: "homebrew workflow fixes", pr_number: 23836, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 10, deletions_count: 9},
		{sha: "c52d405e5a54287c668f20ac95a8a81c3c142236", date: "2025-09-25 06:37:15 UTC", description: "emit config_reload_rejected and config_reloaded counters", pr_number: 23500, scopes: ["config"], type: "feat", breaking_change: false, author: "Suika", files_count: 5, insertions_count: 117, deletions_count: 12},
		{sha: "76dc8b7291e2b3015c5d49f1a3ea6a3247bad97e", date: "2025-09-24 21:35:31 UTC", description: "add .md authors spelling pattern", pr_number: 23843, scopes: ["dev"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 3, deletions_count: 14},
		{sha: "d12c8f14ce1af42cfb1e1b38b11115a5ea884b66", date: "2025-09-24 23:40:53 UTC", description: "Expose a public way to load a config from str", pr_number: 23825, scopes: ["config"], type: "chore", breaking_change: false, author: "Rob Blafford", files_count: 5, insertions_count: 86, deletions_count: 13},
		{sha: "a7d91b343abeb321ba53919924e633b62671e2ba", date: "2025-09-25 17:36:15 UTC", description: "spread out schedules", pr_number: 23852, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 4},
		{sha: "a4ded4a8dfeb8e4ea19bae53c285ffb378f3cc75", date: "2025-09-26 05:14:49 UTC", description: "add expired items output to memory enrichment table", pr_number: 23815, scopes: ["enrichment tables"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 7, insertions_count: 287, deletions_count: 107},
		{sha: "fad8439c7051a8a3968b9184f257239e0bc173b7", date: "2025-09-26 01:00:29 UTC", description: "use shared volume in `opentelemetry-logs` E2E test", pr_number: 23854, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 10, insertions_count: 90, deletions_count: 108},
		{sha: "bd6a8f51d6c16106de76de3330d27415f53940ad", date: "2025-09-26 17:15:53 UTC", description: "increase e2e timeout", pr_number: 23857, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "78935388ac68c7e110fe148b56d564a887f02a11", date: "2025-09-30 09:09:30 UTC", description: "vrl pop() function", pr_number: 23727, scopes: ["external"], type: "docs", breaking_change: false, author: "jlambatl", files_count: 1, insertions_count: 34, deletions_count: 0},
		{sha: "5ce51c046d3e686ed248d9f82800581e5acdc231", date: "2025-09-29 18:28:31 UTC", description: "Document best-practice of not ending with _config in config spec", pr_number: 23866, scopes: ["config"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "ae9010cb89ce19e9254731214861085b1fd8f82f", date: "2025-09-30 00:37:20 UTC", description: "windows rustup stable not installed by default", pr_number: 23868, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 21, deletions_count: 8},
		{sha: "a17a1844efffe0ce4b3cf4851991ccc7c2f838fd", date: "2025-09-29 21:40:52 UTC", description: "Remove `_config` suffix from `truncate_config`", pr_number: 23864, scopes: ["file sink"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 14, deletions_count: 14},
		{sha: "a3ee7ab9854b9ffb4f411733f45811ddbcb9d3e9", date: "2025-09-30 00:48:25 UTC", description: "improvements", pr_number: 23869, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 91, insertions_count: 459, deletions_count: 488},
		{sha: "4985e40303afa0bd6aabfc4d9aea3a99b855e973", date: "2025-09-30 01:03:24 UTC", description: "use correct reqwest feature", pr_number: 23865, scopes: ["azure_blob sink"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 4, deletions_count: 1},
		{sha: "1d86067671a36ea9c35687a11616d18ca9e262f0", date: "2025-09-30 01:14:54 UTC", description: "use rust 1.90", pr_number: 23870, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 17, insertions_count: 19, deletions_count: 39},
		{sha: "ba1131964447904a26a871f1b97f8bd4cca8a796", date: "2025-10-01 03:53:31 UTC", description: "tls auth by client cert", pr_number: 23839, scopes: ["mqtt sink"], type: "fix", breaking_change: false, author: "ValentinChernovNTQ", files_count: 3, insertions_count: 5, deletions_count: 1},
		{sha: "e4e01fa3fc2eb799fe66df6163891f6c67a9fa75", date: "2025-09-30 17:05:32 UTC", description: "remove support for x86_64-apple-darwin", pr_number: 23867, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 26, insertions_count: 37, deletions_count: 73},
		{sha: "1a2dccb548e0e2d51b3a96aebe91b10218548b2c", date: "2025-09-30 19:54:28 UTC", description: "make fingerprinter buffer internal", pr_number: 23859, scopes: ["file source"], type: "feat", breaking_change: false, author: "Thomas", files_count: 5, insertions_count: 155, deletions_count: 198},
		{sha: "2e605f52128deff9ecd7fada87b102357dca8dd9", date: "2025-09-30 23:44:07 UTC", description: "move e2e.yml logic to integration.yml", pr_number: 23873, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 37, deletions_count: 129},
		{sha: "36459cc67e8f41140fdf6e990efddace6334670c", date: "2025-10-01 17:20:30 UTC", description: "Bump tempfile from 3.21.0 to 3.23.0", pr_number: 23889, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "398b81b50b9feb44a5e1fc81e08061edc053ebcc", date: "2025-10-01 17:21:44 UTC", description: "Bump the clap group with 2 updates", pr_number: 23881, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 8},
		{sha: "2855aef0fd5f5693544f3cfc049982c41b1238b9", date: "2025-10-01 21:35:26 UTC", description: "Bump humantime from 2.2.0 to 2.3.0", pr_number: 23895, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "64463fb0b4383c3c7f3cbcb5bc742c19e368e07b", date: "2025-10-01 22:33:22 UTC", description: "update VRL", pr_number: 23903, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 12, deletions_count: 7},
		{sha: "28de351c85cfb06e3825551c467e2d7b1b237b8c", date: "2025-10-02 04:34:09 UTC", description: "stop counting received events twice", pr_number: 23900, scopes: ["fluent source"], type: "fix", breaking_change: false, author: "Yoenn Burban", files_count: 2, insertions_count: 3, deletions_count: 1},
		{sha: "5156c8b5a13dc1cf8f1c8907106a21f793ebce14", date: "2025-10-02 18:43:29 UTC", description: "Ignore E2E datadog-metrics", pr_number: 23917, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "2845c58505b2e6fd78391fa6c6da5deeffb3e31f", date: "2025-10-02 19:09:55 UTC", description: "binstall cargo nextest in int/e2e tests", pr_number: 23913, scopes: ["tests"], type: "feat", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 12, deletions_count: 4},
		{sha: "1e4920444f357f6f75c98e555c073e00fd670b07", date: "2025-10-02 19:36:54 UTC", description: "0.50.0 release typos", pr_number: 23918, scopes: ["website"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "2dfb9fcb96f5198eaf68ba19c873e3b97cdd197e", date: "2025-10-02 20:16:41 UTC", description: "use 8core runners for int tests", pr_number: 23909, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c66f4a34bb2cb72202c7d108e984b5c4baeb9b33", date: "2025-10-02 20:18:40 UTC", description: "Bump aws-smithy-runtime from 1.9.1 to 1.9.2 in the aws group", pr_number: 23879, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "ee390b82fd199b397cae09f3a8303800c501a0d1", date: "2025-10-02 20:19:00 UTC", description: "Bump sysinfo from 0.36.1 to 0.37.1", pr_number: 23892, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "8ff0e90ee298a9937654f601578ea32efe9f58d8", date: "2025-10-02 21:28:45 UTC", description: "Bump amannn/action-semantic-pull-request from 5.5.3 to 6.1.1", pr_number: 23907, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "2e2e5bb1e0421a99c1eecf8847d13425c62dd447", date: "2025-10-02 21:28:55 UTC", description: "Bump github/codeql-action from 3.30.0 to 3.30.5", pr_number: 23908, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "0346d5420980e19d447479561af61e7c218e8a07", date: "2025-10-03 02:20:11 UTC", description: "Bump actions/labeler from 5.0.0 to 6.0.1", pr_number: 23905, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "2332719568154106fc5bdc115823e568b5b206ea", date: "2025-10-03 02:37:31 UTC", description: "Bump security-framework from 3.3.0 to 3.5.1", pr_number: 23887, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 10, deletions_count: 10},
		{sha: "9ead2129b552ecfc561f44bcd0aab614cd8984b7", date: "2025-10-02 22:37:36 UTC", description: "datadog-metrics e2e test fixes", pr_number: 23919, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 94, deletions_count: 16},
		{sha: "759c6b137a9c740a98bccd9b17371f94e7e84b6c", date: "2025-10-03 02:37:43 UTC", description: "Bump proptest from 1.7.0 to 1.8.0", pr_number: 23890, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 7, deletions_count: 7},
		{sha: "03e8021571f30069b5ee47f1e2340ea5ca2e98bc", date: "2025-10-03 02:39:48 UTC", description: "Bump bytesize from 2.0.1 to 2.1.0", pr_number: 23885, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "4d20c763bc21159008f150db71178a06d09461a4", date: "2025-10-03 02:42:00 UTC", description: "Bump async-nats from 0.42.0 to 0.43.1", pr_number: 23886, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 3},
		{sha: "f09401d3bf0b84ef0e4a5264aff5c484b03358ec", date: "2025-10-02 23:24:01 UTC", description: "Bump docker/login-action from 3.5.0 to 3.6.0", pr_number: 23904, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 5, deletions_count: 5},
		{sha: "a36ad74e5cdd0d9d254e66a3700ccba560260bce", date: "2025-10-03 03:58:51 UTC", description: "Bump actions/github-script from 7.0.1 to 8.0.0", pr_number: 23906, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 7, deletions_count: 7},
		{sha: "c21ff5b5b19a3c6caf3596b7d1fb85f0b1226bc9", date: "2025-10-03 00:10:12 UTC", description: "batch netlink-* dep updates", pr_number: 23920, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "d4f791d8f387da451b34a1cbea05888743ae92b3", date: "2025-10-03 04:21:45 UTC", description: "Bump warp from 0.3.7 to 0.4.2", pr_number: 23683, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 58, deletions_count: 7},
		{sha: "bff362389f2834e506f25a8454968ac1696e90a2", date: "2025-10-03 01:19:01 UTC", description: "bump sysinfo to 0.37.2", pr_number: 23921, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "12fd410016520652344b752ad6b6ae84cd21ccf9", date: "2025-10-03 05:40:03 UTC", description: "Bump ossf/scorecard-action from 2.4.2 to 2.4.3", pr_number: 23925, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "7cf189fe4aba3bf29acac9a20025bd9fc7625cfa", date: "2025-10-03 05:40:50 UTC", description: "Bump actions/setup-python from 5 to 6", pr_number: 23924, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9783640c57de3df503465b43c40d6e1e5bb40fd6", date: "2025-10-03 12:18:34 UTC", description: "Bump github/codeql-action from 3.30.5 to 3.30.6", pr_number: 23926, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a7b4dc34cbc9cfe791dfda6864627d6146059334", date: "2025-10-03 12:21:05 UTC", description: "Bump aws-actions/configure-aws-credentials from 4.3.1 to 5.0.0", pr_number: 23928, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "0fc7111eb9a4393b31cfab4b3b015e8243558944", date: "2025-10-03 20:59:55 UTC", description: "Bump actions/cache from 4.2.4 to 4.3.0", pr_number: 23927, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 6, deletions_count: 6},
		{sha: "f054f9c1371be239eeac85fb79ab20d101cb71df", date: "2025-10-04 00:40:41 UTC", description: "properly enable memory enrichment table for `vector tap`", pr_number: 23863, scopes: ["enrichment tables"], type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 2, insertions_count: 36, deletions_count: 2},
		{sha: "0c652ce808a73501bc7ca3e4a5d1de66e5e42472", date: "2025-10-04 00:32:06 UTC", description: "only run changed integrations in the MQ", pr_number: 23937, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 4},
		{sha: "deab79c5172837a44fcd0458c0b2215e6d1a456e", date: "2025-10-07 05:33:16 UTC", description: "fix empty collection rendering by isset", pr_number: 23945, scopes: ["external docs"], type: "docs", breaking_change: false, author: "Huang Chen-Yi", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "50f9a8c5dae8506e3ab7a11d61c43afe07982baa", date: "2025-10-07 05:58:05 UTC", description: "add requirement for docker logs source", pr_number: 23944, scopes: ["docker_logs source"], type: "docs", breaking_change: false, author: "Huang Chen-Yi", files_count: 1, insertions_count: 5, deletions_count: 1},
		{sha: "549381ecd2f84a7c3f03866ad3cd0a6decb2c54b", date: "2025-10-06 18:05:25 UTC", description: "bump VRL to latest sha", pr_number: 23947, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "a5fb0ecc511ad9fde1c79a68074f641bb916f84c", date: "2025-10-06 18:18:53 UTC", description: "show both author name and handle", pr_number: 23948, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 15, deletions_count: 1},
		{sha: "8dff36b9c06df052b4158557fd0b82df5aedf5ad", date: "2025-10-07 06:24:52 UTC", description: "fix `external` type in `networks` in docker compose file", pr_number: 23942, scopes: ["integration test"], type: "fix", breaking_change: false, author: "Huang Chen-Yi", files_count: 2, insertions_count: 4, deletions_count: 3},
		{sha: "5aa7244511bb282658a87053033928c1f80fbbc1", date: "2025-10-06 20:03:56 UTC", description: "merge both cue.sh scripts", pr_number: 23951, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 32, deletions_count: 105},
		{sha: "5c1e1ee7d0543ab8e4b4af85fca46c6c10140e0f", date: "2025-10-06 22:24:57 UTC", description: "Fix incorrect cue.sh path", pr_number: 23953, scopes: ["dev"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "59689d3c5ec29790eb5e109001a0c2993b062a39", date: "2025-10-07 10:34:24 UTC", description: "add new path related documents", pr_number: 23935, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Huang Chen-Yi", files_count: 3, insertions_count: 166, deletions_count: 0},
		{sha: "152cc39965b516840cdc2bc7d66dda3e9b97415b", date: "2025-10-06 23:13:29 UTC", description: "remove check-version script", pr_number: 23940, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 7, insertions_count: 1, deletions_count: 102},
		{sha: "cd2471ab3ed81cc55b13eb4f094af735a210b61e", date: "2025-10-06 23:24:58 UTC", description: "add highlights to typesense", pr_number: 23952, scopes: ["website"], type: "feat", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 20, deletions_count: 2},
		{sha: "90f59d5ca899ef4120840cdf2ffd4ddee3232328", date: "2025-10-07 00:05:28 UTC", description: "re-organize and improve aws guides", pr_number: 23954, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 9, deletions_count: 5},
		{sha: "6b807ef93ce21c70e143c4fb4c67d8baaca14a9b", date: "2025-10-07 00:33:24 UTC", description: "only run tests when change conditions are met", pr_number: 23939, scopes: ["ci"], type: "feat", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 7, deletions_count: 0},
		{sha: "6a8dccc503d0a9386a83a52c274085d435ceda42", date: "2025-10-08 08:04:42 UTC", description: " respect color flag for tests", pr_number: 23957, scopes: ["unit tests"], type: "feat", breaking_change: false, author: "Ivan Rozhnovskiy", files_count: 7, insertions_count: 56, deletions_count: 31},
		{sha: "515a54850a8b1bbd2bc8b5469d966c65adb16c0a", date: "2025-10-08 17:04:37 UTC", description: " respect color flag for tests", pr_number: 23964, scopes: ["unit tests"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 31, deletions_count: 56},
		{sha: "cdb9e3c2ea32101595d5129d980c1c68ac26260d", date: "2025-10-08 21:21:05 UTC", description: "run expensive Component Features check weekly", pr_number: 23963, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "f56549ca18ed3a6969ee70532eee50f0a85449b9", date: "2025-10-09 09:22:12 UTC", description: "fix warning for kubernetes logs source", pr_number: 23965, scopes: ["kubernetes_logs source"], type: "docs", breaking_change: false, author: "Huang Chen-Yi", files_count: 1, insertions_count: 4, deletions_count: 2},
		{sha: "b18ada85600888142703fe5f8276bc670d9330bc", date: "2025-10-08 21:43:05 UTC", description: "introduce `otlp` encoder ", pr_number: 23850, scopes: ["opentelemetry sink"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 48, insertions_count: 585, deletions_count: 110},
		{sha: "2527653b27302989b17666c3534285924c7106b8", date: "2025-10-09 10:30:05 UTC", description: "print error with `Debug` trait to improve the user diagnostic experience", pr_number: 23949, scopes: ["docker_logs source"], type: "feat", breaking_change: false, author: "Huang Chen-Yi", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4044e43bb7d57cfdbdc5ec1edd0ac356ddc5e73b", date: "2025-10-09 06:30:10 UTC", description: "respect color flag for tests (recreated)", pr_number: 23966, scopes: ["unit tests"], type: "feat", breaking_change: false, author: "Ivan Rozhnovskiy", files_count: 7, insertions_count: 56, deletions_count: 31},
		{sha: "9c0dffb72d42bb12120d6b3af96e3541094268ef", date: "2025-10-08 23:57:58 UTC", description: "separate vector-top into it's own module", pr_number: 23969, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 15, insertions_count: 113, deletions_count: 57},
		{sha: "33692fab7dfe897a7e4c9154a559017e4136e981", date: "2025-10-09 01:10:33 UTC", description: "use telemetrygen and delete custom log generator", pr_number: 23968, scopes: ["tests"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 66, deletions_count: 196},
		{sha: "68f0b4cf6a9c5fec461bf7b81617c889bcfc9ebb", date: "2025-10-09 01:18:58 UTC", description: "small vdev improvements and refactor", pr_number: 23912, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 12, deletions_count: 17},
		{sha: "3cd1a3135885f8e0007e7b73fb6eca137f02734d", date: "2025-10-10 01:46:36 UTC", description: "remove legacy checksum/fingerprinting", pr_number: 23874, scopes: ["file source"], type: "chore", breaking_change: false, author: "Thomas", files_count: 10, insertions_count: 68, deletions_count: 572},
		{sha: "a9d635a85c83072b744075e7e10cb38be1b06c79", date: "2025-10-10 18:33:07 UTC", description: "use ` instead of \" in aws guide", pr_number: 23983, scopes: ["website"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "64cb8fa7af4e67f7df30713654bbd3cdab7869ad", date: "2025-10-10 21:41:54 UTC", description: "add maxwidth format option", pr_number: 23985, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "2275c69b7ae5180373f44763bcd9fb7b16025d53", date: "2025-10-10 23:47:29 UTC", description: "latest-vector_default.yaml case was silently failing", pr_number: 23984, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 204, deletions_count: 105},
		{sha: "cbfcb8c182442df88b12e5096af2284d68752d82", date: "2025-10-10 21:54:10 UTC", description: "Add control for metric name splitting", pr_number: 23986, scopes: ["datadog_agent source"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 5, insertions_count: 498, deletions_count: 369},
		{sha: "a0fd6992cf3a02d7acb9aa7fffcc5999782fde2d", date: "2025-10-11 00:48:00 UTC", description: "scripts/run-integration-test.sh must fail early (not skip)", pr_number: 23977, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 49, deletions_count: 21},
		{sha: "0b75760095b019d3ae2caed1e596c4ce4dec85fc", date: "2025-10-11 01:58:12 UTC", description: "misc tests now run in parallel", pr_number: 23987, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 70, deletions_count: 15},
		{sha: "2e128dc63a7d495c9a320f443643b65a89ccc794", date: "2025-10-11 00:15:39 UTC", description: "Export the `top` function for external reuse", pr_number: 23988, scopes: ["top"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "ab8c4da85996c5c853017e38085a60e49ebb3fc5", date: "2025-10-13 18:02:00 UTC", description: "Add HEC indexer ack query compression", pr_number: 23823, scopes: ["splunk_hec sink"], type: "feat", breaking_change: false, author: "Scott Balmos", files_count: 2, insertions_count: 16, deletions_count: 11},
		{sha: "a121acf807d0acd0fda58d743ffb258094fecb54", date: "2025-10-14 19:35:55 UTC", description: "remove redundant setup steps", pr_number: 23999, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 34, deletions_count: 23},
		{sha: "7224315c3336c4d296006ba9997c4f985c6c9ceb", date: "2025-10-14 20:45:13 UTC", description: "add retry delay in sqs::Ingestor", pr_number: 23996, scopes: ["aws_sqs source"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 80, deletions_count: 11},
		{sha: "c1b8027680e8f5cd3d8725f889219b159b544d93", date: "2025-10-14 20:49:36 UTC", description: "refactoring - move code out of mod.rs", pr_number: 24000, scopes: ["codecs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 6, insertions_count: 811, deletions_count: 662},
		{sha: "e1ecf8e536e3ff424d32c25e5cff9ceac8f4ae27", date: "2025-10-15 01:18:36 UTC", description: "add 'use_json_names' options to protobuf codecs", pr_number: 24002, scopes: ["codecs"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 66, insertions_count: 847, deletions_count: 90},
		{sha: "d9e0e3af30f3dbefe50dfab97b8990b5e672e492", date: "2025-10-15 22:46:10 UTC", description: "improve output type sections", pr_number: 24006, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 16, insertions_count: 32, deletions_count: 32},
		{sha: "45093c9f8cc8769cd08eed6026c6dc7cfff44e77", date: "2025-10-15 22:39:31 UTC", description: "add flattened and unflattened key examples to datadog_search tests", pr_number: 24008, scopes: ["datadog service"], type: "chore", breaking_change: false, author: "Tess Neau", files_count: 1, insertions_count: 12, deletions_count: 0},
		{sha: "f8f23df24c7c0542cc0a935677c42b1fc52f248f", date: "2025-10-16 17:57:40 UTC", description: "enable wrap to help with long strings", pr_number: 24013, scopes: ["vrl playground"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "90648c96440bd9bedec5c8a66eec6622e6220d15", date: "2025-10-16 18:33:50 UTC", description: "fix timezone dropdown pop up", pr_number: 24015, scopes: ["vrl playground"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 13, deletions_count: 1},
		{sha: "73c468cd2df1b55d81ecafcc046019bdabfbf82b", date: "2025-10-16 18:57:31 UTC", description: "introduce OTLP decoder", pr_number: 24003, scopes: ["codecs"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 31, insertions_count: 598, deletions_count: 52},
		{sha: "fcb9dfdcc8f5f40afa3a34f9560831bd647762e3", date: "2025-10-16 22:23:55 UTC", description: "add e2e-tests should run filter", pr_number: 24016, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 57, deletions_count: 14},
		{sha: "1bed43c2fc907005f01da5ebfa65a8ec38641581", date: "2025-10-16 23:28:49 UTC", description: "remove build directives from datadog compose files", pr_number: 24018, scopes: ["e2e"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 0, deletions_count: 4},
		{sha: "dc60da02e07c58ae8d96059b2228c64fcd680c05", date: "2025-10-17 17:58:23 UTC", description: "refactor to avoid temp files and leverage docker APIs", pr_number: 23976, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 41, insertions_count: 408, deletions_count: 238},
		{sha: "778b94446db40ec65df0758cd1c5192d401a9955", date: "2025-10-17 19:46:12 UTC", description: "add signal priority option to OTLP decoder", pr_number: 24019, scopes: ["codecs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 27, insertions_count: 632, deletions_count: 60},
		{sha: "ea91a4d3661362cfdc9b570dfddc761a24556a1f", date: "2025-10-17 20:03:59 UTC", description: "parse_aws_alb_log strict_mode", pr_number: 24021, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 50, deletions_count: 0},
		{sha: "01b736903adf5012a81ecc32ba15cc0d7cdad4d4", date: "2025-10-17 20:25:44 UTC", description: "make workflows run when yml files change", pr_number: 24017, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 82, deletions_count: 21},
		{sha: "8c909f2641c25abd9eceebe271fd99584076380a", date: "2025-10-17 23:20:38 UTC", description: "improve internal_log_rate_limit docs", pr_number: 24023, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 14, deletions_count: 1},
		{sha: "09bdb9610a09c92c411133630071e5552b0870bf", date: "2025-10-20 20:50:33 UTC", description: "Run deny on nightly schedule", pr_number: 24029, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "359fc8a47420d3285a6dd4b83d9a6313a38de50b", date: "2025-10-20 21:07:33 UTC", description: "make labeler action glob ci files correctly", pr_number: 24030, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "c6fb99628eeed263a9265af08c378627d63ba36d", date: "2025-10-20 22:15:47 UTC", description: "add setup action", pr_number: 23707, scopes: ["ci"], type: "feat", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 259, deletions_count: 26},
		{sha: "0c99f1646c58d170f91db1827d9778e60f1dbbd4", date: "2025-10-22 08:39:16 UTC", description: "fix tls how it work", pr_number: 24036, scopes: ["external docs"], type: "docs", breaking_change: false, author: "Eric Huang", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "6f7ef56fab687db65808b24ffb8e54c67ee545f2", date: "2025-10-22 02:57:01 UTC", description: "add path configuration option", pr_number: 23956, scopes: ["prometheus_remote_write source"], type: "feat", breaking_change: false, author: "elohmeier", files_count: 3, insertions_count: 116, deletions_count: 1},
		{sha: "192dd25a3eb21e83912f054ae4a1b4d37cf3d3ba", date: "2025-10-21 22:52:13 UTC", description: "add workflow to build and push test runner image", pr_number: 24042, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 78, deletions_count: 0},
		{sha: "e162bda8216e4a9d1d73587b717766b28b08bacc", date: "2025-10-21 23:16:36 UTC", description: "add aggregated test detection outputs to changes.yml", pr_number: 24040, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 18, deletions_count: 7},
		{sha: "a9d244a7d5f07b3ee50dd8137ae0de8a5814e057", date: "2025-10-22 18:23:51 UTC", description: "fix environment image and add test", pr_number: 24033, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 44, deletions_count: 10},
		{sha: "770ae9643cb867b07444b2f9af6da899e1852fb0", date: "2025-10-22 19:57:01 UTC", description: "capture stderr and refactor", pr_number: 24045, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 43, deletions_count: 32},
		{sha: "13f77b9815c56e0355a6cd52801cafb01a8d6a2e", date: "2025-10-23 02:38:29 UTC", description: "prevent crash on config reload with enrichment table sources", pr_number: 24014, scopes: ["enrichment tables"], type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 3, insertions_count: 140, deletions_count: 20},
		{sha: "2ead14508c2e6c235a025263d38626fd6970526b", date: "2025-10-23 08:41:02 UTC", description: "fix docker client with specified socket path", pr_number: 24026, scopes: ["docker_logs source"], type: "fix", breaking_change: false, author: "Eric Huang", files_count: 2, insertions_count: 5, deletions_count: 1},
		{sha: "cd5d44276e77ece2e58ec9cf2d2e25ad9298ccef", date: "2025-10-22 21:05:44 UTC", description: "guides and highlights author/date fixes", pr_number: 24047, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 10, insertions_count: 68, deletions_count: 9},
		{sha: "61e7bf349c2c5858f96050ab2e82e96704cf1bcb", date: "2025-10-22 21:16:39 UTC", description: "Remove references to soak-builder", pr_number: 24032, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 3},
		{sha: "4cd5f675d3a40e648acd405353f1b4d3f631740a", date: "2025-10-22 21:48:13 UTC", description: "fix creation dates for a few md files", pr_number: 24048, scopes: ["website"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "9b775a572b235d8e89b9f0b554fc6b296274b3d5", date: "2025-10-22 21:53:38 UTC", description: "add --reuse-image flag for CI optimization", pr_number: 24041, scopes: ["vdev"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 13, insertions_count: 127, deletions_count: 23},
		{sha: "4c34fc60899d104ed4c571660f6e8e761e6ec719", date: "2025-10-23 10:42:26 UTC", description: "improve error handling for journald source by spawn new stderr handler", pr_number: 23941, scopes: ["journald source"], type: "feat", breaking_change: false, author: "Eric Huang", files_count: 2, insertions_count: 55, deletions_count: 12},
		{sha: "c6a6a85e3b55463db3eada8e400e46daab0c30aa", date: "2025-10-23 01:03:29 UTC", description: "optimize integration tests by reusing test-runner images", pr_number: 24052, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 97, deletions_count: 5},
		{sha: "5e792078f7e75f692720adba1fdc21c29d4cb636", date: "2025-10-23 01:37:20 UTC", description: "remove CARGO_NET_GIT_FETCH_WITH_CLI", pr_number: 24055, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 6, insertions_count: 0, deletions_count: 18},
		{sha: "6ec5b90bf8cf54ba6a5d1c780eab57651c7acbdd", date: "2025-10-23 18:38:26 UTC", description: "add RUST_BACKTRACE/CARGO_TERM_COLOR to setup action", pr_number: 24056, scopes: ["ci"], type: "feat", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 8, deletions_count: 2},
		{sha: "4bde7f97513ed48eb851fa26ae21363694ed4683", date: "2025-10-23 18:51:29 UTC", description: "Remove RUST_VERSION from int/e2e Dockerfile", pr_number: 24057, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 0, deletions_count: 2},
		{sha: "5e1c3bf2768af4130420f06cd742b4111ac2d561", date: "2025-10-24 01:24:55 UTC", description: "ignore conflicting metadata instead of returning HTTP 400", pr_number: 23773, scopes: ["prometheus_remote_write source"], type: "fix", breaking_change: false, author: "elohmeier", files_count: 5, insertions_count: 412, deletions_count: 41},
		{sha: "b08819ef49e221f0542da48291f4c9d84eae8994", date: "2025-10-23 20:15:04 UTC", description: " revert  \"Remove RUST_VERSION from int/e2e Dockerfile (#24057)\"", pr_number: 24062, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 2, deletions_count: 0},
		{sha: "98a8d4119612c353e4a036e14b9f90030d9e4ae5", date: "2025-10-23 23:36:34 UTC", description: "Add missing step to last needs of integration.yml", pr_number: 24065, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "c7889dba2f59e6ba6f54ea532f65a9a759208584", date: "2025-10-24 00:07:13 UTC", description: "update internal_log_rate_limit tags", pr_number: 24050, scopes: ["internal_logs source"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 110, insertions_count: 122, deletions_count: 367},
		{sha: "573241a1fe4ed280eb24fefbc3e927d236e4e2a3", date: "2025-10-24 00:29:21 UTC", description: "bump fakeintake version (updated sha)", pr_number: 23922, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "b9737f74a94260221298b1e29a06de8208825ad4", date: "2025-10-25 00:51:44 UTC", description: "reload transforms with external VRL on SIGHUP", pr_number: 23898, scopes: ["remap transform"], type: "feat", breaking_change: false, author: "Andrey Shibalov", files_count: 5, insertions_count: 34, deletions_count: 1},
		{sha: "f3d26082c81a0dc75c05c1d3dd432278ff3208b4", date: "2025-10-25 05:34:41 UTC", description: "watch-config file events handling", pr_number: 23899, scopes: ["config"], type: "fix", breaking_change: false, author: "Andrey Shibalov", files_count: 3, insertions_count: 32, deletions_count: 9},
		{sha: "52a1c65e96ff295094879634fb14ce33f0d2d6b8", date: "2025-10-24 23:24:02 UTC", description: "fix HTTP not decompressing payloads", pr_number: 24068, scopes: ["opentelemetry source"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 99, deletions_count: 59},
		{sha: "825f4a332846a806eaf9584d829a4e4f4241ae54", date: "2025-10-27 17:53:32 UTC", description: "import only if flag is set", pr_number: 24082, scopes: ["prometheus_remote_write source"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "834529fb97699cd89ffb9ef41377869e98a91dbe", date: "2025-10-27 19:31:09 UTC", description: "Remove RUST_VERSION from int/e2e Dockerfile", pr_number: 24083, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 8, deletions_count: 12},
		{sha: "a1ca14fb704d77d974a52c30bd9c82add879f0b8", date: "2025-10-27 20:16:44 UTC", description: "use builder pattern to avoid large list of arguments", pr_number: 24084, scopes: ["topology"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 116, deletions_count: 48},
		{sha: "e618fdfd6b38b981a9cda3fdd3018d28744fe573", date: "2025-10-27 21:19:28 UTC", description: "reuse code from util/http/encoding.rs", pr_number: 24071, scopes: ["datadog_agent source"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 8, insertions_count: 25, deletions_count: 36},
		{sha: "e732a6ebddd8640a56eceb2283c343846d5a7621", date: "2025-10-27 22:01:36 UTC", description: "refactor utilization.rs and add tests", pr_number: 24085, scopes: ["internal_metrics source"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 128, deletions_count: 26},
		{sha: "97c8d28c7765b3cf7dc519d60259703a0a9a0cfb", date: "2025-10-28 00:14:48 UTC", description: "corrects stop logic", pr_number: 24086, scopes: ["vdev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 5},
		{sha: "db89076a80aa7e48c866be9f847020fe66ba3e0c", date: "2025-10-28 00:50:52 UTC", description: "add user facing change explanation in PR template", pr_number: 24070, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "e43d490fba57820ea6b39f3f15f4c46a6a815dc2", date: "2025-10-28 00:58:00 UTC", description: "use official squid image", pr_number: 24090, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "64aaccac438efa6ab03bfc1f9d9a7dd60698d0c7", date: "2025-10-28 01:01:23 UTC", description: "disable config error log rate limit", pr_number: 24091, scopes: ["config"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "22cb8c5d83e469390a4c85fb269434dfd35154de", date: "2025-10-28 06:06:03 UTC", description: "prevent negative utilization on late messages", pr_number: 24073, scopes: ["metrics"], type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 2, insertions_count: 54, deletions_count: 4},
		{sha: "f6be7db2b3d2e357316d3e151ff410f8631118ba", date: "2025-10-28 21:59:44 UTC", description: "Fix local mqtt int test", pr_number: 24096, scopes: ["dev"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 2, deletions_count: 0},
		{sha: "11c55214bec4ae90c83dc12b00d817dbb0fb9ccb", date: "2025-10-29 18:21:44 UTC", description: "run mqtt int tests", pr_number: 24102, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "caf159276d07d8993c4b0a59f51f61696b2f7d23", date: "2025-10-29 18:45:55 UTC", description: "use one dockerfile for e2e and int", pr_number: 24101, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 43, deletions_count: 65},
		{sha: "31e8d2e03703c9ae879533d82e0112ba16b28165", date: "2025-10-29 21:27:08 UTC", description: "cache vdev", pr_number: 24103, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 56, deletions_count: 5},
		{sha: "6547b81f2a807f411bd81f65059cdea53e5976ec", date: "2025-10-29 22:56:10 UTC", description: "check modified files only for style", pr_number: 24106, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 48, deletions_count: 7},
		{sha: "9fdced84af203e07ba81e8052b634314b8b8e42d", date: "2025-10-30 18:02:29 UTC", description: "run K8s e2e test suite only on MQ", pr_number: 24110, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "fb74a1eab52c20b985b63ebd39d013b6f64c56c0", date: "2025-10-30 23:55:36 UTC", description: "prevent utilization metric loss on configuration reload", pr_number: 24080, scopes: ["metrics"], type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 4, insertions_count: 114, deletions_count: 45},
		{sha: "bfaefdc8d4bbf0d97cdc848e020f1559b9f37383", date: "2025-10-30 19:57:41 UTC", description: "vdev build on cache miss", pr_number: 24113, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "5d70d07c3806f09ddf0597374de4553ed1394399", date: "2025-10-30 19:51:26 UTC", description: "add opentelemetry metrics e2e tests", pr_number: 24109, scopes: ["dev"], type: "feat", breaking_change: false, author: "Thomas", files_count: 15, insertions_count: 707, deletions_count: 117},
		{sha: "ae8ad712906742fd77f38f58120db6af27cd757b", date: "2025-10-30 20:06:05 UTC", description: "multicast_and_unicast_udp_message no longer hangs on macOS", pr_number: 24112, scopes: ["dev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 8, deletions_count: 3},
		{sha: "d8abed57442322105ad05992368ec98f4c3227f6", date: "2025-10-30 21:47:38 UTC", description: "parallelize e2e tests (ci-integration-review)", pr_number: 24115, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 10, deletions_count: 27},
		{sha: "e486428e05061d4810ea943250afbe5167e08d97", date: "2025-10-30 21:22:52 UTC", description: "fix journald tests for local macOS", pr_number: 24114, scopes: ["dev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 5, deletions_count: 3},
		{sha: "42f71067ca5a9c9f989578bbce90ab84b503ecaf", date: "2025-10-30 21:49:31 UTC", description: "aws-kinesis-firehose tests", pr_number: 24117, scopes: ["dev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "086d8f3c928167e5290647a205132e3466549412", date: "2025-10-30 23:05:57 UTC", description: "add always build option to scripts/run-integration-test.sh", pr_number: 24120, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 9, deletions_count: 4},
		{sha: "d6421e32e38ed924893cc94ec3959c09fde16c33", date: "2025-10-31 19:26:35 UTC", description: "Update `dd-rust-license-tool` to v1.0.4", pr_number: 24122, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 112, deletions_count: 2},
		{sha: "8c5af6208d673f488e980ae866ae881946710e78", date: "2025-11-02 01:40:32 UTC", description: "fix path in datadog-metrics e2e test.yaml", pr_number: 24127, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "bd9b87700a4178decb54547bc158385156ad96f9", date: "2025-11-04 11:37:00 UTC", description: "Buffer counter underflowed (#23872)", pr_number: 23973, scopes: ["instrument"], type: "fix", breaking_change: false, author: "silas.u", files_count: 2, insertions_count: 22, deletions_count: 23},
		{sha: "5cf227e646734e13b47de0d559c353b20aae3461", date: "2025-11-03 22:37:13 UTC", description: "vdev caching", pr_number: 24126, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 20, deletions_count: 3},
		{sha: "aef66cfae8f68a7006b9c1cebba9ff022e0520da", date: "2025-11-04 11:57:01 UTC", description: "bump `avro-rs` crate to improve avro encoding error", pr_number: 24119, scopes: ["codecs"], type: "feat", breaking_change: false, author: "Eric Huang", files_count: 5, insertions_count: 210, deletions_count: 55},
	]
}
