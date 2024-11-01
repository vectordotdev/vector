package metadata

releases: "0.33.0": {
	date:     "2023-09-27"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.33.0!

		Be sure to check out the [upgrade guide](/highlights/2023-09-27-0-33-0-upgrade-guide) for
		breaking changes in this release.

		This release marks the switch of the default configuration language for Vector from TOML to
		YAML. We continue to support both (as well as JSON), but the documentation will prefer YAML
		in its configuration examples. We think this will lead to more legible configurations as
		well as more consistency with Vector's Helm chart which requires YAML configuration. See the
		associated [release highlight](/highlights/2023-08-30-yaml-default-format) for more details
		on the motivation for this change.

		In addition to the usual enhancements and bug fixes, this release includes also includes:

		- A new AWS SNS sink

		"""

	known_issues: [
		"The Debian package installer will overwrite existing `/etc/default/vector` and `/etc/vector/vector.yaml` files. This was fixed in v0.33.1.",
		"The `clickhouse` sink health check will fail due to the accidentally inclusion of an extra `/` in the request URI. This was fixed in v0.34.0.",
	]

	changelog: [
		{
			type: "feat"
			scopes: ["exec source"]
			description: """
				The `exec` source now allows customization of the environment variables exposed to
				the subprocess via two new options:

				- `clear_environment` to remove environment variables propagated from Vector process
				- `environment` to set custom environment variables for the subprocess
				"""
			contributors: ["hhromic"]
			pr_numbers: [18223]
		},
		{
			type: "fix"
			scopes: ["socket source"]
			description: """
				The type definition for the `port` field included on events emitted from the
				`socket` source is now correctly marked as an integer rather than a string. This
				removes unnecessary casting when the `log_namespacing` feature is enabled.
				"""
			pr_numbers: [18180]
		},
		{
			type: "fix"
			scopes: ["elasticsearch sink"]
			description: """
				The `elasticsearch` sink now correctly ignores the `pipeline` configuration when it
				is an empty string (`""`) to allow disabling this option. Previously it would result
				in failed Elasticsearch requests due to an empty pipeline name being passed.
				"""
			pr_numbers: [18248]
		},
		{
			type: "enhancement"
			scopes: ["cli"]
			description: """
				Vector now has the ability to disable OpenSSL probing for remote certificates via
				`--openssl-no-probe` (or `VECTOR_OPENSSL_NO_PROBE`).
				"""
			contributors: ["hhromic"]
			pr_numbers: [18229]
		},
		{
			type: "fix"
			scopes: ["kubernetes_logs source"]
			description: """
				Fix log event generation from the `kubernetes_logs` source when `log_namespacing` is
				enabled. Previously it would emit empty log events.
				"""
			pr_numbers: [18244]
		},
		{
			type: "fix"
			scopes: ["dedupe transform"]
			description: """
				When `log_namespacing` is enabled, the `dedupe` transform can now access metadata
				fields using the `%some_field` syntax.
				"""
			pr_numbers: [18241]
		},
		{
			type: "fix"
			scopes: ["sample transform"]
			description: """
				When `log_namespacing` is enabled, the `sample` transform now adds the `sample_rate`
				field to metadata rather than to the event itself.
				"""
			pr_numbers: [18259]
		},
		{
			type: "enhancement"
			scopes: ["config"]
			description: """
				Vector now parses more configuration fields that represent "event paths" (like the
				`kafka` source `key_field`) at config parse time rather than at runtime. This
				results in earlier surfacing of errors.
				"""
			pr_numbers: [18188]
		},
		{
			type: "feat"
			scopes: ["aws_sns sink"]
			description: """
				A new `aws_sns` sink has been added to send events to AWS SNS.
				"""
			contributors: ["wochinge"]
			pr_numbers: [18259]
		},
		{
			type: "enhancement"
			scopes: ["http_server source"]
			description: """
				The `http_server` source can now have the response code it sends configured via the
				new `response_code` configuration parameter.
				"""
			contributors: ["kunalmohan"]
			pr_numbers: [18208]
		},
		{
			type: "enhancement"
			scopes: ["route transform"]
			description: """
				The `route` transform can now have the `_unmatched` route disabled via the
				`reroute_unmatched` parameter. This helps with suppressing the warning that is
				emitted if this output is not consumed by any other components.
				"""
			contributors: ["hhromic"]
			pr_numbers: [18309]
		},
		{
			type: "chore"
			scopes: ["config"]
			description: """
				The default configuration language for Vector was updated from TOML to YAML. See the
				[highlight](/highlights/2023-08-30-yaml-default-format) for more details on the
				motivation for this change.

				As part of this migration, Vector will prefer the `/etc/vector/vector.yaml` as the
				default location in the future rather than `/etc/vector/vector.toml`.
				"""
			pr_numbers: [18325, 18632, 18606, 18388, 18502, 18435, 18345, 18420, 18378]
		},
		{
			type: "enhancement"
			scopes: ["websocket sink"]
			description: """
				The `websocket` sink now accepts any data type that the configured codec accepts.
				For example, this means it supports logs, metrics, and traces when the `native` or
				`native_json` codecs are in use.
				"""
			pr_numbers: [18295]
		},
		{
			type: "fix"
			scopes: ["remap transform"]
			description: """
				When `log_namespacing` is enabled, the `remap` transform now correctly handles
				converting an array result into log events by avoiding wrapping non-object array
				elements as `{"message": "<the value>"}` and instead using the value directly as the
				log event.
				"""
			pr_numbers: [18372]
		},
		{
			type: "fix"
			scopes: ["codec"]
			description: """
				When `log_namespacing` is enabled, the `json` codec will now allow decoding of
				non-object values to create the log event rather than erroring if the incoming
				value is not an object.
				"""
			pr_numbers: [18379]
		},
		{
			type: "enhancement"
			scopes: ["releasing", "arm"]
			description: """
				We now publish `armv7hl` RPM packages. These are the same as the `armv7` packages
				but are more accurately named per the RPM packaging guidelines.

				As part of this change we are deprecating the `armv7` packages. This change should
				be transparent to users using `yum` but if you are mirroring or directly downloading
				the RPM files you will want to switch to the new naming scheme:
				`vector-<version>-1.armv7hl.rpm`.

				See the [upgrade guide](/highlights/2023-09-27-0-33-0-upgrade-guide#armv7-rename)
				for more details.
				"""
			pr_numbers: [18387]
			breaking: true
		},
		{
			type: "enhancement"
			scopes: ["kubernetes_logs source"]
			description: """
				The `kubernetes_logs` source now has an `oldest_first` option to configure the
				source to always consume the oldest file first. This is the same option that exists
				on the `file` source. It can enable better behavior by releasing file handles to
				rotated files before moving on to reading newer files.
				"""
			pr_numbers: [18376]
		},
		{
			type: "enhancement"
			scopes: ["aws provider"]
			description: """
				AWS components now support the [AWS FIPS
				endpoints](https://docs.aws.amazon.com/sdkref/latest/guide/feature-endpoints.html)
				by specifying `use_fips_endpoint` in your shared AWS config file or by setting
				the `AWS_USE_FIPS_ENDPOINT` environment variable.

				**Note** this does not yet work when accessing the STS endpoints to fetch
				authentication credentials. This is being tracked by
				[#18382](https://github.com/vectordotdev/vector/issues/18382).
				"""
			pr_numbers: [18390]
		},
		{
			type: "enhancement"
			scopes: ["codec"]
			description: """
				The `csv` codec now allows additional configuration options when encoding:

				- `capacity`: the capacity of the internal buffer in bytes (default 4098)
				- `delimiter`: the delimiter to use (defaults to `,`)
				- `double_quote`: when enabled (default) escapes double quotes by doubling them (`"`
				   is encoded as `""`). If disabled, then uses the configured `escape` character to
				   escape them instead.
				- `escape`: The escape character to use when escaping quotes (defaults to `\\`).
				   Only applies when `double_quote` is false.
				"""
			contributors: ["scMarkus"]
			pr_numbers: [18320]
		},
		{
			type: "fix"
			scopes: ["codec"]
			description: """
				The `gelf` codec now defaults to framing using null byte (`\\0`) when encoding.
				Previously it defaulted to newline (`\\n`) but the GELF server implementation
				[expects the null byte](https://github.com/Graylog2/graylog2-server/issues/1240).
				"""
			contributors: ["MartinEmrich"]
			pr_numbers: [18419]
		},
		{
			type: "fix"
			scopes: ["releasing"]
			description: """
				The published Debian packages no longer contain a `conffiles` control file as it was
				unnecessary (all configuration files are under `/etc` and are automatically flagged
				as conffiles by `dh_installdeb`).

				This existing `conffiles` file was contained an invalid trailing empty line which
				caused issues on some package managers such as Uyuni with SUSE Manager.
				"""
			pr_numbers: [18455]
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				The `component_errors_total` metric is no longer incremented for HTTP client errors
				that are automatically retried.
				"""
			pr_numbers: [18505]
		},
		{
			type: "fix"
			scopes: ["metrics"]
			description: """
				Vector avoids a panic that occurred when attempting to encode an empty sketch metric.
				"""
			pr_numbers: [18530]
		},
		{
			type: "fix"
			scopes: ["new_relic sink"]
			description: """
				A few fix related fixes to the New Relic sink handling of metrics were made:

				- Metric tags are sent to New Relic as attributes
				- The metric type is sent rather than New Relic always treating the metrics as gauges
				"""
			contributors: ["asllop"]
			pr_numbers: [18151]
		},
		{
			type: "fix"
			scopes: ["performance"]
			description: """
				Vector now defaults the worker thread concurrency to the detection provided by the
				Rust standard library, which attempts to take into account constraints applied by
				containerization, rather than just the number of detected CPUs. This should result
				in better resource utilization in container environments.

				See the [upgrade
				guide](/highlights/2023-09-27-0-33-0-upgrade-guide#runtime-worker-threads) for
				more details about this.
				"""
			pr_numbers: [18541]
		},
		{
			type: "chore"
			scopes: ["datadog_logs sink"]
			description: """
				The `datadog_logs` `endpoint` configuration is now treated as the "base URL" where
				the expected path, `/api/v2/logs` is appended. This makes this option more
				consistent with the `endpoint` options appearing on the other Datadog sinks.

				See the [upgrade
				guide](/highlights/2023-09-27-0-33-0-upgrade-guide#datadog-logs-endpoint) for
				details.
				"""
			pr_numbers: [18497]
			breaking: true
		},
		{
			type: "fix"
			scopes: ["platform"]
			description: """
				The published Vector artifacts are now compiled with an allocator page size of 64 kb
				rather than 4 kb. This allows Vector to run on systems with 64 kb pages (e.g. CentOS
				7/8 when on AARCH64) as well as continuing to run on systems with the more
				typical page size of 4 kb.
				"""
			pr_numbers: [18497]
		},
		{
			type: "fix"
			scopes: ["observability"]
			description: """
				Warnings about maximum allocation groups are now suppressed when the allocation
				tracking feature is not in use.
				"""
			pr_numbers: [18589]
		},
		{
			type: "fix"
			scopes: ["gcp provider"]
			description: """
				GCP components will now automatically retry unauthorized responses. The expectation
				is that users would rather intervene than drop data in this scenario.
				"""
			pr_numbers: [18589]
		},
		{
			type: "enhancement"
			scopes: ["security"]
			description: """
				The deprecated legacy OpenSSL provider support now defaults to disabled. It can be
				enabled via `--openssl-legacy-provider=true`.

				The `--openssl-legacy-provider` flag will be removed in a future release but loading
				of this provider will still be available via `OPENSSL_CONF` as described in the
				[upgrade guide](/highlights/2023-09-27-0-33-0-upgrade-guide#openssl-legacy-provider).
				"""
			pr_numbers: [18609]
			breaking: true
		},
		{
			type: "enhancement"
			scopes: ["datadog_agent source", "datadog_metrics sink"]
			description: """
				The `datadog_metrics` sink now transmits the origin of the metric to Datadog for display
				in metrics explorer. Metrics from the `datadog_agent` source have their origin passed
				through but otherwise an appropriate origin header is set for other metrics sources.
				"""
			pr_numbers: [18405]
		},
		{
			type: "enhancement"
			scopes: ["codec"]
			description: """
				Vector's protobuf definition for events was updated to have a new field to store
				metadata. Most users will not care about this, but if you are consuming Vector's
				protobuf encoded events directly please see [upgrade
				guide](/highlights/2023-09-27-0-33-0-upgrade-guide#vector-proto-metadata).
				"""
			pr_numbers: [18405]
		},
		{
			type: "enhancement"
			scopes: ["kafka sink"]
			description: """
				The `kafka` sink now builds requests concurrently which is expected to improve
				performance.
				"""
			pr_numbers: [18634]
		},
		{
			type: "fix"
			scopes: ["sinks"]
			description: """
				The following sinks now avoid ballooning in memory consumption in the pretense of
				back-pressure by limiting request building concurrency:

				- `amqp`
				- `appsignal`
				- `azure_monitor_logs`
				- `clickhouse`
				- `gcp_stackdriver`
				- `kafka`
				- `honeycomb`
				- `http`
				- `nats`
				- `pulsar`
				"""
			pr_numbers: [18634, 18637]
		},
		{
			type: "enhancement"
			scopes: ["journald source"]
			description: """
				The `journald` source now has an `extra_args` configuration option to allow
				specifying additional arguments to pass through to `journalctl` when fetching
				events.
				"""
			pr_numbers: [18568]
		},
		{
			type: "fix"
			scopes: ["concurrency"]
			description: """
				The `request.concurrency`  had a couple of tweaks to fix the documentation:

				- A value of `none` now configures "no concurrency" (same as `1`). This
				  was the documented behavior of `none`, but previously it would actually configure
				  adaptive concurrency.
				- As `none` was the default for most sinks, to maintain the expected default
				  behavior of adaptive concurrency, the default for `request.concurrency` for these
				  sinks is now `adaptive` rather than `none`.

				See the [upgrade
				guide](/highlights/2023-09-27-0-33-0-upgrade-guide#request-concurrency) for more
				details.
				"""
			pr_numbers: [18568]
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				VRL was updated to v0.7.0 which brings the following:

				Bug Fixes:

				- `parse_nginx_log` doesn't fail if the values of key-value pairs in error logs are missing
				- `encode_gzip` and `encode_zlib` now correctly check that the compression level is valid, preventing a panic
				- type definitions for arrays and objects with undefined values was improved
				- `parse_aws_vpc_flow_log now` handles account-id value as a string, avoiding loss of leading zeros and handling the case where value is unknown

				Features:

				- `parse_key_value` can now parse values enclosed in single quote characters
				- added `pretty` parameter for `encode_json` function to produce pretty-printed JSON string
				- added `community_id` function for generation of [Community IDs](https://github.com/corelight/community-id-spec)
				- `parse_aws_vpc_flow_log` can now handle VPC logs using version 5 fields
				- the deprecated `to_timestamp` function was removed (use `parse_timestamp` and `from_unix_timestamp` instead)
				- the `truncate` function now takes a `suffix` argument to control the suffix to append. This deprecates the existing `ellipsis` function
				"""
			pr_numbers: [999999]
		},
	]

	commits: [
		{sha: "e27684c6e05d3561a96c652fdd8662285c08dcf8", date: "2023-08-15 09:01:28 UTC", description: "add support for customizing command environment", pr_number: 18223, scopes: ["exec source"], type: "feat", breaking_change: false, author: "Hugo Hromic", files_count: 2, insertions_count: 126, deletions_count: 0},
		{sha: "b0c89ab111ace1387acc85e2b2ca0e97217d9325", date: "2023-08-15 06:30:08 UTC", description: "socket tcp port typedef", pr_number: 18180, scopes: ["config"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "1b90398cbd0442c1cd639e736d6785c9cd790d49", date: "2023-08-15 07:54:44 UTC", description: "temporarily ignore `ed25519-dalek` security vulnerability", pr_number: 18245, scopes: ["deps", "security"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 5, deletions_count: 0},
		{sha: "d9dbed8896793016deb48808e59cb200c4e641a0", date: "2023-08-15 22:58:36 UTC", description: "Ignore `pipeline` argument if it is an empty string", pr_number: 18248, scopes: ["elasticsearch"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 25, deletions_count: 1},
		{sha: "ef51e7a52e0fadea78b0f68c78d4bee78d1fb6bc", date: "2023-08-16 00:05:14 UTC", description: "Bump Vector to 0.33.0", pr_number: 18250, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "a4d73ca2cca51f197cf48be7128ba875a8fb5be7", date: "2023-08-16 08:34:33 UTC", description: "Add CLI arg and env variable to control openssl probing", pr_number: 18229, scopes: ["core"], type: "enhancement", breaking_change: false, author: "Hugo Hromic", files_count: 3, insertions_count: 38, deletions_count: 3},
		{sha: "8918c66af5bca25603be2ef491c07afff350a4f5", date: "2023-08-16 04:17:48 UTC", description: "Fix events being empty when log namespacing is enabled", pr_number: 18244, scopes: ["kubernetes_logs source"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 8, insertions_count: 490, deletions_count: 113},
		{sha: "adbc06fd45f9ab0bd34a1981ee836e7890b70b06", date: "2023-08-16 10:31:24 UTC", description: "remove obsolete codec file and import", pr_number: 18257, scopes: ["exec source"], type: "chore", breaking_change: false, author: "Hugo Hromic", files_count: 2, insertions_count: 0, deletions_count: 104},
		{sha: "d155a237281c02960d44d03b4ca3737290de2504", date: "2023-08-16 02:54:27 UTC", description: "Regenerate k8s manifests for v0.24.0 of the chart", pr_number: 18251, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "23a1a2df6bdf170054757ad048edaf18821989ae", date: "2023-08-16 03:06:04 UTC", description: "Allow region to be optional", pr_number: 18258, scopes: ["aws_s3 source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 21, deletions_count: 13},
		{sha: "3a6af99c3975859f16c0aac5c30de77a7932fad6", date: "2023-08-16 11:40:15 UTC", description: "Bump flate2 from 1.0.26 to 1.0.27", pr_number: 18254, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a77b6521956448481e7aa8023f0e02506da7176c", date: "2023-08-17 00:51:16 UTC", description: "Bump anyhow from 1.0.72 to 1.0.74", pr_number: 18255, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "d05bc3e636082d7adb27080e9dcff855c3928336", date: "2023-08-17 05:22:40 UTC", description: "Bump serde_json from 1.0.104 to 1.0.105", pr_number: 18267, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 7, deletions_count: 7},
		{sha: "28de9594def66b3a6222c550683f91ef93fe6739", date: "2023-08-17 05:23:33 UTC", description: "Bump mongodb from 2.6.0 to 2.6.1", pr_number: 18268, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "2a8d9749eeb6d94dea583bb6f3cd03b8a340ec35", date: "2023-08-17 05:33:55 UTC", description: "Bump thiserror from 1.0.44 to 1.0.46", pr_number: 18253, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "b755a46901af404102c77df286187ffa451c6f49", date: "2023-08-17 01:39:07 UTC", description: "make pin-project a workspace dependency", pr_number: 18176, scopes: [], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 5, deletions_count: 4},
		{sha: "c49593964799ba05d587bd4d3c6d02ac34df190d", date: "2023-08-17 07:46:38 UTC", description: "Refactor to use StreamSink", pr_number: 18209, scopes: ["appsignal sink"], type: "chore", breaking_change: false, author: "Tom de Bruijn", files_count: 9, insertions_count: 753, deletions_count: 287},
		{sha: "4ec6c11a72a1312c5a79135aa649c0e26cda5da9", date: "2023-08-17 02:25:44 UTC", description: "change dedupe config paths to `ConfigTargetPath`", pr_number: 18241, scopes: ["config"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 16, insertions_count: 182, deletions_count: 54},
		{sha: "69b4c1c48a6f07a8dccf295a49962ef28050683f", date: "2023-08-17 06:02:16 UTC", description: "Use metadata when log namespacing is enabled", pr_number: 18259, scopes: ["sample transform"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 23, deletions_count: 5},
		{sha: "7a1c49c3bc65743fc2c0e688233357af5b3ad4cd", date: "2023-08-17 10:26:48 UTC", description: "Bump no-proxy from 0.3.3 to 0.3.4", pr_number: 18277, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "294c1ddfb3f0398105fb02b37c9b9a38e50a6a6c", date: "2023-08-18 03:12:36 UTC", description: "Refactor to use StreamSink components", pr_number: 18243, scopes: ["nats sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 12, insertions_count: 1098, deletions_count: 756},
		{sha: "40f525cae3eb7c6867afeeed4b2bd82cf85f5a65", date: "2023-08-17 23:15:05 UTC", description: "Use the decoder to calculate type defs", pr_number: 18274, scopes: ["aws_s3 source"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 2, insertions_count: 57, deletions_count: 6},
		{sha: "d10e37379a11b430f527895bcaa207533a71817b", date: "2023-08-18 03:25:00 UTC", description: "Bump syn from 2.0.28 to 2.0.29", pr_number: 18282, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 31, deletions_count: 31},
		{sha: "fb5f099f1c2c403f6888c9f03c35121db50e3d0f", date: "2023-08-18 03:26:05 UTC", description: "Bump anyhow from 1.0.74 to 1.0.75", pr_number: 18284, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "1fbbdcb235519bbc407b6291a035d5e6e87d0955", date: "2023-08-18 03:26:54 UTC", description: "Bump dyn-clone from 1.0.12 to 1.0.13", pr_number: 18285, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "378926d863dfb82ff71380ec11db40d14f188e85", date: "2023-08-18 03:30:17 UTC", description: "Bump thiserror from 1.0.46 to 1.0.47", pr_number: 18286, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "704cbfee475f6bed6b7e22fdf463e2fbbbad3a76", date: "2023-08-18 03:30:45 UTC", description: "Bump typetag from 0.2.12 to 0.2.13", pr_number: 18287, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "47051a5b80bf970fdfe8c3348d88d4fb3c1542d7", date: "2023-08-18 05:09:19 UTC", description: "Bump tokio from 1.30.0 to 1.32.0", pr_number: 18279, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 12, deletions_count: 12},
		{sha: "aed8224d4d9aaaa2d94ccf9e2919593a32a0010e", date: "2023-08-17 23:45:48 UTC", description: "Fix PATH modification to allow for spaces", pr_number: 18294, scopes: ["distribution"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "f2a68871ddff4bcfe79af3d7af7351f8b75fba38", date: "2023-08-18 07:03:08 UTC", description: "Bump quote from 1.0.32 to 1.0.33", pr_number: 18283, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 71, deletions_count: 71},
		{sha: "5ce5ff19365a42afbb857acbcd48636bb2d99194", date: "2023-08-18 03:17:02 UTC", description: "disable vrl 'string_path' feature", pr_number: 18188, scopes: ["config"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 48, insertions_count: 307, deletions_count: 291},
		{sha: "03fe2fea176066729b9ef27c27acbfa46fcabd96", date: "2023-08-18 07:27:16 UTC", description: "Bump mlua from 0.8.9 to 0.8.10", pr_number: 18292, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "47f25d31b0124c0c75d4679efff16ebf4f02dc0f", date: "2023-08-18 07:27:35 UTC", description: "Bump clap from 4.3.21 to 4.3.22", pr_number: 18293, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 20, deletions_count: 20},
		{sha: "1e7e99c1840d57fbab9fa00b6d894f874ae0bd31", date: "2023-08-18 12:58:42 UTC", description: "Refactor to use StreamSink", pr_number: 18220, scopes: ["redis sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 8, insertions_count: 943, deletions_count: 680},
		{sha: "ac90069d448b094f17fc8fc4a212bd73f7b6ca36", date: "2023-08-18 13:06:15 UTC", description: "split tests into own module file", pr_number: 18301, scopes: ["exec source"], type: "chore", breaking_change: false, author: "Hugo Hromic", files_count: 2, insertions_count: 463, deletions_count: 464},
		{sha: "833ac19092bd7c7b88514d98c2c7176e5ff002b1", date: "2023-08-19 00:09:20 UTC", description: "Bump ordered-float from 3.7.0 to 3.8.0", pr_number: 18302, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 14, deletions_count: 14},
		{sha: "7b2bddc26d8ffa51c4f968a50a1b2983b98717f2", date: "2023-08-19 07:02:21 UTC", description: "add AWS Simple Notification Service `aws_sns` sink", pr_number: 18141, scopes: ["new sink"], type: "feat", breaking_change: false, author: "Tobias Wochinger", files_count: 26, insertions_count: 1464, deletions_count: 341},
		{sha: "2a45722cc777c7b971754148928bc376a06d82b1", date: "2023-08-19 07:47:48 UTC", description: "tidy `encode_input` function", pr_number: 18300, scopes: [], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 17, deletions_count: 23},
		{sha: "afdc66e08fc46a990e7f0c65a8a1a540de8ef52b", date: "2023-08-19 14:29:18 UTC", description: "Configurable http response code", pr_number: 18208, scopes: ["http_server source"], type: "enhancement", breaking_change: false, author: "Kunal Mohan", files_count: 12, insertions_count: 146, deletions_count: 5},
		{sha: "69621bd79ad38ed6059443c739886eb5d611b5af", date: "2023-08-19 10:42:24 UTC", description: "add missing Cargo feature", pr_number: 18308, scopes: ["log_to_metric transform"], type: "chore", breaking_change: false, author: "Hugo Hromic", files_count: 3, insertions_count: 8, deletions_count: 0},
		{sha: "771f47685241bf849315c9a0c1bd3f0cf74ddcd4", date: "2023-08-19 09:46:45 UTC", description: "Bump num_enum from 0.6.1 to 0.7.0", pr_number: 18238, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "ca30b6d9558e0457a6b07133441f9c79959e63ac", date: "2023-08-19 10:11:36 UTC", description: "Bump ordered-float from 3.8.0 to 3.9.0", pr_number: 18307, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 14, deletions_count: 14},
		{sha: "31ec4b387b9ef66c803606ac257f3e8580ddc5e0", date: "2023-08-19 12:36:05 UTC", description: "Bump http-serde from 1.1.2 to 1.1.3", pr_number: 18310, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "d3a623540ec08e63dd3b075614159db663ac0dc2", date: "2023-08-19 07:42:58 UTC", description: "refactor to new style", pr_number: 18200, scopes: ["http sink"], type: "chore", breaking_change: false, author: "neuronull", files_count: 17, insertions_count: 1465, deletions_count: 1044},
		{sha: "e6ec664538715026a3ab78b83f2e59ebd9b5a409", date: "2023-08-22 00:46:10 UTC", description: "Revert missing Cargo feature `transforms-log_to_metric`", pr_number: 18327, scopes: ["log_to_metric transform"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 0, deletions_count: 8},
		{sha: "ea2d57667ebe5bbea2124403b94febc702fcf759", date: "2023-08-22 04:41:31 UTC", description: "Bump serde from 1.0.183 to 1.0.185", pr_number: 18319, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 11, deletions_count: 12},
		{sha: "b3a1d5307453e5c4ba70c3b2bde45f879f5b9973", date: "2023-08-22 08:41:44 UTC", description: "Bump similar-asserts from 1.4.2 to 1.5.0", pr_number: 18318, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 6, deletions_count: 6},
		{sha: "e9637665b04a0dfe2a785673607f7592ee8fecac", date: "2023-08-22 08:42:21 UTC", description: "Bump notify from 6.0.1 to 6.1.0", pr_number: 18317, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 11, deletions_count: 10},
		{sha: "54d48d79b5ed569a8e213c697a79dd0d91177d2e", date: "2023-08-22 09:06:01 UTC", description: "Bump serde_with from 3.2.0 to 3.3.0", pr_number: 18315, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 12, deletions_count: 12},
		{sha: "a7800f74b0cdd75415b6852e37e03dbd4a4f4e27", date: "2023-08-22 09:25:26 UTC", description: "Bump tokio-postgres from 0.7.7 to 0.7.9", pr_number: 18316, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 22, deletions_count: 9},
		{sha: "d41f9f6082d64f1ea9585ae4642cd9646d8621b4", date: "2023-08-22 09:56:38 UTC", description: "Bump h2 from 0.3.20 to 0.3.21", pr_number: 18330, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "6397edbcca5a0672985b93c8720dbe9c04caec8f", date: "2023-08-22 04:04:27 UTC", description: "Fix issue with `cargo` refetching refs on every run", pr_number: 18331, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "71343bd91ee6e851b430c69ac27753ba0e41104c", date: "2023-08-22 11:27:56 UTC", description: "Add option to enable/disable unmatched output", pr_number: 18309, scopes: ["route transform"], type: "feat", breaking_change: false, author: "Hugo Hromic", files_count: 2, insertions_count: 99, deletions_count: 23},
		{sha: "61c0ae8a54ecd17a4457f6916987effa3d2f903b", date: "2023-08-22 13:46:34 UTC", description: "Normalize metrics ", pr_number: 18217, scopes: ["appsignal sink"], type: "feat", breaking_change: false, author: "Noemi", files_count: 7, insertions_count: 476, deletions_count: 552},
		{sha: "85dd43921f9948148d088cb94d3e49127cd613c1", date: "2023-08-23 00:55:55 UTC", description: "make YAML appear first in the example configurations", pr_number: 18325, scopes: ["docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 9, insertions_count: 24, deletions_count: 24},
		{sha: "a1d05e42b0ff69bbb04cedb359b2614b09b1a400", date: "2023-08-22 22:16:36 UTC", description: "Bump k8s manifests to v0.24.1 of the chart", pr_number: 18334, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "d0440848e70d6f8eaedfaa20696c74b60d631519", date: "2023-08-23 12:40:14 UTC", description: "Add Cargo feature for the transform", pr_number: 18337, scopes: ["log_to_metric transform"], type: "chore", breaking_change: false, author: "Hugo Hromic", files_count: 4, insertions_count: 9, deletions_count: 0},
		{sha: "05765d8a4e773974fbfe8cda2d35130be521fc7e", date: "2023-08-23 04:40:35 UTC", description: "Bump clap from 4.3.22 to 4.3.23", pr_number: 18311, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 20, deletions_count: 20},
		{sha: "ae5de9cd2d112879a56d3e81600b0c932588aa63", date: "2023-08-23 11:43:10 UTC", description: "Bump notify from 6.1.0 to 6.1.1", pr_number: 18332, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "754bee00333ef80dc236fd2f11bfe3cf42335da8", date: "2023-08-23 11:43:22 UTC", description: "Bump dashmap from 5.5.0 to 5.5.1", pr_number: 18338, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "9b4625ce54c8c31a2b627732b3e13648a28fc0b9", date: "2023-08-23 12:21:39 UTC", description: "Bump reqwest from 0.11.18 to 0.11.19", pr_number: 18329, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 28, deletions_count: 11},
		{sha: "2f458f61584b0aa046cabb7c1b4c82b87496cc98", date: "2023-08-23 08:41:43 UTC", description: "add more comparison examples", pr_number: 18333, scopes: ["docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 33, deletions_count: 3},
		{sha: "e15aec7a3896a8b4091832fd1e42b7279483db52", date: "2023-08-24 00:07:19 UTC", description: "add support for YAML and JSON to the generate command", pr_number: 18345, scopes: ["config", "cli"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 258, deletions_count: 132},
		{sha: "1c303e83949e1f2a9feb30176faaf070fb9b55fc", date: "2023-08-23 22:10:02 UTC", description: "Allow any data type depending on configured codec", pr_number: 18295, scopes: ["websocket sink"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 11, deletions_count: 4},
		{sha: "ab41edc475783a4285b9cc7b2b95384471c71fd3", date: "2023-08-24 02:10:55 UTC", description: "Bump ordered-float from 3.9.0 to 3.9.1", pr_number: 18350, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 14, deletions_count: 14},
		{sha: "e104dc461130b47faeb91d27e486097202981361", date: "2023-08-24 00:45:48 UTC", description: "refactor to new style", pr_number: 18280, scopes: ["honeycomb sink"], type: "chore", breaking_change: false, author: "neuronull", files_count: 10, insertions_count: 457, deletions_count: 260},
		{sha: "0d8ab26c33e7dfca8cff4e84ed4559f1b4553ca0", date: "2023-08-24 01:39:27 UTC", description: "refactor to new style", pr_number: 18335, scopes: ["gcp_stackdriver_logs sink"], type: "chore", breaking_change: false, author: "neuronull", files_count: 10, insertions_count: 928, deletions_count: 688},
		{sha: "8086e19dcce467d29c07ef3e56ebe79bca75c57a", date: "2023-08-24 05:14:48 UTC", description: "fix [dev-dependencies] for some libs", pr_number: 18328, scopes: ["deps"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 5, deletions_count: 1},
		{sha: "2493288204781314d7b9a08d439bc50b4a0ed5b4", date: "2023-08-24 06:18:48 UTC", description: "Bump encoding_rs from 0.8.32 to 0.8.33", pr_number: 18360, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "5f4a6d8354c62a474ccdfa8954a60f0b9afcc2b7", date: "2023-08-24 10:19:29 UTC", description: "Bump clap from 4.3.23 to 4.3.24", pr_number: 18362, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 20, deletions_count: 20},
		{sha: "2aaea89e8a63ae23eddfa9ac72dbcf52cc58c4c0", date: "2023-08-25 01:06:50 UTC", description: "Bump reqwest from 0.11.19 to 0.11.20", pr_number: 18366, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "15a63b404262ef297929b3693d2387bfdd445f90", date: "2023-08-25 03:06:54 UTC", description: "log namespace should be used when splitting events from arrays", pr_number: 18372, scopes: ["remap transform"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 2, insertions_count: 60, deletions_count: 3},
		{sha: "836a31e34d9ed72c7ad8410d89d8610d72d8dd98", date: "2023-08-25 03:11:33 UTC", description: "Bump bytesize from 1.2.0 to 1.3.0", pr_number: 18367, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "83ec3cf555ea801b7d1b9a754d3b7695970d6270", date: "2023-08-25 07:17:55 UTC", description: "Bump serde from 1.0.185 to 1.0.186", pr_number: 18370, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 11, deletions_count: 11},
		{sha: "76ffdabb89cb15df857959f930c9bcb5255e2e33", date: "2023-08-25 07:18:45 UTC", description: "Bump clap_complete from 4.3.2 to 4.4.0", pr_number: 18374, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "f15144bb16a5d7a7389b20e2625c162101907f02", date: "2023-08-25 05:50:07 UTC", description: "Fix deserializing non-object values with the `Vector` namespace", pr_number: 18379, scopes: ["json codec"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 5, insertions_count: 95, deletions_count: 48},
		{sha: "4c901ed9e241ac6b7076a292efad3958c0d9ecde", date: "2023-08-26 05:52:58 UTC", description: "Begin publishing armv7hl rpm packages", pr_number: 18387, scopes: ["releasing"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 44, deletions_count: 1},
		{sha: "671695929ccc4a17cfcd26fba757a8692eb4fbe3", date: "2023-08-26 10:25:13 UTC", description: "Bump tokio-test from 0.4.2 to 0.4.3", pr_number: 18357, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "2a4235ca2c32c0ca6314fb7262d69bdfceba0b15", date: "2023-08-26 12:58:33 UTC", description: "fix tests for the generate command", pr_number: 18383, scopes: ["tests"], type: "fix", breaking_change: false, author: "Hugo Hromic", files_count: 1, insertions_count: 10, deletions_count: 0},
		{sha: "aca3a296174d38cbe6c4da61e30d8d3033a30060", date: "2023-08-26 13:03:56 UTC", description: "decouple syslog source and codec features", pr_number: 18381, scopes: ["dev"], type: "feat", breaking_change: false, author: "Hugo Hromic", files_count: 3, insertions_count: 7, deletions_count: 4},
		{sha: "f33aff1a81b64ec444d204cd5d9709d427139767", date: "2023-08-26 13:01:29 UTC", description: "Bump aws-actions/configure-aws-credentials from 2.2.0 to 3.0.1", pr_number: 18386, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "725b9bd7feba15f7ab7c22d75f61934c0ef80c67", date: "2023-08-26 13:43:17 UTC", description: "Bump tokio-postgres from 0.7.9 to 0.7.10", pr_number: 18391, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "016890e566008960419c28e72f371cd217cc7885", date: "2023-08-26 13:46:54 UTC", description: "Bump rdkafka from 0.33.2 to 0.34.0", pr_number: 18393, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "dc665666bcd4d3487ca3684fd2fe41d4415cea52", date: "2023-08-26 07:33:22 UTC", description: "Expose `oldest_first`", pr_number: 18376, scopes: ["kubernetes_logs source"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 37, deletions_count: 10},
		{sha: "f8d073eb3fbc3569b3d47ddc9a755f17ced5114d", date: "2023-08-29 02:03:55 UTC", description: "Update fork of rust-openssl", pr_number: 18404, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "ff6e8884941cbbced58b781936e9af1cf69dd7c5", date: "2023-08-29 03:13:00 UTC", description: "add comment trigger filter for regression workflow concurrency group", pr_number: 18408, scopes: ["ci"], type: "fix", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4a2805d4984a46755b79ef347b723f9e301cb96c", date: "2023-08-29 09:18:01 UTC", description: "Bump serde from 1.0.186 to 1.0.188", pr_number: 18395, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 11, deletions_count: 11},
		{sha: "5dca377b8a45acaaeafdf0b9bdab04f5877a3880", date: "2023-08-29 09:19:07 UTC", description: "Bump base64 from 0.21.2 to 0.21.3", pr_number: 18398, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 21, deletions_count: 21},
		{sha: "21f7679e9cf152d6d0f6cd6c8abeae72cbe3b365", date: "2023-08-29 09:19:17 UTC", description: "Bump regex from 1.9.3 to 1.9.4", pr_number: 18399, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 15, deletions_count: 15},
		{sha: "dd8a0ef20e27eb315ef60733b88c8a11fadaa6ad", date: "2023-08-29 09:21:01 UTC", description: "Bump docker/setup-buildx-action from 2.9.1 to 2.10.0", pr_number: 18406, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "02c1b4c96aa632f1e7d8ff91acad09a581a38e9d", date: "2023-08-29 02:31:50 UTC", description: "Use FIPS endpoints when configured to do so", pr_number: 18390, scopes: ["aws provider"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 9, deletions_count: 0},
		{sha: "a252eda9ee70e0bc717196e4ace98d0ada87f270", date: "2023-08-29 09:45:04 UTC", description: "Bump trust-dns-proto from 0.22.0 to 0.23.0", pr_number: 18349, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 51, deletions_count: 57},
		{sha: "4359c9a9fae8a09096739152efe2e3936843f57e", date: "2023-08-29 05:54:56 UTC", description: "use `rstest` in `generate` command tests (vs wrong usage of `proptest`)", pr_number: 18365, scopes: ["dev"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 5, insertions_count: 53, deletions_count: 23},
		{sha: "fd21b19a5b57d173723ecf84e8b9216dfad359cd", date: "2023-08-29 03:35:32 UTC", description: "Bump MSRV to 1.70.0", pr_number: 18394, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 5, deletions_count: 5},
		{sha: "0cda90678c22351d6ae08e9733751c3432d3e2d2", date: "2023-08-29 11:52:30 UTC", description: "Bump rmpv from 1.0.0 to 1.0.1", pr_number: 18049, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7bc19427eab2b14992ee64fb1032baf30414d5cc", date: "2023-08-29 12:25:20 UTC", description: "Bump memchr from 2.5.0 to 2.6.0", pr_number: 18410, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "d3b9eda8ba4e46eaa87b17983577a00eb7642dd0", date: "2023-08-29 12:28:44 UTC", description: "Bump clap from 4.3.24 to 4.4.1", pr_number: 18411, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 26, deletions_count: 27},
		{sha: "82be883cfc4c0cbfb39e071d285a45a19a6693d9", date: "2023-08-29 12:30:02 UTC", description: "Bump ratatui from 0.22.0 to 0.23.0", pr_number: 18412, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 28, deletions_count: 10},
		{sha: "e39d9b3547124206135853688f653862e8c47a13", date: "2023-08-29 10:27:55 UTC", description: "Unlink python before `brew install`", pr_number: 18402, scopes: ["ci"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "15792f62477a57fdb79397c3d209128d8493bf55", date: "2023-08-30 04:00:58 UTC", description: "Bump openssl from 0.10.56 to 0.10.57", pr_number: 18400, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 5, deletions_count: 5},
		{sha: "8639655ca0a5247eeb6bb44668fe3b3480051210", date: "2023-08-30 11:01:17 UTC", description: "Bump async-compression from 0.4.1 to 0.4.2", pr_number: 18417, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "3f4603cc3d6e77b5163a06671f7d1d72e1eea1de", date: "2023-08-30 11:01:26 UTC", description: "Bump memchr from 2.6.0 to 2.6.1", pr_number: 18421, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "fa6f2cdc880e4bdd0dc2e99ba0fb8ef2679182ba", date: "2023-08-30 11:01:39 UTC", description: "Bump bstr from 1.6.0 to 1.6.1", pr_number: 18422, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "314a37b0fa852e96348b9f490bf83830f1dc9f1b", date: "2023-08-30 12:44:02 UTC", description: "Bump url from 2.4.0 to 2.4.1", pr_number: 18414, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "7a6365e2dec20b374bd2e9e31760a46014dc8d21", date: "2023-08-30 22:29:39 UTC", description: "Use `Ipv#Addr` constants", pr_number: 17627, scopes: [], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 10, insertions_count: 16, deletions_count: 28},
		{sha: "c07c99d95b72622a536701c104966d143a184848", date: "2023-08-31 03:03:47 UTC", description: "Split compilation features", pr_number: 18431, scopes: ["prometheus_remote_write source", "prometheus_scrape source"], type: "chore", breaking_change: false, author: "Will Wang", files_count: 3, insertions_count: 12, deletions_count: 2},
		{sha: "f2cd59ad048c9b867e240cc29e92de1cbffc2d5b", date: "2023-08-31 02:27:28 UTC", description: "Drop use of `once_cell::{sync,unsync}::OnceCell`", pr_number: 17621, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 14, insertions_count: 36, deletions_count: 34},
		{sha: "ce7da4e3249a9bf450d1b34ddc7a8c40aa9c1ea1", date: "2023-08-31 05:59:10 UTC", description: "Create built.rs with versions and expose versions to the UI", pr_number: 18424, scopes: ["playground"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 8, insertions_count: 203, deletions_count: 104},
		{sha: "a1127044f2c53439282be64a66f1ddf692f94ee7", date: "2023-09-01 09:05:14 UTC", description: "default to nullbyte delimiter for GELF #18008", pr_number: 18419, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Martin Emrich", files_count: 1, insertions_count: 5, deletions_count: 1},
		{sha: "9c1abd665fec92e03f8a19be1c13e89b359d5f07", date: "2023-09-01 04:24:59 UTC", description: "Fix feature check", pr_number: 18440, scopes: ["prometheus_remote_write source", "prometheus_scrape source"], type: "fix", breaking_change: false, author: "Will Wang", files_count: 6, insertions_count: 23, deletions_count: 11},
		{sha: "89f0f088f20876c44e256254410c293100db195c", date: "2023-09-01 10:23:19 UTC", description: "Fixed NixOS page", pr_number: 18396, scopes: [], type: "docs", breaking_change: false, author: "Jonathan Davies", files_count: 1, insertions_count: 18, deletions_count: 3},
		{sha: "221e0a1379aa36f99ed85be124644801ddd8b862", date: "2023-09-01 07:16:31 UTC", description: "Bump prost-reflect from 0.11.4 to 0.11.5", pr_number: 18426, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "adf134b77fdd9f998bca8cb4a6af808baa2cf321", date: "2023-09-01 07:16:49 UTC", description: "Bump dashmap from 5.5.1 to 5.5.3", pr_number: 18427, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "5e3eaa984fbafd8956bd77db072de429a2d5c4b2", date: "2023-09-01 11:19:54 UTC", description: "Bump memchr from 2.6.1 to 2.6.2", pr_number: 18434, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "699851891d23eab8364f740bd06657a2e90c85cb", date: "2023-09-01 11:20:07 UTC", description: "Bump clap from 4.4.1 to 4.4.2", pr_number: 18447, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 22, deletions_count: 23},
		{sha: "749594cf357bd0b1932de8e83b287b2cfe51c54c", date: "2023-09-01 11:20:21 UTC", description: "Bump headers from 0.3.8 to 0.3.9", pr_number: 18448, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 5, deletions_count: 6},
		{sha: "e652ea4023dd4c07d59489f5343c1cdfc8cbb083", date: "2023-09-01 19:56:27 UTC", description: "separate aws support in es & prometheus sink", pr_number: 18288, scopes: ["es sink"], type: "enhancement", breaking_change: false, author: "Suika", files_count: 12, insertions_count: 169, deletions_count: 151},
		{sha: "7849d804cf2bf73c2f3e77e03c8b7af73aaa1a06", date: "2023-09-02 00:32:37 UTC", description: "Bump to Rust 1.72.0", pr_number: 18389, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 76, insertions_count: 289, deletions_count: 231},
		{sha: "bc6c421da92e23d1d2c76853c5c36c9387a7979a", date: "2023-09-02 08:22:32 UTC", description: "add `ENVIRONMENT_AUTOPULL` override to Makefile", pr_number: 18446, scopes: ["dev"], type: "feat", breaking_change: false, author: "Hugo Hromic", files_count: 2, insertions_count: 5, deletions_count: 1},
		{sha: "4e4ece637681cafb949d09b0f6680069a4daa281", date: "2023-09-02 08:42:46 UTC", description: "Bump tibdex/github-app-token from 1.8.0 to 1.8.2", pr_number: 18454, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "40ef7c4d1f6505353aa7a17b289023243118f7e9", date: "2023-09-02 05:00:35 UTC", description: "Remove `conf-files` directive for `cargo-deb`", pr_number: 18455, scopes: ["debian platform"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "164796404fb01e3f7bd9207e00a42e3a8251142a", date: "2023-09-02 05:17:21 UTC", description: "Emphasize the \"may\" bit of the backpressure docs", pr_number: 18457, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "8b6a307a3f6960132f51a7fd793475e0fb7c7751", date: "2023-09-02 10:38:45 UTC", description: "Bump bstr from 1.6.1 to 1.6.2", pr_number: 18433, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "9f9502662352093fd14ddc99758f0164bec36352", date: "2023-09-02 10:38:50 UTC", description: "Bump inventory from 0.3.11 to 0.3.12", pr_number: 18429, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c1ed01755788f15a96c2c83b2213b2c492155c85", date: "2023-09-02 06:45:24 UTC", description: "Re-add docker-compose installation", pr_number: 18415, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 6, deletions_count: 0},
		{sha: "d9c75bd3c2675c192d907a7d0436320e915b7916", date: "2023-09-02 11:18:04 UTC", description: "Bump crossterm from 0.26.1 to 0.27.0", pr_number: 18168, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 20},
		{sha: "d3d8a4c703bbe4aea2ad1945fc93f446457dc5ca", date: "2023-09-02 11:28:19 UTC", description: "Bump tower-http from 0.4.3 to 0.4.4", pr_number: 18461, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "2e2692ed94192fb75bd447e132acadcff9927857", date: "2023-09-02 11:28:42 UTC", description: "Bump redis from 0.23.2 to 0.23.3", pr_number: 18464, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7ad631348760abe42bba42950c4a7571ba187f42", date: "2023-09-05 05:40:51 UTC", description: "Bump thiserror from 1.0.47 to 1.0.48", pr_number: 18473, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "1aeb42e98c2d36d2e167327ce1a9f60bbc90432b", date: "2023-09-05 05:42:38 UTC", description: "Bump regex from 1.9.4 to 1.9.5", pr_number: 18472, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 10, deletions_count: 10},
		{sha: "f2b46d6cc6eaaf099354d5235fd749e60e238d54", date: "2023-09-05 07:05:39 UTC", description: "Bump syn from 2.0.29 to 2.0.31", pr_number: 18471, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 33, deletions_count: 33},
		{sha: "8b017b6231cb82752a8b112441648b57eca57d1f", date: "2023-09-06 00:54:54 UTC", description: "Bump async-recursion from 1.0.4 to 1.0.5", pr_number: 18466, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "325fbea85b08c569f203d5f32a0aeccc906d40ad", date: "2023-09-06 00:57:38 UTC", description: "Bump cached from 0.44.0 to 0.45.0", pr_number: 18478, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 6},
		{sha: "69a1ca011ab86237fc1351fb4f5dad4c7cdb3dce", date: "2023-09-05 23:58:19 UTC", description: "Bump memchr from 2.6.2 to 2.6.3", pr_number: 18470, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "59dfd6789c213396ddadff94460971056419fb9a", date: "2023-09-06 00:47:43 UTC", description: "Bump actions/checkout from 3 to 4", pr_number: 18476, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 19, insertions_count: 60, deletions_count: 60},
		{sha: "5cfb3e41c0735d8c5650e17b75508c0e861fe647", date: "2023-09-06 11:44:12 UTC", description: "Add notes about `ingress_upstreaminfo` log format for `parse_nginx_log()` function", pr_number: 18477, scopes: [], type: "docs", breaking_change: false, author: "Sergey Pyankov", files_count: 2, insertions_count: 4, deletions_count: 2},
		{sha: "f11eeb3cd8b79aa5cb942434505438d0c6e48a0d", date: "2023-09-06 05:40:23 UTC", description: "Add checksums for artifacts", pr_number: 18483, scopes: ["releasing"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 4, insertions_count: 96, deletions_count: 1},
		{sha: "e19243fb05f2f65705892258aae1a1becb4040fe", date: "2023-09-06 13:10:25 UTC", description: "feature gate aws-core features", pr_number: 18482, scopes: [], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 4, deletions_count: 1},
		{sha: "f8981e182ca1a4cb3c2366868294d9143c983f41", date: "2023-09-06 22:27:25 UTC", description: "Bump cached from 0.45.0 to 0.45.1", pr_number: 18485, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 6},
		{sha: "7ec5b9703c4ea08ecb486ba370482b031892a846", date: "2023-09-07 03:11:52 UTC", description: "Bump lru from 0.11.0 to 0.11.1", pr_number: 18484, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "ed1dedf5855ca5eafbc27d5abc9789f00fafa342", date: "2023-09-06 21:00:54 UTC", description: "Change the default configuration tab and add comments to…", pr_number: 18420, scopes: ["docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 156, deletions_count: 128},
		{sha: "8981cba07c768b0155d90b92d799eed464ee0b7a", date: "2023-09-07 01:19:31 UTC", description: "Improve checksum script", pr_number: 18487, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 2, deletions_count: 3},
		{sha: "0bbb152a067e070388c231d5dc4ec2edc8cd35d0", date: "2023-09-06 22:19:55 UTC", description: "Bump azure_core from 0.13.0 to 0.14.0", pr_number: 18361, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 19, deletions_count: 19},
		{sha: "3fd648603d2f46bbf6a01a3e7e4c4af7c70304ca", date: "2023-09-07 07:40:30 UTC", description: "revert bump actions/checkout from 3 to 4", pr_number: 18490, scopes: ["ci"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 19, insertions_count: 60, deletions_count: 60},
		{sha: "4fba377b66a564a6753579f27fcbd8f6f32644b6", date: "2023-09-07 07:25:56 UTC", description: "Bump prost from 0.11.9 to 0.12.0", pr_number: 18460, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 187, deletions_count: 82},
		{sha: "c150b144a062d96027424e982b54f74b7e37072d", date: "2023-09-07 00:43:22 UTC", description: "Add highlight announcing YAML as the new default config …", pr_number: 18435, scopes: ["docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 34, deletions_count: 0},
		{sha: "8ec87eb43073b4242f489ea91cc56f684905e006", date: "2023-09-07 04:07:11 UTC", description: "Remove openssl-sys patch", pr_number: 18495, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 10, deletions_count: 13},
		{sha: "9859b9ed93d20d61df7f24aa25dbeabc7bda2d27", date: "2023-09-07 11:11:04 UTC", description: "Bump webpki from 0.22.0 to 0.22.1", pr_number: 18494, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 8},
		{sha: "712a2101ceb980805dfeddccfdef8ee1b25fab8f", date: "2023-09-07 07:58:03 UTC", description: "don't continue on errors in unit-mac test", pr_number: 18496, scopes: ["ci"], type: "fix", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 0, deletions_count: 3},
		{sha: "8d07e184afa239ff9e111bdd8c0f4c7620f4d959", date: "2023-09-08 05:43:03 UTC", description: "remove '---\\n' prefix from toYaml config example generator", pr_number: 18502, scopes: ["docs"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "f7c3c965ed45ce05d0a3c5d8f10d12068c9e7f0c", date: "2023-09-09 02:57:44 UTC", description: "Bump serde_derive_internals from 0.28.0 to 0.29.0", pr_number: 18499, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 6, deletions_count: 6},
		{sha: "567de50d66aca3b6302063a55988adfc5fb19540", date: "2023-09-09 00:26:36 UTC", description: "Add git sha to the VRL playground header", pr_number: 18500, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 54, deletions_count: 23},
		{sha: "9cb0ca3b4a8705e2959c62a081345531f74e8b14", date: "2023-09-09 00:30:24 UTC", description: "add convert config command", pr_number: 18378, scopes: ["config"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 8, insertions_count: 447, deletions_count: 2},
		{sha: "a544e6e8061244771ea89029e9489318773cf441", date: "2023-09-09 00:12:47 UTC", description: "don't increment component_errors_total for `HttpClient` warning", pr_number: 18505, scopes: ["observability"], type: "fix", breaking_change: false, author: "neuronull", files_count: 2, insertions_count: 2, deletions_count: 6},
		{sha: "737d2f1bb17506832b68bb69205db90b09768008", date: "2023-09-09 01:03:41 UTC", description: "Bump aws-actions/configure-aws-credentials from 3.0.1 to 3.0.2", pr_number: 18511, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "f19d166a33ac4887250f79c298124111af2b6422", date: "2023-09-09 01:04:01 UTC", description: "Bump docker/build-push-action from 4.1.1 to 4.2.1", pr_number: 18512, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8d5003a04a929cac1df5c68e099ab5edbc2233d4", date: "2023-09-09 09:09:21 UTC", description: "Bump myrotvorets/set-commit-status-action from 1.1.7 to 2.0.0", pr_number: 18510, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 27, deletions_count: 27},
		{sha: "bfdb5b046985ee9d6b4876b4ab4f1bb095c048b6", date: "2023-09-09 12:34:05 UTC", description: "Bump bytes from 1.4.0 to 1.5.0", pr_number: 18508, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 81, deletions_count: 81},
		{sha: "af444ea9dcb9ec5c3b32904b5a9f1267b1bb634d", date: "2023-09-09 12:34:20 UTC", description: "Bump clap_complete from 4.4.0 to 4.4.1", pr_number: 18509, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "688e2d90f3ecf2fc801aaa20785e43d6886876c1", date: "2023-09-09 13:53:06 UTC", description: "Bump toml from 0.7.6 to 0.7.7", pr_number: 18507, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 17, deletions_count: 17},
		{sha: "f5cd27afd0bd6aa93ac1b18f508f0ecedaa87489", date: "2023-09-09 15:54:54 UTC", description: "Bump cidr-utils from 0.5.10 to 0.5.11", pr_number: 18516, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "44a87dcf3face3c1cec9e20ab6de50be0dd3d131", date: "2023-09-11 22:19:44 UTC", description: "Bump base64 from 0.21.3 to 0.21.4", pr_number: 18522, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 23, deletions_count: 23},
		{sha: "f30537e2840cf51513d52dd5563f4233d98a48d8", date: "2023-09-11 22:20:14 UTC", description: "Bump syn from 2.0.31 to 2.0.32", pr_number: 18524, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 37, deletions_count: 37},
		{sha: "7b6ad621a07dc16b59e7922bc73ad12f4cb16bf5", date: "2023-09-12 01:53:14 UTC", description: "group azure and prost crates for dependabot", pr_number: 18525, scopes: ["ci"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 8, deletions_count: 0},
		{sha: "2687ed1f9f9a88bc4d39813907a4d239dc6d50a8", date: "2023-09-12 00:54:26 UTC", description: "Bump serde_json from 1.0.105 to 1.0.106", pr_number: 18523, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 7, deletions_count: 7},
		{sha: "a6305deb1638f440a928599223e4fe5cd7184bcd", date: "2023-09-11 19:07:20 UTC", description: "Bump chrono to 0.4.30", pr_number: 18527, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "0476bb5f55494e4ec70d298204ffe1fcb4524780", date: "2023-09-12 04:01:13 UTC", description: "Bump toml from 0.7.7 to 0.7.8", pr_number: 18520, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 13, deletions_count: 13},
		{sha: "7295f223576b8530a167bcef63ac1a1857db8ef1", date: "2023-09-12 04:02:19 UTC", description: "Bump tibdex/github-app-token from 1.8.2 to 2.0.0", pr_number: 18528, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "ae0aa11c409a5bb6b74c35adf22a06dcd1b42895", date: "2023-09-11 23:05:26 UTC", description: "skip encoding empty sketches", pr_number: 18530, scopes: ["metrics"], type: "fix", breaking_change: false, author: "Luke Steensen", files_count: 1, insertions_count: 54, deletions_count: 21},
		{sha: "3d7199ea42368a09d5ac909b6fd89f66758889a3", date: "2023-09-12 04:35:38 UTC", description: "Bump the azure group with 4 updates", pr_number: 18529, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 16, deletions_count: 16},
		{sha: "8eb52562f034930b83eb4416a667bea1381dd9c8", date: "2023-09-12 23:31:11 UTC", description: "Bump socket2 from 0.5.3 to 0.5.4", pr_number: 18531, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 9, deletions_count: 9},
		{sha: "3e3c7ada10e7406aed31ac36e9fcfe0aa9bd65a8", date: "2023-09-13 01:43:37 UTC", description: "Bump async-compression from 0.4.2 to 0.4.3", pr_number: 18539, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "953e305470b474ce2ba368ab587db8abbc4693fa", date: "2023-09-13 06:33:30 UTC", description: "Multiple fixes related to metrics", pr_number: 18151, scopes: ["new_relic sink"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 186, deletions_count: 71},
		{sha: "8aae235a3a9aa5ce07fe9131b8cb1bcc60c2e1bc", date: "2023-09-13 11:35:44 UTC", description: "Bump docker/metadata-action from 4.6.0 to 5.0.0", pr_number: 18543, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c30a4b2402eed5b0e9530b896d753572f447db01", date: "2023-09-13 09:36:00 UTC", description: "Bump docker/build-push-action from 4.2.1 to 5.0.0", pr_number: 18546, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "dcda1deb26c55acdc0015d535f6dabf0196be3b8", date: "2023-09-13 09:36:30 UTC", description: "Bump docker/setup-qemu-action from 2.2.0 to 3.0.0", pr_number: 18547, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "30188644651731a088617a945dbcc16aee604871", date: "2023-09-13 09:36:54 UTC", description: "Bump aws-actions/configure-aws-credentials from 3.0.2 to 4.0.0", pr_number: 18544, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 6, deletions_count: 6},
		{sha: "899e3c0586428bc38fe4060afc1f99db7a11e41d", date: "2023-09-13 09:37:12 UTC", description: "Bump docker/setup-buildx-action from 2.10.0 to 3.0.0", pr_number: 18545, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "9e8407e3ff5dc7d063b886b2eef43673c7bc7c39", date: "2023-09-14 00:10:56 UTC", description: "Bump clap from 4.4.2 to 4.4.3", pr_number: 18550, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 18, deletions_count: 18},
		{sha: "6f091e1353485164ec7fc8871f69d6d40f703578", date: "2023-09-14 22:35:48 UTC", description: "Bump proc-macro2 from 1.0.66 to 1.0.67", pr_number: 18561, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 72, deletions_count: 72},
		{sha: "3c3e25194623f966fc32dc637e5adc22dbb84168", date: "2023-09-14 20:39:33 UTC", description: "Bump serde_json from 1.0.106 to 1.0.107", pr_number: 18562, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 7, deletions_count: 7},
		{sha: "59e22fcb2ca115d168fded1ed52def2743f52281", date: "2023-09-14 20:39:44 UTC", description: "Bump libc from 0.2.147 to 0.2.148", pr_number: 18563, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "95297b2014277ca990a98283098aa6ed4d388fc4", date: "2023-09-14 20:39:58 UTC", description: "Bump serde-wasm-bindgen from 0.5.0 to 0.6.0", pr_number: 18565, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "ddb519508f5dc929ca19da3ac6706f6949167f46", date: "2023-09-14 23:25:38 UTC", description: "Add protobuf compatibility check (pt 1)", pr_number: 18552, scopes: ["ci"], type: "feat", breaking_change: false, author: "Doug Smith", files_count: 3, insertions_count: 18, deletions_count: 0},
		{sha: "730bb151618d0af6d1a39f75e2830c984cffb8db", date: "2023-09-15 00:57:39 UTC", description: "default tokio worker threads to `std::thread::available_parallelism()`", pr_number: 18541, scopes: ["core"], type: "feat", breaking_change: false, author: "Doug Smith", files_count: 2, insertions_count: 22, deletions_count: 13},
		{sha: "5a52b61f5bfa1339e80553da89adba1a1db0d527", date: "2023-09-15 01:20:18 UTC", description: "discuss disk throughput configurations in sizing guidance", pr_number: 18566, scopes: ["docs"], type: "chore", breaking_change: false, author: "Doug Smith", files_count: 1, insertions_count: 4, deletions_count: 2},
		{sha: "f750cf7480881976b65ff2c4b4c4e6d62b51be14", date: "2023-09-14 23:32:23 UTC", description: "Bump syn from 2.0.32 to 2.0.33", pr_number: 18559, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 37, deletions_count: 37},
		{sha: "0f90c39f8dd515a6fbff5484f780903c74ab02b7", date: "2023-09-15 03:00:48 UTC", description: "Bump enumflags2 from 0.7.7 to 0.7.8", pr_number: 18560, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "6c34cd531d6c5ceebe17d68ee5044c610e081c59", date: "2023-09-15 05:10:07 UTC", description: "Drop patch to use custom `chrono` repo", pr_number: 18567, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 2, deletions_count: 4},
		{sha: "d976c6e1d21854455f2e84b228ee3780c5eef776", date: "2023-09-15 03:12:27 UTC", description: "Bump docker/login-action from 2 to 3", pr_number: 18556, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "0382bc5bb89ccde211cd57955b56db091923a799", date: "2023-09-15 03:42:08 UTC", description: "Bump toml from 0.7.8 to 0.8.0", pr_number: 18549, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 39, deletions_count: 14},
		{sha: "1ac19dded48f0ea3f91290c36f5462551277bbba", date: "2023-09-14 23:06:15 UTC", description: "Use `endpoint` config setting consistent with the other datadog_ sinks.", pr_number: 18497, scopes: ["datadog_logs sink"], type: "chore", breaking_change: true, author: "neuronull", files_count: 7, insertions_count: 45, deletions_count: 27},
		{sha: "e23941c59ba7b7f14cd318ce5b946f31d307106a", date: "2023-09-15 12:35:43 UTC", description: "Use large pages for better OS compatibility", pr_number: 18481, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "0e600013a22bd1e91a956a9372ca15e5c28518fe", date: "2023-09-16 01:42:04 UTC", description: "Bump cargo_toml from 0.15.3 to 0.16.0", pr_number: 18571, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 30},
		{sha: "3afda3c5e8fca52dc689acb4d283988dd304c2fd", date: "2023-09-19 01:25:10 UTC", description: "don't show warning about max allocation groups if tracing not enabled", pr_number: 18589, scopes: ["core"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 2, insertions_count: 9, deletions_count: 4},
		{sha: "16edc225dcb3f58a79c2b703c35eec7dc650605e", date: "2023-09-19 04:46:09 UTC", description: "Bump the prost group with 3 updates", pr_number: 18579, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 32, deletions_count: 32},
		{sha: "b3f76b56d7d3d2a07ab33c6cdbb9cf4ac9f87fb0", date: "2023-09-19 09:40:23 UTC", description: "Bump indoc from 2.0.3 to 2.0.4", pr_number: 18582, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "9d295634538b9e97c0dbee1e1c72672c090ff83f", date: "2023-09-19 05:57:08 UTC", description: "Replace 'vector.toml' with 'vector.yaml'", pr_number: 18388, scopes: ["config", "docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 26, insertions_count: 143, deletions_count: 46},
		{sha: "996372f935af0cc6a03d987ff92a5f98c5c89813", date: "2023-09-19 22:42:21 UTC", description: "Bump bollard from 0.14.0 to 0.15.0", pr_number: 18581, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 24, deletions_count: 26},
		{sha: "e80d7b7eb29e90ac4a2156675464f191c3e428ec", date: "2023-09-20 02:43:11 UTC", description: "Bump syn from 2.0.33 to 2.0.37", pr_number: 18580, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 37, deletions_count: 37},
		{sha: "d179c57d0ddd09fbf6547941883c39c12a327b3a", date: "2023-09-20 00:36:24 UTC", description: "Editorial edits for updated component descriptions", pr_number: 18590, scopes: [], type: "docs", breaking_change: false, author: "May Lee", files_count: 51, insertions_count: 240, deletions_count: 240},
		{sha: "24701cfe6a643df61f0d6074f3ae92140de234e8", date: "2023-09-19 21:40:00 UTC", description: "Use yaml instead of toml file", pr_number: 18606, scopes: ["distribution"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "d5f4caa3d69cd2f9ff71350925e6c36d9bb9b611", date: "2023-09-20 02:32:21 UTC", description: "Bump dyn-clone from 1.0.13 to 1.0.14", pr_number: 18607, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "c47e65fc6521834c494d4b3afd15c37ada8b20c7", date: "2023-09-20 06:32:57 UTC", description: "Bump cargo_toml from 0.16.0 to 0.16.1", pr_number: 18605, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "addc46e28dcd1c73f653bb507e55f01b2c52c759", date: "2023-09-20 08:12:47 UTC", description: "Bump tibdex/github-app-token from 2.0.0 to 2.1.0", pr_number: 18608, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "75d03b370fbcf87872baebcb5d29ee2066ae88d2", date: "2023-09-20 12:04:37 UTC", description: "retry on unauthorized", pr_number: 18586, scopes: ["gcp service"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "d8f36e45b7f443a97dc6367d79bc16620971e05d", date: "2023-09-20 06:07:40 UTC", description: "Add DEPRECATIONS.md file to track deprecations", pr_number: 18613, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 13, deletions_count: 0},
		{sha: "587c2e7d8cbcc833379eb28e6e1d77902e12bede", date: "2023-09-20 21:22:06 UTC", description: "support Datadog metric origin metadata", pr_number: 18405, scopes: ["metrics"], type: "feat", breaking_change: false, author: "neuronull", files_count: 2055, insertions_count: 1802, deletions_count: 1128},
		{sha: "180647b895aafc4aeb53dad10f8dae48a656ea3e", date: "2023-09-20 23:58:55 UTC", description: "Add workspace to new rust docs deployment", pr_number: 18616, scopes: ["internal docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "1eb933d60adb8353e4bd62842aca38b158c333b9", date: "2023-09-21 04:29:20 UTC", description: "protobuf compatibility check (pt 2)", pr_number: 18553, scopes: ["ci"], type: "feat", breaking_change: false, author: "Doug Smith", files_count: 1, insertions_count: 27, deletions_count: 0},
		{sha: "a6b1bed163fa52e1a3fc2189ac24f0f355d752cf", date: "2023-09-21 06:06:59 UTC", description: "remove openssl legacy provider flag and update docs", pr_number: 18609, scopes: ["deps"], type: "enhancement", breaking_change: true, author: "Doug Smith", files_count: 11, insertions_count: 44, deletions_count: 44},
		{sha: "fd58af921b4b019ef11018a7200ceb50dc2ccac0", date: "2023-09-21 22:44:27 UTC", description: "Document intentional label on component_discarded_events_total", pr_number: 18622, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 6, deletions_count: 1},
		{sha: "918beac97732d9517c61fee5dfaf0b97e03fe6ae", date: "2023-09-21 22:52:33 UTC", description: "Add CODEOWNERS for documentation updates", pr_number: 18628, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "99de28b67799ed190e8cf00ee6f148ac1924ae37", date: "2023-09-21 23:05:16 UTC", description: "Regenerate manifests from new Helm chart version", pr_number: 18629, scopes: ["kubernetes"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "f2d60cbb9d2a4bdf74e559113c76a8d53a243f3d", date: "2023-09-22 02:57:06 UTC", description: "Convert a few more configs to YAML", pr_number: 18632, scopes: ["docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 93, deletions_count: 76},
		{sha: "76efb4b91792a8c59eb7c4b0376220364dd2d72f", date: "2023-09-22 01:30:58 UTC", description: "Update chat.vector.dev redirect", pr_number: 18635, scopes: ["website"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "feca4c8e89f7166df29e321f78985395e118ebf0", date: "2023-09-22 07:49:18 UTC", description: "Bump smallvec from 1.11.0 to 1.11.1", pr_number: 18620, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "fa526a817a470feec273398d1c0127ca86f5a4f5", date: "2023-09-22 07:50:33 UTC", description: "Bump cargo_toml from 0.16.1 to 0.16.2", pr_number: 18619, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "466ef846bdd7c58f1c341dc557fe01eac47bcc1d", date: "2023-09-22 05:17:46 UTC", description: "Bump Rust to 1.72.1", pr_number: 18638, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "32be0d3658919dc4d8fe38392a2f9a120144c40c", date: "2023-09-23 01:49:58 UTC", description: "group tonic crates in dependabot", pr_number: 18645, scopes: ["deps"], type: "chore", breaking_change: false, author: "Doug Smith", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "9523987bf9ba7df2d9731dfa6dc3be0afcc24611", date: "2023-09-23 06:02:14 UTC", description: "Bump tonic from 0.10.0 to 0.10.1", pr_number: 18639, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 21, deletions_count: 19},
		{sha: "e8d946fba2170eca1220d8caa527201767f4298f", date: "2023-09-23 05:00:38 UTC", description: "Bump md-5 from 0.10.5 to 0.10.6", pr_number: 18648, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 2},
		{sha: "6bab5be898dbc14f57bba6c46fb5cc0ac340e89e", date: "2023-09-23 05:02:06 UTC", description: "Bump indicatif from 0.17.6 to 0.17.7", pr_number: 18647, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7ecb06ad45c151636259a555c58303c0a519a11c", date: "2023-09-23 09:05:21 UTC", description: "Bump async-nats from 0.31.0 to 0.32.0", pr_number: 18640, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 21, deletions_count: 11},
		{sha: "ec9efb5d092086bc43f037f4a342df2cbb398127", date: "2023-09-23 09:05:57 UTC", description: "Bump tonic-build from 0.10.0 to 0.10.1", pr_number: 18641, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "2e7e7e7e0a5f0957bdaf4af9bc65bfe32a5f0ac8", date: "2023-09-23 05:20:06 UTC", description: "configurable app name", pr_number: 18554, scopes: ["enterprise"], type: "feat", breaking_change: false, author: "Doug Smith", files_count: 9, insertions_count: 93, deletions_count: 13},
		{sha: "3c662f3ff0042826c38f8452b03d80b1a9db73ba", date: "2023-09-23 05:53:24 UTC", description: "performance improvements and fix memory leak", pr_number: 18634, scopes: ["kafka sink"], type: "fix", breaking_change: false, author: "Doug Smith", files_count: 4, insertions_count: 149, deletions_count: 132},
		{sha: "75ebda0826992c5e4b63a882e4425fb5ed1e0dcc", date: "2023-09-26 08:41:10 UTC", description: "add networking overrides to website Makefile", pr_number: 18655, scopes: ["dev"], type: "feat", breaking_change: false, author: "Hugo Hromic", files_count: 1, insertions_count: 9, deletions_count: 2},
		{sha: "7cbb7585327b829e8d3f32b4b8047d0af22700c6", date: "2023-09-26 03:43:41 UTC", description: "Add amplify build spec files to appropriate directories", pr_number: 18668, scopes: ["websites"], type: "chore", breaking_change: false, author: "Devin Ford", files_count: 4, insertions_count: 78, deletions_count: 0},
		{sha: "89697d102793a4786c1ad61f64a90b32769c204a", date: "2023-09-26 08:54:47 UTC", description: "add environment networking overrides to Makefile", pr_number: 18654, scopes: ["dev"], type: "feat", breaking_change: false, author: "Hugo Hromic", files_count: 1, insertions_count: 7, deletions_count: 1},
		{sha: "6ced6ca22546d1033c66029dbe3b920868824134", date: "2023-09-26 06:16:41 UTC", description: "resolve memory leak by always setting a request builder concurrency limit", pr_number: 18637, scopes: ["sinks"], type: "fix", breaking_change: false, author: "Doug Smith", files_count: 29, insertions_count: 117, deletions_count: 243},
		{sha: "b35527d142465d2274af6a582da681b049a3b8d3", date: "2023-09-27 00:56:07 UTC", description: "fix concurrency default & docs", pr_number: 18651, scopes: ["config"], type: "fix", breaking_change: true, author: "Doug Smith", files_count: 45, insertions_count: 369, deletions_count: 141},
		{sha: "a4cd8b74872e118bfb4dc06d010b765195033070", date: "2023-09-27 05:49:19 UTC", description: "Bump cargo_toml from 0.16.2 to 0.16.3", pr_number: 18674, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "3edafefa359f2bcf8a61bd15267814d53166a914", date: "2023-09-27 05:49:47 UTC", description: "Bump clap_complete from 4.4.1 to 4.4.2", pr_number: 18673, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "70e8b5fe55dc9c4767f2fc992e83ac3bb932b740", date: "2023-09-27 04:15:41 UTC", description: "Update VRL to 0.7.0", pr_number: 18672, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 24, insertions_count: 340, deletions_count: 267},
	]
}
