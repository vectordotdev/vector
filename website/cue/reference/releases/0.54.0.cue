package metadata

releases: "0.54.0": {
	date:     "2026-03-10"
	codename: ""

	whats_next: []

	description: """
		The Vector team is excited to announce version `0.54.0`!

		## Release highlights
		- Enhanced `vector top` with new keybinds for scrolling, sorting, and filtering. Press `?` to
		  see all available keybinds.
		- The `datadog_logs` sink now defaults to `zstd` compression instead of no compression, resulting
		  in better network efficiency and higher throughput.
		- Added `component_latency_seconds` histogram and `component_latency_mean_seconds` gauge internal
		  metrics, exposing the time an event spends in a component.
		- Syslog encoding transform received major upgrades with improved RFC compliance, support for
		  scalars/nested objects/arrays in structured data, and better UTF-8 safety.
		- Added a new `azure_logs_ingestion` sink that supports the Azure Monitor Logs Ingestion API.
		  The existing `azure_monitor_logs` sink is now deprecated, and users should migrate before
		  Microsoft ends support for the old Data Collector API (currently scheduled for September 2026).

		## Breaking Changes

		- The `datadog_logs` sink now defaults to `zstd` compression. You can explicitly set `compression` to preserve
		  previous behavior.
		"""

	changelog: [
		{
			type: "fix"
			description: """
				Fixed a hard-to-trigger race between closing a memory buffer and outstanding
				sends that could rarely cause a lost event array at shutdown.
				"""
			contributors: ["bruceg"]
		},
		{
			type: "feat"
			description: """
				Add support for the Azure Monitor Logs Ingestion API through a new `azure_logs_ingestion` sink.

				The `azure_monitor_logs` sink is now deprecated, and current users will need to migrate to `azure_logs_ingestion` before Microsoft end support for the old Data Collector API (currently scheduled for September 2026).
				"""
			contributors: ["jlaundry"]
		},
		{
			type: "fix"
			description: """
				Remove the `tokio-util` patch override and preserve recoverable decoding behavior via `DecoderFramedRead`.
				"""
			contributors: ["Trighap52"]
		},
		{
			type: "enhancement"
			description: """
				The `clickhouse` sink now supports complex data types (Array, Map, and Tuple) when using the `arrow_stream` format.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "feat"
			description: """
				Added new keybinds to `vector top` for scrolling, sorting and filtering. You can now press `?` when using `vector top` to see all available keybinds.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: """
				The `log_to_metric` transform now correctly handles aggregated histogram and aggregated summary metrics.
				"""
			contributors: ["jblazquez"]
		},
		{
			type: "enhancement"
			description: """
				The `prometheus_remote_write` sink now supports the `healthcheck.uri` field to customize the healthcheck endpoint.
				"""
			contributors: ["simonhammes"]
		},
		{
			type: "fix"
			description: """
				Fixed recording of buffer utilization metrics to properly record on both send
				and receive in order to reflect the actual level and not just the "full" level.
				"""
			contributors: ["bruceg"]
		},
		{
			type: "fix"
			description: """
				The ClickHouse sink's ArrowStream format now correctly handles MATERIALIZED, ALIAS, EPHEMERAL, and DEFAULT columns. MATERIALIZED, ALIAS, and EPHEMERAL columns are excluded from the fetched schema since they cannot receive INSERT data. DEFAULT columns are kept but marked nullable so events are not rejected when the server-computed value is omitted.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "fix"
			description: """
				Fixed an issue where directory secret backends failed to resolve secrets organized in subdirectories
				(e.g., Kubernetes mounted secrets at paths like: `/secrets/my-secrets/username`)
				"""
			contributors: ["pront", "vparfonov"]
		},
		{
			type: "fix"
			description: """
				Fixed `vector test` printing literal `\\x1b` escape codes instead of rendering ANSI colors when reporting VRL compilation errors.
				"""
			contributors: ["thomasqueirozb"]
		},
		{
			type: "feat"
			description: """
				Added inode metrics to the `host_metrics` source filesystem collector on unix systems. The `filesystem_inodes_total`, `filesystem_inodes_free`, `filesystem_inodes_used`, and `filesystem_inodes_used_ratio` metrics are now available.
				"""
			contributors: ["mushrowan"]
		},
		{
			type: "enhancement"
			description: """
				Upgrades the syslog encoding transform with three major improvements:

				Structured Data Enhancements (RFC 5424):

				- Supports scalars
				- Handles nested objects (flattened with dot notation)
				- Serializes arrays as JSON strings, e.g., `tags="[\"tag1\",\"tag2\",\"tag3\"]"` (RFC 5424 spec doesn't define how to handle arrays in structured data)
				- Validates SD-ID and PARAM-NAME fields per RFC 5424
				- Sanitizes invalid characters to underscores

				UTF-8 Safety Fix:

				- Fixes panics from byte-based truncation on multibyte characters
				- Implements character-based truncation for all fields
				- Prevents crashes with emojis, Cyrillic text, etc.

				RFC 3164 Compliance Improvements:

				- Bug fix: Structured data is now properly ignored (previously incorrectly prepended)
				- TAG field sanitized to ASCII printable characters (33-126)
				- Adds debug logging when structured data is ignored
				"""
			contributors: ["vparfonov"]
		},
		{
			type: "enhancement"
			description: """
				The `arrow_stream` codec now uses `arrow-json` instead of `serde_arrow` for Arrow encoding.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "feat"
			description: """
				The `azure_blob` sink now supports routing requests through HTTP/HTTPS proxies, enabling uploads from restricted networks that require an outbound proxy.
				"""
			contributors: ["joshuacoughlan"]
		},
		{
			type: "enhancement"
			description: """
				Added the `component_latency_seconds` histogram and
				`component_latency_mean_seconds` gauge internal metrics, exposing the time an
				event spends in a single transform including the transform buffer.
				"""
			contributors: ["bruceg"]
		},
		{
			type: "enhancement"
			description: """
				The `datadog_logs` sink now defaults to `zstd` compression instead of no compression. This results in
				better network efficiency and higher throughput. You can explicitly set `compression = "none"` to
				restore the previous behavior of no compression, or set `compression = "gzip"` if you were previously
				using gzip compression explicitly.
				"""
			contributors: ["jszwedko", "pront"]
		},
		{
			type: "enhancement"
			description: """
				Add `content_encoding` and `cache_control` options to the `gcp_cloud_storage` sink. `content_encoding` overrides the `Content-Encoding` header (defaults to the compression scheme's content encoding). `cache_control` sets the `Cache-Control` header for created objects.
				"""
			contributors: ["benjamin-awd"]
		},
		{
			type: "fix"
			description: """
				The `opentelemetry` source now correctly uses `Definition::any()` for logs output schema when `use_otlp_decoding` is enabled.
				Users can now enable schema validation for this source.
				"""
			contributors: ["pront"]
		},
		{
			type: "enhancement"
			description: """
				Small optimization to the `websocket` source performance by avoiding getting a new time for every event in an array.
				"""
			contributors: ["bruceg"]
		},
		{
			type: "enhancement"
			description: """
				The `prometheus_remote_write` sink now supports custom HTTP headers via the `request.headers` configuration option. This allows users to add custom headers to outgoing requests, which is useful for authentication, routing, or other integration requirements with Prometheus-compatible backends.
				"""
			contributors: ["elohmeier"]
		},
		{
			type: "chore"
			description: """
				Removed the misleadingly-named `default-no-vrl-cli` feature flag, which did not control VRL CLI compilation.
				This flag was equivalent to `default` without `api-client` and `enrichment-tables`.
				Use `default-no-api-client` as a replacement (note: this includes `enrichment-tables`) or define custom features as needed.
				"""
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: """
				Added `internal_metrics` configuration section to the `tag_cardinality_limit` transform to better organize internal metrics configuration. The `internal_metrics.include_extended_tags` option controls whether to include extended tags (`metric_name`, `tag_key`) in the `tag_value_limit_exceeded_total` metric to help identify which specific metrics and tag keys are hitting the configured value limit. This option defaults to `false` because these tags have potentially unbounded cardinality.
				"""
			contributors: ["kaarolch"]
		},
		{
			type: "chore"
			description: """
				The `*buffer_utilization_mean` metrics have been enhanced to use time-weighted
				averaging which make them more representative of the actual buffer utilization
				over time.

				This change is breaking due to the replacement of the existing
				`buffer_utilization_ewma_alpha` config option with
				`buffer_utilization_ewma_half_life_seconds`.
				"""
			contributors: ["bruceg"]
		},
	]

	vrl_changelog: """
		### [0.31.0 (2026-03-05)]

		#### New Features

		- Added a new `parse_yaml` function. This function parses yaml according to the [YAML 1.1 spec](https://yaml.org/spec/1.1/).

		authors: juchem (https://github.com/vectordotdev/vrl/pull/1602)
		- Added `--quiet` / `-q` flag to the CLI to suppress the banner text when starting the REPL.

		authors: thomasqueirozb (https://github.com/vectordotdev/vrl/pull/1617)

		#### Fixes

		- Fixed a bug where lexer parse errors would emit a generic span with 202 error code instead of the
		proper error. Also fixed error positions from nested lexers (e.g., string literals inside function
		arguments) to correctly point to the actual location in the source.

		Before (generic E202 syntax error):

		```text
		$ string("\a")

		error[E202]: syntax error
		┌─ :1:1
		│
		1 │ string("\a")
		│ ^^^^^^^^^^^^ unexpected error: invalid escape character: \a
		│
		= see language documentation at https://vrl.dev
		= try your code in the VRL REPL, learn more at https://vrl.dev/examples
		```

		After (correct E209 invalid escape character):

		```text
		$ string("\a")

		error[E209]: invalid escape character: \a
		┌─ :1:10
		│
		1 │ string("\a")
		│          ^ invalid escape character: a
		│
		= see language documentation at https://vrl.dev
		= try your code in the VRL REPL, learn more at https://vrl.dev/examples
		```

		authors: thomasqueirozb (https://github.com/vectordotdev/vrl/pull/1579)
		- Fixed a bug where `parse_duration` panicked when large values overflowed during multiplication.
		The function now returns an error instead.

		authors: thomasqueirozb (https://github.com/vectordotdev/vrl/pull/1618)
		- Corrected the type definition of the `basename` function to indicate that it can also return `null`.
		Previously the type definition indicated that the function could only return bytes (or strings).

		authors: thomasqueirozb (https://github.com/vectordotdev/vrl/pull/1635)
		- Fixed incorrect parameter types in several stdlib functions:

		- `md5`: `value` parameter was typed as `any`, now correctly typed as `bytes`.
		- `seahash`: `value` parameter was typed as `any`, now correctly typed as `bytes`.
		- `floor`: `value` parameter was typed as `any`, now correctly typed as `float | integer`; `precision` parameter was typed as `any`, now correctly typed as `integer`.
		- `parse_key_value`: `key_value_delimiter` and `field_delimiter` parameters were typed as `any`, now correctly typed as `bytes`.

		Note: the function documentation already reflected the correct types.

		authors: thomasqueirozb (https://github.com/vectordotdev/vrl/pull/1650)

		### [0.30.0 (2026-01-22)]
		"""

	commits: [
		{sha: "c5f899575441a15598a76dd10c785074de93a0f7", date: "2026-01-28 19:32:30 UTC", description: "v0.53.0 release", pr_number: 24560, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 45, insertions_count: 311, deletions_count: 99},
		{sha: "7594b8adfd35cb43102ca0bde6aaf18fce14f5b1", date: "2026-01-28 22:20:15 UTC", description: "Add custom instrumentation hook", pr_number: 24558, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 32, deletions_count: 9},
		{sha: "0af6553bc542c1191aa94e48c14af4c50d4d588c", date: "2026-01-29 05:03:24 UTC", description: "match cla link to gh workflow", pr_number: 24565, scopes: ["internal"], type: "docs", breaking_change: false, author: "eldondevat", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "22e92a603ea2ddcd660752a1a774d459e6b74607", date: "2026-01-28 23:19:40 UTC", description: "Refactor EWMA + Gauge into a new struct", pr_number: 24556, scopes: ["observability"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 45, deletions_count: 16},
		{sha: "2da8b249974a82b9cfc9e1cd65ab0ca833dc1be5", date: "2026-01-28 21:32:08 UTC", description: "Use correct keys for histogram/summary", pr_number: 24394, scopes: ["log_to_metric transform"], type: "fix", breaking_change: false, author: "Javier Blazquez", files_count: 3, insertions_count: 34, deletions_count: 31},
		{sha: "5f1efed7b0eabfb3a4c189acc377ce235ad69d01", date: "2026-01-30 20:35:50 UTC", description: "Update crates and migrate to the new SDK", pr_number: 24255, scopes: ["azure_blob sink"], type: "feat", breaking_change: false, author: "Josh Coughlan", files_count: 14, insertions_count: 1046, deletions_count: 412},
		{sha: "9ba83a1c7e575793bba2d0b25b0787e6deddda54", date: "2026-01-30 22:20:55 UTC", description: "Two tiny optimizations", pr_number: 24520, scopes: ["sample transform"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 31, deletions_count: 44},
		{sha: "25c281903acf9db0fb7ae34c7f79f53f6535f2f7", date: "2026-01-30 22:21:57 UTC", description: "Micro-optimize send loop", pr_number: 24555, scopes: ["websocket source"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 8, deletions_count: 4},
		{sha: "94221283e89043533f4711175c75fa423fed076e", date: "2026-01-31 01:33:33 UTC", description: "minor release template improvements", pr_number: 24575, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "88638eebf8fbc467c4e158237a9ebd86e81a0ec7", date: "2026-02-03 01:44:52 UTC", description: "bump actions/cache from 5.0.1 to 5.0.3", pr_number: 24579, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "e512d47c6dc80269696bacf9b3b619bca57b20ab", date: "2026-02-02 20:57:55 UTC", description: "bump actions/checkout from 6.0.1 to 6.0.2", pr_number: 24581, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 26, insertions_count: 73, deletions_count: 73},
		{sha: "bae894dcd8912e0cb0774b602abcea2dc4213f12", date: "2026-02-02 18:20:36 UTC", description: "allow environment interpolation from http provider config", pr_number: 24341, scopes: ["http provider"], type: "enhancement", breaking_change: false, author: "John Sonnenschein", files_count: 1, insertions_count: 29, deletions_count: 3},
		{sha: "aba5fb479de401da11c512e734c7f23e295d9766", date: "2026-02-02 21:33:28 UTC", description: "bump github/codeql-action from 4.31.9 to 4.32.0", pr_number: 24580, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "1040f7844210c105233b792624b9949a63b76ee2", date: "2026-02-02 21:37:40 UTC", description: "bump docker/login-action from 3.6.0 to 3.7.0", pr_number: 24582, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "4a1eda5bcd3d68cbc73d89dd0e0d9ef0d94cfd54", date: "2026-02-03 11:38:28 UTC", description: "add support for Arrow complex types", pr_number: 24409, scopes: ["clickhouse sink"], type: "enhancement", breaking_change: false, author: "Benjamin Dornel", files_count: 11, insertions_count: 1783, deletions_count: 1846},
		{sha: "f77ab8ac21766d11ab6545c0759d11e649533ab5", date: "2026-02-02 22:59:19 UTC", description: "bump clap from 4.5.53 to 4.5.56 in the clap group across 1 directory", pr_number: 24500, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 8, deletions_count: 8},
		{sha: "0ddd610f28cd9c563c29dff69a328302ff205fc4", date: "2026-02-03 04:06:44 UTC", description: "bump the tokio group with 5 updates", pr_number: 24485, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 49, deletions_count: 112},
		{sha: "cb701eb23bdad0fac0f476687fd4e7cf1539d160", date: "2026-02-03 19:12:12 UTC", description: "Remove orphaned audit.yml", pr_number: 24584, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 0, deletions_count: 18},
		{sha: "d97292c0c5c3d91213bca60920ae843409ffec1a", date: "2026-02-04 01:41:14 UTC", description: "bump bytes from 1.10.1 to 1.11.1", pr_number: 24587, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 100, deletions_count: 100},
		{sha: "383c2ffa7ab7af97ffba2df438cbf24b3e0a4d88", date: "2026-02-05 03:15:48 UTC", description: "bump git2 from 0.20.2 to 0.20.4", pr_number: 24598, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "382437064afeb1a98121d927028852bb89204ea6", date: "2026-02-06 21:00:30 UTC", description: "Refactor output types into sub-module", pr_number: 24604, scopes: ["transforms"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 364, deletions_count: 359},
		{sha: "9338ee0ba39491a1a676e162109ad75decac825f", date: "2026-02-06 22:11:36 UTC", description: "Bump vrl and add description to parameters", pr_number: 24597, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Thomas", files_count: 11, insertions_count: 26, deletions_count: 1},
		{sha: "20360ac25e7a79730dd7ab39f5ca09ef48bfe52b", date: "2026-02-07 00:10:04 UTC", description: "bump time from 0.3.44 to 0.3.47", pr_number: 24608, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 9, deletions_count: 9},
		{sha: "e6df5ba13b847c00b90f0d6e03066c1946f97b4d", date: "2026-02-07 00:17:37 UTC", description: "Refactor transform builders into methods", pr_number: 24605, scopes: ["topology"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 149, deletions_count: 149},
		{sha: "2a8183a52393f46f51461b918175f893555fc273", date: "2026-02-07 06:34:41 UTC", description: "add defaults to Parameters and internal_failure_reasons to functions", pr_number: 24613, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Thomas", files_count: 12, insertions_count: 226, deletions_count: 154},
		{sha: "7cd3395e3049e1ce53d43e4aaf41415355c77bc4", date: "2026-02-10 05:11:50 UTC", description: "add scrolling, sorting and filtering to `vector top`", pr_number: 24355, scopes: ["cli"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 11, insertions_count: 926, deletions_count: 29},
		{sha: "1404ec385fa456742a1785d764f675762510057f", date: "2026-02-10 20:31:56 UTC", description: "bump axios from 1.13.2 to 1.13.5 in /website", pr_number: 24622, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 13, deletions_count: 13},
		{sha: "5e8715bfe91d16cdafec4c4736863b39ec164617", date: "2026-02-10 23:32:54 UTC", description: "bump diff from 4.0.2 to 4.0.4 in /website", pr_number: 24519, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "b523c6d677561fbfde8044680df33f706f844bc6", date: "2026-02-11 01:42:57 UTC", description: "Add transform latency metrics", pr_number: 24627, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 24, insertions_count: 751, deletions_count: 334},
		{sha: "726c383d704fb0e9018ccdc91b9d08831d499f21", date: "2026-02-11 07:59:20 UTC", description: "collect inode metrics", pr_number: 24625, scopes: ["host_metrics source"], type: "feat", breaking_change: false, author: "rowan", files_count: 3, insertions_count: 59, deletions_count: 7},
		{sha: "d360a8e8c1ee239e1428479cc0203da839eca230", date: "2026-02-11 09:11:18 UTC", description: "add support for `healthcheck.uri`", pr_number: 24603, scopes: ["prometheus_remote_write sink"], type: "enhancement", breaking_change: false, author: "Simon", files_count: 3, insertions_count: 13, deletions_count: 2},
		{sha: "8517809dad1f4ffdf9808fcf6992a70935951ffb", date: "2026-02-12 02:15:07 UTC", description: "[web-8160] upgrade typesense-sync to be v30 compatible", pr_number: 24640, scopes: ["website"], type: "chore", breaking_change: false, author: "Reda El Issati", files_count: 5, insertions_count: 259, deletions_count: 144},
		{sha: "5e02189cc4be072b8261fc9c3d1152f273cb5345", date: "2026-02-12 03:15:46 UTC", description: "Add missing newline to typesense-sync.ts", pr_number: 24642, scopes: ["website"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "5c8a811807ed5412fd45b9b01a58290591796ce6", date: "2026-02-12 06:40:54 UTC", description: "Bump vrl and add return_kind to functions", pr_number: 24614, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Thomas", files_count: 11, insertions_count: 41, deletions_count: 1},
		{sha: "d10f3a5b892833232002c97f24bf008dbef1eeec", date: "2026-02-13 10:37:50 UTC", description: "Bump VRL and implement category for functions", pr_number: 24653, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Thomas", files_count: 17, insertions_count: 93, deletions_count: 1},
		{sha: "3a73af45d530d5a7b95a34e1cfdb1d6213dd0274", date: "2026-02-14 03:50:40 UTC", description: "Add proxy support", pr_number: 24256, scopes: ["azure_blob sink"], type: "feat", breaking_change: false, author: "Josh Coughlan", files_count: 6, insertions_count: 41, deletions_count: 11},
		{sha: "b44dfee88b59b40d6602d68e3e18541d01954d46", date: "2026-02-18 02:14:02 UTC", description: "bump the patches group across 1 directory with 34 updates", pr_number: 24645, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 14, insertions_count: 451, deletions_count: 483},
		{sha: "692704adc1948e9a90e0dc51b52b557fa4e79619", date: "2026-02-18 02:14:04 UTC", description: "bump the aws group across 1 directory with 7 updates", pr_number: 24588, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 22, deletions_count: 34},
		{sha: "cc664d665ad55c223be788fc5281d28d7c14374a", date: "2026-02-17 22:26:59 UTC", description: "remove default-no-vrl-cli", pr_number: 24672, scopes: ["feature flags"], type: "chore", breaking_change: true, author: "Thomas", files_count: 3, insertions_count: 6, deletions_count: 1},
		{sha: "a51820e73030c65ede9e3afef252386746d5445d", date: "2026-02-18 00:19:16 UTC", description: "add integration tests for the top command", pr_number: 24649, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 663, deletions_count: 1},
		{sha: "76c78377ea3006c2dc29a740fdee6206b383566a", date: "2026-02-18 01:56:39 UTC", description: "bump the tracing group across 1 directory with 4 updates", pr_number: 24671, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 106, deletions_count: 106},
		{sha: "9aeae23363d1766542cb28ea3315b305cd84599d", date: "2026-02-18 19:26:19 UTC", description: "update cargo-deny to support CVSS version 4", pr_number: 24678, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "31ded018c26436b74b7526cb0509eecb0e57fd5a", date: "2026-02-19 03:31:27 UTC", description: "hardcode DOCKER_API_VERSION=1.44 in amazon-ecs-local-container-endpoints", pr_number: 24684, scopes: ["dev"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "26ae601da3e871711ec2922e7c0879155f1ec616", date: "2026-02-19 04:58:14 UTC", description: "update keccak to fix cargo-deny check", pr_number: 24679, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "0b25c7698393f42e09bf79858782cebb027b36e2", date: "2026-02-19 04:37:36 UTC", description: "do not skip the IT suite when ran manually", pr_number: 24683, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 11, deletions_count: 7},
		{sha: "6724b0a8f2af5403bdbe30d315895e3c434c5ed5", date: "2026-02-19 05:36:01 UTC", description: "Update num-bigint-dig 0.8.4 -> 0.8.6 to resolve future incompatibilities", pr_number: 24664, scopes: ["deps"], type: "chore", breaking_change: false, author: "zapdos26", files_count: 1, insertions_count: 2, deletions_count: 3},
		{sha: "2a220496a7b5af8ff94e25b0aaa1753a39182ec1", date: "2026-02-19 05:38:18 UTC", description: "bump VRL and use Parameter builder", pr_number: 24681, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Thomas", files_count: 11, insertions_count: 131, deletions_count: 180},
		{sha: "395c85f5ce2a99b325d7d4d07ef1c39b2a7ec1fa", date: "2026-02-19 19:17:44 UTC", description: "bump the csv group with 2 updates", pr_number: 24431, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "a6e37ca053dc8501113c37d1dad2b74555c6ef95", date: "2026-02-19 20:38:15 UTC", description: "Record buffer utilization on receive", pr_number: 24650, scopes: ["observability"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 102, deletions_count: 33},
		{sha: "90e3342359e1a59e61f2c0ad1de2cc6bf9c07fd9", date: "2026-02-19 23:00:20 UTC", description: "reference issue #24687 above DOCKER_API_VERSION hack", pr_number: 24696, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "e487d6ed6fa413f2ced27780227424f02472c799", date: "2026-02-19 23:36:15 UTC", description: "Fix K8s E2E test failures", pr_number: 24694, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 16, deletions_count: 4},
		{sha: "64caf5ee3b3e0e318c4f1a40dbc68cf6182b8d02", date: "2026-02-20 01:59:56 UTC", description: "Add K8s-related scripts to K8s change detection filter", pr_number: 24698, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "bd79aeca7946844a2fa29720cac34a9f62717e71", date: "2026-02-20 09:28:46 UTC", description: "add custom HTTP headers support", pr_number: 23962, scopes: ["prometheus_remote_write sink"], type: "feat", breaking_change: false, author: "elohmeier", files_count: 6, insertions_count: 172, deletions_count: 18},
		{sha: "adf1aba42b99acfb880f466d6f091a31b43d201d", date: "2026-02-21 10:34:40 UTC", description: "Initial `azure_logs_ingestion` sink", pr_number: 22912, scopes: ["azure_logs_ingestion sink"], type: "feat", breaking_change: false, author: "Jed Laundry", files_count: 19, insertions_count: 1832, deletions_count: 3},
		{sha: "cfad8051e27bd8682c27dbf4c085b7839fbc4615", date: "2026-02-20 20:41:45 UTC", description: "bump k8s and minikube versions", pr_number: 24699, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "89652601e0c512f6af52f048d9c53fdc38b826fa", date: "2026-02-20 19:45:58 UTC", description: "Fix race draining a memory buffer", pr_number: 24695, scopes: ["buffers"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 78, deletions_count: 1},
		{sha: "2cbcc1672771848cb5d9a543961c8e39b664f653", date: "2026-02-20 21:56:49 UTC", description: "default to zstd compression", pr_number: 19456, scopes: ["datadog_logs sink"], type: "enhancement", breaking_change: false, author: "Doug Smith", files_count: 8, insertions_count: 109, deletions_count: 26},
		{sha: "81e546cda71a6b903c3fc7b631518a9b338173a6", date: "2026-02-20 23:43:17 UTC", description: "update tracing in cargo lock", pr_number: 24703, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "68609dff0976bf0a60c1bbf25edc028d1432d085", date: "2026-02-21 00:11:09 UTC", description: "add dep update choice to PR template", pr_number: 24704, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 5, deletions_count: 3},
		{sha: "c101e3c696dafcad94bdbe612c8b08b3a8214d9c", date: "2026-02-21 00:39:58 UTC", description: "upload test results for make commands that use nextest", pr_number: 24680, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 19, deletions_count: 40},
		{sha: "cbbae138a299b6fd380d2b2d56d51bc1d858e264", date: "2026-02-21 01:18:43 UTC", description: "introduce AGENTS.md", pr_number: 23858, scopes: ["dev"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 365, deletions_count: 0},
		{sha: "5ce1198a2a4efef0102f8d401670215a3ca15612", date: "2026-02-21 06:55:24 UTC", description: "remove tokio-util patch dependency", pr_number: 24658, scopes: ["deps"], type: "fix", breaking_change: false, author: "Zyad Haddad", files_count: 21, insertions_count: 251, deletions_count: 53},
		{sha: "9465cec953ec69c6c213d30c8a5737805647cb69", date: "2026-02-21 03:15:48 UTC", description: "Time-weight buffer utilization means", pr_number: 24697, scopes: ["observability"], type: "enhancement", breaking_change: true, author: "Bruce Guenter", files_count: 13, insertions_count: 236, deletions_count: 55},
		{sha: "359b646c853c96eb5ca7ad81c1cbd88915c431a5", date: "2026-02-21 06:05:06 UTC", description: "fix top tests features and run them on CI", pr_number: 24677, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 9, insertions_count: 35, deletions_count: 5},
		{sha: "cb2112601d0b0a424b082a06c53fe3cd8879790e", date: "2026-02-23 19:27:36 UTC", description: "remove 'type: bug' label (now using 'type: Bug')", pr_number: 24711, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 2},
		{sha: "0a37c8cc251b63dd7a199174f3c06f4c729ad4cf", date: "2026-02-23 19:57:48 UTC", description: "remove 'type: feature' label (now using 'type: Feature')", pr_number: 24713, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 2},
		{sha: "541819c67b8e53e6fc5801468c2cf1bfbd456465", date: "2026-02-23 19:57:48 UTC", description: "remove last instance of BoxService", pr_number: 24707, scopes: ["splunk service"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 10, deletions_count: 5},
		{sha: "89b2c832552a2d63720c648a52c3b503968a5e6c", date: "2026-02-23 19:59:51 UTC", description: "use cargo hack to perform single feature compilation checks", pr_number: 23961, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 45, deletions_count: 113},
		{sha: "717a5690344c414c47e902f57bb3a8795c8cd54d", date: "2026-02-23 20:01:45 UTC", description: "install correct deny version", pr_number: 24712, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "82786f550309cd39d5c488c4dad3ed5c4da4ffa0", date: "2026-02-23 20:51:11 UTC", description: "bump vdev version", pr_number: 24714, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "023b6f7d26a771a516c42edd24a439b393f1fba2", date: "2026-02-23 21:50:14 UTC", description: "fix various inconsistencies", pr_number: 24715, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Thomas", files_count: 5, insertions_count: 114, deletions_count: 26},
		{sha: "dae71aee6c9a6afeb199d4fbb6a40af0333f897b", date: "2026-02-23 22:47:46 UTC", description: "declare versions in one place one - DRY", pr_number: 24716, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 68, deletions_count: 85},
		{sha: "41324d241bfb93cb6a699e29008a44d1087111bd", date: "2026-02-23 23:02:59 UTC", description: "remove windows build jobs restriction", pr_number: 24717, scopes: ["ci"], type: "enhancement", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 0, deletions_count: 6},
		{sha: "ef05c2106c77125e898d4dd6557ae67994d87b11", date: "2026-02-24 00:22:39 UTC", description: "bump datadog-ci version", pr_number: 24718, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "724bb428767c185d949686be1f77cd98288cb412", date: "2026-02-24 18:11:26 UTC", description: "add content_encoding and cache_control options", pr_number: 24506, scopes: ["gcp_cloud_storage sink"], type: "feat", breaking_change: false, author: "Benjamin Dornel", files_count: 4, insertions_count: 180, deletions_count: 4},
		{sha: "75c1c62ea0e9fda8dbc89ef7fdaf812a09537bb9", date: "2026-02-25 01:00:22 UTC", description: "bump nix to 0.31 and remove patch dependency", pr_number: 24725, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 28, deletions_count: 33},
		{sha: "148f035c86697697cdbb883b738a191d2918afb6", date: "2026-02-25 01:05:38 UTC", description: "render VRL examples' input", pr_number: 24726, scopes: ["website"], type: "feat", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 11, deletions_count: 1},
		{sha: "3c2bc02c91e7923aa6c01c68659f0ac5e9a45db1", date: "2026-02-25 02:00:40 UTC", description: "added tap tests", pr_number: 24724, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 13, insertions_count: 676, deletions_count: 193},
		{sha: "b1359dc7f17d269d8a30b825653cda3ea29678ee", date: "2026-02-25 19:04:51 UTC", description: "bundle dependabot aws-* security updates", pr_number: 24732, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "e3c52276ba8063c4edc21e20563e86ca53f83b1b", date: "2026-02-25 20:25:22 UTC", description: "set DD_API_KEY in test-make-command.yml to upload test results", pr_number: 24764, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "8cd6703f687fa8747bb2de495d5ed15658b1a4ed", date: "2026-02-26 02:57:44 UTC", description: "wrap enrichment errors in a custom type", pr_number: 24495, scopes: ["enrichment tables"], type: "chore", breaking_change: false, author: "Yoenn Burban", files_count: 10, insertions_count: 161, deletions_count: 83},
		{sha: "4738d4794f7ec6f9f038a36f5c2c420345ec2224", date: "2026-02-26 01:28:43 UTC", description: "bump colored from 3.0.0 to 3.1.1", pr_number: 24762, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "6462709411820963cb5e60e7f6d1fd3a419746d5", date: "2026-02-26 02:01:49 UTC", description: "bump github/codeql-action from 4.32.0 to 4.32.4", pr_number: 24735, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "64a292997b2c156c7e1353bd68be055cae83e5e8", date: "2026-02-25 21:19:08 UTC", description: "bump the tower group across 1 directory with 2 updates", pr_number: 24501, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 11, deletions_count: 38},
		{sha: "d83a7f4d9b062a0f959cd3fb94a4aef47a471ffa", date: "2026-02-26 09:22:02 UTC", description: "add support for default columns", pr_number: 24692, scopes: ["clickhouse sink"], type: "fix", breaking_change: false, author: "Benjamin Dornel", files_count: 3, insertions_count: 249, deletions_count: 51},
		{sha: "3ec39d22f937d74e38e63f7894d3bfbdcbef437c", date: "2026-02-26 03:27:14 UTC", description: "bump proptest from 1.8.0 to 1.10.0", pr_number: 24752, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 38, deletions_count: 12},
		{sha: "84599a6576883efb7ae86fdaf651ce4f05d20a18", date: "2026-02-26 03:31:06 UTC", description: "bump data-encoding from 2.9.0 to 2.10.0", pr_number: 24751, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0cf89052406cd96cebbb8d05a14711870ab971e9", date: "2026-02-26 03:42:37 UTC", description: "bump toml_edit from 0.22.27 to 0.23.9", pr_number: 24759, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 15, deletions_count: 2},
		{sha: "ec40b258134aae68644beac2c0fadbd8c0deb23b", date: "2026-02-26 05:20:33 UTC", description: "bump the aws group with 2 updates", pr_number: 24738, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "f839e42f8e52b9ab21421cd65e62a279b8dffe86", date: "2026-02-26 01:27:05 UTC", description: "bump the clap group with 2 updates", pr_number: 24739, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 10, deletions_count: 10},
		{sha: "10a5ca2c4c29c0ffadc2b9e5e5d45effcb70e18f", date: "2026-02-26 01:32:45 UTC", description: "add toml to codecs dev-dependencies", pr_number: 24766, scopes: ["dev"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "76c637fb00259106a08633d66d195aa1f0587b25", date: "2026-02-26 06:33:36 UTC", description: "bump arc-swap from 1.7.1 to 1.8.2", pr_number: 24749, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 3},
		{sha: "61c2b5b84b04df0ac994cf66b4691f9095d89edf", date: "2026-02-26 21:50:11 UTC", description: "consolidate features", pr_number: 24637, scopes: ["feature flags"], type: "chore", breaking_change: true, author: "Thomas", files_count: 1, insertions_count: 21, deletions_count: 14},
		{sha: "a367fc0e9a7fd5371e71a943c838107fa5697427", date: "2026-02-26 22:56:19 UTC", description: "distribute MIT-0 and Unicode-3.0 licenses", pr_number: 24775, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 55, deletions_count: 0},
		{sha: "53670b1a02b1a638956118ebfd6a231b4e24415a", date: "2026-02-27 05:05:43 UTC", description: "bump the patches group with 9 updates", pr_number: 24737, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 123, deletions_count: 124},
		{sha: "6dacc589c4c225283e53be36b0e095fed24738a0", date: "2026-02-27 06:41:45 UTC", description: "Add metric and tag name to tag_value_limit_exceeded_total metric", pr_number: 24236, scopes: ["tag_cardinality_limit transform"], type: "enhancement", breaking_change: false, author: "Karol Chrapek", files_count: 7, insertions_count: 113, deletions_count: 7},
		{sha: "7122b6871ed53176b5ce2bf529a376cc1a86b52d", date: "2026-02-27 06:48:57 UTC", description: "Add cue fmt during documentation generation", pr_number: 24771, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Karol Chrapek", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "1487b5304089467126006685df4d5684be8d0229", date: "2026-02-27 13:57:30 UTC", description: "replace `serde_arrow` with `arrow-json`", pr_number: 24661, scopes: ["codecs"], type: "enhancement", breaking_change: false, author: "Benjamin Dornel", files_count: 10, insertions_count: 352, deletions_count: 385},
		{sha: "331257fea2f875b67ac89a5a94c506dbadc56389", date: "2026-02-27 02:00:21 UTC", description: "Remove CI-only formatting check from check-docs.sh", pr_number: 24777, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 15, deletions_count: 14},
		{sha: "cb4a60a20ccbc2301850dbc8a0d513334a880bf1", date: "2026-02-27 20:06:39 UTC", description: "Export some transform config types", pr_number: 24776, scopes: ["transforms"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 54, deletions_count: 63},
		{sha: "63acf11933309335982052e0109f07d961bfc388", date: "2026-02-28 05:59:35 UTC", description: "advanced syslog Structured Data & RFC compliance fixes", pr_number: 24662, scopes: ["codecs"], type: "enhancement", breaking_change: false, author: "Vitalii Parfonov", files_count: 3, insertions_count: 458, deletions_count: 77},
		{sha: "ff4ebc74d4b069cd8e30e21c2b64f9c93afb67e4", date: "2026-02-27 23:47:28 UTC", description: "simplify publish workflow by consolidating duplicated jobs", pr_number: 24778, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 45, deletions_count: 422},
		{sha: "0a32a064f9c705cdd663c6314e2fcf9278a29c37", date: "2026-02-28 05:29:11 UTC", description: "bump memchr from 2.7.5 to 2.8.0", pr_number: 24755, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "1dd2b998ad7704e4a2d8bdf6b6788c3d12893565", date: "2026-02-28 05:32:52 UTC", description: "bump minimatch from 3.1.2 to 3.1.5 in /website", pr_number: 24783, scopes: ["website deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "4f026ab4903c200133d09e071eab3136f1eb23e0", date: "2026-02-28 05:37:50 UTC", description: "bump derive_more from 2.0.1 to 2.1.1", pr_number: 24744, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 18, deletions_count: 8},
		{sha: "fb645a43e6c2b6b7a9d93bae8b4bcd4615133745", date: "2026-02-28 05:41:39 UTC", description: "bump evmap from 10.0.2 to 11.0.0", pr_number: 24754, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 42, deletions_count: 3},
		{sha: "660e92d553c966f57daa471447ceb3048f24aa91", date: "2026-02-28 05:47:46 UTC", description: "bump smpl_jwt from 0.8.0 to 0.9.0", pr_number: 24757, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 18, deletions_count: 2},
		{sha: "8dfd20d1a4dd2b8f4618f5fd4d296a5caf4c66c2", date: "2026-02-28 06:04:41 UTC", description: "bump bytesize from 2.1.0 to 2.3.1", pr_number: 24758, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "c788f9359dfca3b2363ca99a565a3334455541da", date: "2026-02-28 02:24:47 UTC", description: "restrict GITHUB_TOKEN permissions in workflows", pr_number: 24785, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 16, deletions_count: 6},
		{sha: "33057c8db8a348bc9dca9fb26ed931510a3fcb25", date: "2026-03-03 01:27:01 UTC", description: "expose vrl functions flag", pr_number: 24630, scopes: ["deps"], type: "chore", breaking_change: false, author: "dd-sebastien-lb", files_count: 2, insertions_count: 8, deletions_count: 3},
		{sha: "722586174cdac164249c1939868a0078b7bd90b2", date: "2026-03-03 00:42:05 UTC", description: "update Cargo.lock", pr_number: 24825, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 5, deletions_count: 60},
		{sha: "8b603ddfc38e18c94d7d346c49042f057dfdacae", date: "2026-03-03 08:17:29 UTC", description: "support directory paths with path separators in secret keys", pr_number: 24824, scopes: ["security"], type: "fix", breaking_change: false, author: "Vitalii Parfonov", files_count: 5, insertions_count: 28, deletions_count: 3},
		{sha: "d90916abfd639af236a501cfaf26d7f6e3b8e3e0", date: "2026-03-03 01:22:12 UTC", description: "Delete obsolete LLVM/clang 9 RUSTFLAGS step", pr_number: 24826, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 8},
		{sha: "0b15473a3329819fa538340486309d95571227b2", date: "2026-03-03 01:28:15 UTC", description: "add new component docs guide", pr_number: 24823, scopes: ["internal"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 42, deletions_count: 5},
		{sha: "c0fc69ecc3ca6eba0f7601e578c3dc8306f10031", date: "2026-03-03 02:03:15 UTC", description: "specify hugo version", pr_number: 24829, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "f78d95d59e0a1832b0577bc6945a7187a3c0b789", date: "2026-03-03 21:56:13 UTC", description: "various agents md updates", pr_number: 24832, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 122, deletions_count: 103},
		{sha: "56dbc78d9d9176ac06b98d875ab64997e8083413", date: "2026-03-04 06:35:55 UTC", description: "bump the patches group with 4 updates", pr_number: 24788, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 22, deletions_count: 22},
		{sha: "7e97ead1775c1cadd882ca176aa2497d8b7ee1b5", date: "2026-03-04 00:16:26 UTC", description: "implement least privilege for GitHub Actions token permissions", pr_number: 24835, scopes: ["ci"], type: "chore", breaking_change: false, author: "Benson Fung", files_count: 20, insertions_count: 121, deletions_count: 32},
		{sha: "ecd132d6cdde1c7e9cea506cbbd37d7af871d116", date: "2026-03-04 21:40:57 UTC", description: "Bump VRL and add check_type_only: false", pr_number: 24836, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 6, insertions_count: 10, deletions_count: 9},
		{sha: "51c04e029e1a81c5f036dc14ba0deeb22d5ecb20", date: "2026-03-05 01:10:59 UTC", description: "stop printing literal escaped ANSI codes to output", pr_number: 24843, scopes: ["unit tests"], type: "fix", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 60, deletions_count: 3},
		{sha: "11cd0a573886db17bb0688063fd91efdde7882c2", date: "2026-03-05 02:20:42 UTC", description: "Enable all vector-vrl-functions features by default", pr_number: 24845, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 3, deletions_count: 3},
		{sha: "9ff9d838a5a88a1b0e660ddfcd65283a1fdc1543", date: "2026-03-05 02:21:33 UTC", description: "explicitly enable preserve_order feature for serde_json", pr_number: 24846, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "7091366c9946363a57f3d6f37e40d9b1822710a4", date: "2026-03-05 02:54:56 UTC", description: "fix source output", pr_number: 24847, scopes: ["opentelemetry source"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 12, deletions_count: 1},
		{sha: "19edb2578b280f6b2ea40d334d75756c55affd16", date: "2026-03-05 18:53:21 UTC", description: "bump the artifact group with 2 updates", pr_number: 24820, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 29, deletions_count: 29},
		{sha: "6c158da72b870f17e7adf7aa07175b8408695fdd", date: "2026-03-05 20:14:41 UTC", description: "fix aggregate_vector_metrics docs and improve enrichment explainer", pr_number: 24849, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a175af1532b7489c43932fac0d612380a3888dfb", date: "2026-03-06 00:57:57 UTC", description: "fix website token permissions", pr_number: 24853, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "f119e7883af91d77e711c740316efaa04397604f", date: "2026-03-06 01:35:00 UTC", description: "add disk space cleanup to component features workflow", pr_number: 24852, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "3037b0c55f2d4407291260f914b2d4e82d2b682c", date: "2026-03-06 02:14:01 UTC", description: "move VRL-specific crates under lib/vector-vrl/", pr_number: 24854, scopes: ["dev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 29, insertions_count: 19, deletions_count: 19},
		{sha: "664a0a2cb897bacae2b5f13455626506f85b6072", date: "2026-03-06 02:51:26 UTC", description: "remove ux-team", pr_number: 24850, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "71b993593e28dfc0d2a787d6616fd28733ef19ae", date: "2026-03-06 20:42:44 UTC", description: "remove gardener workflows", pr_number: 24857, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 0, deletions_count: 204},
		{sha: "3f17b6f055b84721539c945e5c4f7d5cc826e55c", date: "2026-03-06 21:09:40 UTC", description: "document api as a global option", pr_number: 24858, scopes: ["website"], type: "docs", breaking_change: false, author: "Thomas", files_count: 5, insertions_count: 47, deletions_count: 72},
		{sha: "66e531e3080c96dda03d0ae5ac2845ad98fce728", date: "2026-03-06 21:27:55 UTC", description: "tighten changelog workflow security", pr_number: 24859, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 23},
		{sha: "c65983531e90ed95054477880505feba03dbf9a2", date: "2026-03-07 01:03:28 UTC", description: "update npm CI packages", pr_number: 24861, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "40e82911e30e798efa497c8149d31db99ae1b729", date: "2026-03-07 01:53:19 UTC", description: "update lading to 0.31.2", pr_number: 24855, scopes: ["ci"], type: "chore", breaking_change: false, author: "George Hahn", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "70a8bfaa65532566c71027246bc1881ea3f7bb2f", date: "2026-03-07 03:01:36 UTC", description: "remove docker dependency from deb/rpm package targets", pr_number: 24864, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 4, insertions_count: 14, deletions_count: 15},
		{sha: "1fe79946ecf66cc8bd050b24882edded08b79dbd", date: "2026-03-07 03:15:14 UTC", description: "use VDEV env var in scripts", pr_number: 24862, scopes: ["ci"], type: "chore", breaking_change: false, author: "Thomas", files_count: 11, insertions_count: 41, deletions_count: 15},
		{sha: "46a7035d7630a804fc9865d798319e2707816d4f", date: "2026-03-07 07:35:28 UTC", description: "revert markdownlint version bump", pr_number: 24867, scopes: ["deps"], type: "chore", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "2debb993f31a39968334236bd58e36d6504ee9b0", date: "2026-03-07 19:54:34 UTC", description: "update SMP CLI to 0.26.1", pr_number: 24865, scopes: ["ci"], type: "chore", breaking_change: false, author: "George Hahn", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b519edd97252ae1dab8210cfc3d164dadee16c9e", date: "2026-03-07 23:41:20 UTC", description: "Automatically generate VRL function documentation", pr_number: 24719, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Thomas", files_count: 240, insertions_count: 1263, deletions_count: 11380},
		{sha: "8dec725817de08b20ac834f5b281d25632e0de09", date: "2026-03-09 19:30:55 UTC", description: "replace check-component-docs with check-generated-docs", pr_number: 24871, scopes: ["internal docs"], type: "fix", breaking_change: false, author: "Thomas", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "c4c802270f75e5674cb6476a32490dceac778732", date: "2026-03-09 21:00:14 UTC", description: "Bump version to 0.3.0", pr_number: 24872, scopes: ["vdev"], type: "chore", breaking_change: false, author: "Thomas", files_count: 3, insertions_count: 3, deletions_count: 3},
		{sha: "79999f6cf8d482e4264c82f4d215a99d88dae8c6", date: "2026-03-09 22:39:27 UTC", description: "render all top level configuration fields", pr_number: 24863, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 22, insertions_count: 1755, deletions_count: 943},
		{sha: "f2c50cbad476eaf8b0679f19354188b60bb2affb", date: "2026-03-09 23:01:50 UTC", description: "add changes job to integration-test-suite needs to catch cancellations", pr_number: 24875, scopes: ["ci"], type: "fix", breaking_change: false, author: "Thomas", files_count: 1, insertions_count: 1, deletions_count: 0},
	]
}
