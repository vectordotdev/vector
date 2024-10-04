package metadata

releases: "0.34.0": {
	date:     "2023-11-07"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.34.0!

		Be sure to check out the [upgrade guide](/highlights/2023-11-07-0-34-0-upgrade-guide) for
		breaking changes in this release.

		In addition to the usual enhancements and bug fixes, this release also includes

		- A new `protobuf` encoder for sinks.
		- A fix to pass event metadata and secrets through disk buffers to have this data available
		  in sinks. Note this change has potential security implications as things like Datadog API
		  keys or Splunk HEC tokens could end up persisted in disk buffers. See the [release
		  highlight](/highlights/2023-10-30-secrets-in-disk-buffers) for details about this
		  change and recommended practices to either secure the disk buffers or to avoid
		  storing secrets in events altogether.

		This release also marks the deprecation of the OS package repositories hosted at
		`repositories.timber.io`. Instead, packages have been moved to `apt.vector.dev` and
		`yum.vector.dev`. Please see the [release
		highlight](/highlights/2023-11-07-new-linux-repos) for details about this change and
		instructions on how to migrate. The repositories located at `repositories.timber.io` will
		be decommissioned on February 28th, 2024.
		"""

	known_issues: [
		"""
			The Datadog Metrics sink fails to send a large number of requests due to incorrectly
			sized batches [#19110](https://github.com/vectordotdev/vector/issues/19110). This is
			fixed in v0.34.1.
			""",
		"""
			The Loki sink incorrectly sets the `Content-Encoding` header on requests to
			`application/json` when the default `snappy` compression is used. This results in
			Loki rejecting the requests with an HTTP 400 response. This is fixed in v0.34.1.
			""",
		"""
			The `protobuf` encoder does not work in sinks
			[#19230](https://github.com/vectordotdev/vector/issues/19230). Fixed in `v0.34.2`.
			""",
	]

	changelog: [
		{
			type: "feat"
			scopes: ["codecs"]
			description: """
				Sinks can now encode data as [protobuf](https://protobuf.dev/) through support for
				a new `protobuf` encoder (configurable using `encoding.codec`).
				"""
			contributors: ["goakley"]
			pr_numbers: [18598]
		},
		{
			type: "fix"
			scopes: ["observability", "docker_logs source"]
			description: """
				The `docker_logs` source no longer increments `component_errors_total` for
				out-of-order logs since this is not an error.
				"""
			pr_numbers: [18649]
		},
		{
			type: "fix"
			scopes: ["codecs"]
			description: """
				The `native_json` codec no longer errors when encoding 64-bit float values that
				represent infinity. Instead, these are encoded as the strings `inf` and `-inf`. This
				most commonly occurred when encoding histograms where the last bucket bound included
				infinity.
				"""
			pr_numbers: [18650]
		},
		{
			type: "fix"
			scopes: ["http_server source"]
			description: """
				The `http_server` source no longer panics when handling metrics decoded using the
				`native` or `native_json` codecs. This means that it can now be used in conjunction
				with the `http` sink to send data between Vector instances using the `native` or
				`native_json` codecs; however, the `vector` source/sink pair is still the preferred route for
				Vector-to-Vector communication.
				"""
			pr_numbers: [18781]
		},
		{
			type: "fix"
			scopes: ["kafka source"]
			description: """
				The `kafka` source now fully drains acknowledgements during consumer group
				rebalancing and when Vector is shutting down. This avoids situations where Vector
				would duplicate message processing.
				"""
			contributors: ["jches"]
			pr_numbers: [14761]
		},
		{
			type: "fix"
			scopes: ["gcp_stackdriver_metrics sink"]
			description: """
				The `gcp_stackdriver_metrics` sink now correctly handles configured batch sizes
				greater than the default of `1`. Previously it would only send the last event in
				each batch.
				"""
			pr_numbers: [18749]
		},
		{
			type: "chore"
			scopes: ["datadog_metrics sink"]
			description: """
				The `datadog_metrics` sink now uses Datadog's `/api/v2/series` metrics endpoints to
				send timeseries data. This change should functionally be transparent to users but
				result in improved performance.
				"""
			pr_numbers: [18761]
		},
		{
			type: "chore"
			scopes: ["releasing"]
			description: """
				The armv7 rpm package, `vector-<version>-1.armv7.rpm`, is no longer published. It
				has been replaced by `vector-<version>-1.armv7hl.rpm` to better follow RPM
				guidelines. If using the `yum` package manager, this change should be transparent.

				See the [upgrade guide](/highlights/2023-11-07-0-34-0-upgrade-guide#armv7-rename)
				for more details.
				"""
			breaking: true
			pr_numbers: [18837]
		},
		{
			type: "fix"
			scopes: ["security", "networking"]
			description: """
				Sources that receive incoming TLS traffic now correctly apply any configured
				`tls.alpn_protocols` options. Previously, these were only applied for sources
				creating outgoing TLS connections.
				"""
			contributors: ["anil-db"]
			pr_numbers: [18843]
		},
		{
			type: "fix"
			scopes: ["sources"]
			description: """
				Sources now correctly emit a log and increment `component_discarded_events_total`
				when incoming requests are cancelled before the events are pushed to downstream
				components.
				"""
			pr_numbers: [18859]
		},
		{
			type: "enhancement"
			scopes: ["codecs"]
			description: """
				Sinks now have additional options for `encoding.timestamp_format`:

				- `unix_float`: Represents the timestamp as a Unix timestamp in floating point.
				- `unix_ms`: Represents the timestamp as a Unix timestamp in milliseconds.
				- `unix_ns`: Represents the timestamp as a Unix timestamp in nanoseconds.
				- `unix_us`: Represents the timestamp as a Unix timestamp in microseconds
				"""
			contributors: ["srstrickland"]
			pr_numbers: [18817]
		},
		{
			type: "fix"
			scopes: ["amqp sink"]
			description: """
				The `amqp` sink no longer panics when the channel is in an error state. Instead,
				Vector now emits an error event when this occurs.
				"""
			pr_numbers: [18923, 18932]
		},
		{
			type: "enhancement"
			scopes: ["prometheus_remote_write sink"]
			description: """
				The `prometheus_remote_write` sink now has the ability to disable aggregation by
				setting `batch.aggregate` to `false`.
				"""
			pr_numbers: [18676]
		},
		{
			type: "chore"
			scopes: ["datadog provider"]
			description: """
				The deprecated `region` configuration option was removed from the Datadog sinks.
				Instead of `region`, the `site` option should be used.

				See the [upgrade
				guide](/highlights/2023-11-07-0-34-0-upgrade-guide#datadog-deprecated-config-options)
				for more details.
				"""
			breaking: true
			pr_numbers: [18940]
		},
		{
			type: "enhancement"
			scopes: ["nats source"]
			description: """
				The `nats` source has a new `subscriber_capacity` configuration option to control
				how many messages the NATS subscriber buffers before incoming messages are dropped.
				"""
			pr_numbers: [18899]
		},
		{
			type: "chore"
			scopes: ["observability"]
			description: """
				The deprecated `component_name` tag has been removed from all internal metrics. Instead, the
				`component_id` tag should be used.
				"""
			breaking: true
			pr_numbers: [18942]
		},
		{
			type: "fix"
			scopes: ["datadog_agent source"]
			description: """
				The `datadog_agent` source now records the "interval" on any incoming metrics that
				have it set rather than just `rate`. This is useful as metrics can be interpreted as
				rates later when viewing the data in Datadog, where the `interval` field will be
				used.
				"""
			pr_numbers: [18889]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				Sources and sinks that run a HTTP server now emit additional internal metrics:

				- `http_server_requests_received_total`
				- `http_server_responses_sent_total`
				- `http_server_handler_duration_seconds`
				"""
			pr_numbers: [18887]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				Sources that run a gRPC server now emit additional internal metrics:

				- `grpc_server_messages_received_total`
				- `grpc_server_messages_sent_total`
				- `grpc_server_handler_duration_seconds`
				"""
			pr_numbers: [18887]
		},
		{
			type: "enhancement"
			scopes: ["buffers", "security"]
			description: """
				Event metadata, including secrets like Datadog API key or Splunk HEC token, are now
				persisted when writing events to a disk buffer so that sinks have access to it.

				As part of this change, disk buffers created by Vector now have more restrictive
				file permissions on Unix platforms. Previously, they were world-readable, but are now
				only writable by the Vector process user (typically `vector`) and readable by group.

				See the [release highlight](/highlights/2023-10-30-secrets-in-disk-buffers) for
				details about this change and recommended practices to either secure the disk
				buffers or to avoid storing secrets in events altogether.
				"""
			pr_numbers: [18816, 18887]
		},
		{
			type: "fix"
			scopes: ["blackhole sink"]
			description: """
				The `blackhole` sink no longer reports events processed by default. Instead, this
				behavior can be opted into by setting `print_interval_secs` to a non-zero number.

				See the [upgrade
				guide](/highlights/2023-11-07-0-34-0-upgrade-guide#blackhole-sink-reporting) for
				more details.
				"""
			breaking: true
			pr_numbers: [18963]
		},
		{
			type: "chore"
			scopes: ["observability"]
			description: """
				The deprecated `peer_addr` metric tag was removed from the
				`component_received_bytes_total` internal metric that was published by TCP-based
				sources to eliminate cardinality explosion of this metric.
				"""
			breaking: true
			pr_numbers: [18982]
		},
		{
			type: "chore"
			scopes: ["observability"]
			description: """
				Deprecated metrics that were redundant with `component_errors_total` were removed.

				See the [upgrade
				guide](/highlights/2023-11-07-0-34-0-upgrade-guide#deprecated-component-errors-total-metrics) for
				a full list.
				"""
			breaking: true
			pr_numbers: [18965]
		},
		{
			type: "enhancement"
			scopes: ["journald source"]
			description: """
				The `journald` source has a new `emit_cursor` option that, when enabled, adds the
				`__CURSOR` field to emitted log records.
				"""
			contributors: ["sproberts92"]
			pr_numbers: [18882]
		},
		{
			type: "chore"
			scopes: ["observability"]
			description: """
				Several internal metrics were deprecated for HTTP-based components:
				`requests_completed_total`, `request_duration_seconds`, and
				`requests_received_total`.

				See the [upgrade
				guide](/highlights/2023-11-07-0-34-0-upgrade-guide#deprecate-obsolete-http-metrics)
				for more details.
				"""
			pr_numbers: [18972]
		},
		{
			type: "feat"
			scopes: ["vrl"]
			description: """
				Vector's version of VRL was updated to 0.8.1, with the following changes:

				- Added the `contains_all` function
				- `from_unix_timestamp` now accepts a new unit, `microseconds`
				- `parse_nginx_log` no longer fails if `upstream_response_length`,
				  `upstream_response_time`, and `upstream_status` are missing
				- Added the `parse_float` function
				- Improved fallibility diagnostics
				"""
			pr_numbers: [19011]
		},
		{
			type: "feat"
			scopes: ["cli"]
			description: """
				Vector now has the ability to start with an empty configuration when using
				`--allow-empty-config`. This is useful if you want to start Vector before loading
				a configuration using `--watch-config` or when reloading.
				"""
			pr_numbers: [19021]
		},
		{
			type: "chore"
			scopes: ["security"]
			description: """
				In this release, we drop support for enabling the OpenSSL legacy provider when using
				`--openssl-legacy-provider` (and its environment variable:
				`VECTOR_OPENSSL_LEGACY_PROVIDER`).

				See the [upgrade
				guide](/highlights/2023-11-07-0-34-0-upgrade-guide#openssl-legacy-provider)
				for more details.
				"""
			breaking: true
			pr_numbers: [19015]
		},
		{
			type: "fix"
			scopes: ["clickhouse sink"]
			description: """
				A bug in the `clickhouse` sink heath check where it would add an extra `/` to the
				URI, resulting in failures, was fixed. This was a regression in v0.33.0.
				"""
			pr_numbers: [19061]
		},
	]

	commits: [
		{sha: "aca7753a229116fff56305a93f036bc122d75f30", date: "2023-09-27 06:12:29 UTC", description: "Bump semver from 1.0.18 to 1.0.19", pr_number: 18662, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "ca9e5b4b5f09d872f0f6738e774b2e39fd847cab", date: "2023-09-27 10:15:36 UTC", description: "Bump memmap2 from 0.7.1 to 0.8.0", pr_number: 18659, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "3dab23984a16fc7080ff95cfcbb3de3f4c45cc55", date: "2023-09-27 04:35:05 UTC", description: "Set partial Origin Metrics in edge cases", pr_number: 18677, scopes: ["datadog_metrics sink"], type: "chore", breaking_change: false, author: "neuronull", files_count: 2, insertions_count: 46, deletions_count: 20},
		{sha: "432777631445eea8c17551763808c7d767472258", date: "2023-09-27 11:47:27 UTC", description: "update environment for website development", pr_number: 18657, scopes: ["dev"], type: "fix", breaking_change: false, author: "Hugo Hromic", files_count: 2, insertions_count: 54, deletions_count: 11},
		{sha: "e2b7de07795b1a649ceb6d0e9555034a042d490b", date: "2023-09-27 07:51:01 UTC", description: "Add Vector workload checks", pr_number: 18569, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 8, insertions_count: 277, deletions_count: 1},
		{sha: "53cad38db12ceb11e0394b4d5906f7de541ec7dc", date: "2023-09-27 05:46:52 UTC", description: "Revive old remap tests", pr_number: 18678, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1416, deletions_count: 1420},
		{sha: "1c2f9704e4c4c799e75500b9250a976890e79329", date: "2023-09-28 00:15:16 UTC", description: "Bump Vector version to v0.34.0", pr_number: 18693, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "5f4c3baa0c7656bc162b0be2a336ed6845fd77b9", date: "2023-09-28 00:26:44 UTC", description: "Update manifests to v0.26.0 of the chart", pr_number: 18694, scopes: ["releasing", "kubernetes"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "c35ae64725b2f184761909d003a1c0d56f29b3ed", date: "2023-09-28 02:59:30 UTC", description: "Gate config conversion tests", pr_number: 18698, scopes: ["dev"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 6, deletions_count: 1},
		{sha: "c0d24b9d46ff09359a8a24a7d49d898bae8f4706", date: "2023-09-28 10:00:11 UTC", description: "Bump tempfile from 3.6.0 to 3.8.0", pr_number: 18686, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 18, deletions_count: 13},
		{sha: "efbe673780f00d5540b316bf57b25add14f8c449", date: "2023-09-28 15:09:28 UTC", description: "Bump toml from 0.8.0 to 0.8.1", pr_number: 18687, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 15, deletions_count: 15},
		{sha: "737f5c36545fa6fc9c71e0ca3c779e96902bf0af", date: "2023-09-30 09:07:22 UTC", description: "add support for protobuf encoding", pr_number: 18598, scopes: ["codecs"], type: "feat", breaking_change: false, author: "Glen Oakley", files_count: 50, insertions_count: 1502, deletions_count: 40},
		{sha: "d0e605ec4192d379c796ec771d3f12c6f8bda0c9", date: "2023-09-30 03:07:32 UTC", description: "Add more `dependabot` groups", pr_number: 18719, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 19, deletions_count: 0},
		{sha: "f98cd5d3a5558b459fce6c3b0afb0babb533b10c", date: "2023-09-30 09:11:33 UTC", description: "Bump check-spelling/check-spelling from 0.0.21 to 0.0.22", pr_number: 18723, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "93d7af46f465693a315b0ee69df5f91329742926", date: "2023-09-30 07:13:07 UTC", description: "do not emit component error for out of order logs", pr_number: 18649, scopes: ["docker source"], type: "fix", breaking_change: false, author: "Doug Smith", files_count: 2, insertions_count: 32, deletions_count: 45},
		{sha: "17bd2b1cb60861a9854a419636a74a08ab75bce2", date: "2023-09-30 04:31:42 UTC", description: "Add a summary if the regression workflow is skipped", pr_number: 18724, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 7, deletions_count: 0},
		{sha: "052ed98ad08813f4fdc41c2c86362bb6e5bc86d3", date: "2023-09-30 12:52:46 UTC", description: "Bump chrono from 0.4.30 to 0.4.31", pr_number: 18583, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 20, insertions_count: 78, deletions_count: 31},
		{sha: "6b92a83b05872b8343df9bdb0aefd6b4ca68169b", date: "2023-09-30 12:53:19 UTC", description: "Bump thiserror from 1.0.48 to 1.0.49", pr_number: 18683, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "87af0bd9c175131cc63a7952fe4ea455554a310f", date: "2023-09-30 12:53:28 UTC", description: "Bump sha2 from 0.10.7 to 0.10.8", pr_number: 18684, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "b37ce3cc9314333159f60a8e562a3c56ae32e1a2", date: "2023-09-30 12:53:38 UTC", description: "Bump apache-avro from 0.15.0 to 0.16.0", pr_number: 18685, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 54, deletions_count: 42},
		{sha: "65643daf9f2ce3d6cc315283fd15065518de792e", date: "2023-09-30 12:55:19 UTC", description: "Bump indexmap from 2.0.0 to 2.0.1", pr_number: 18705, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 21, deletions_count: 21},
		{sha: "c95df7cf5ec4178ed2e6bdee32c6445d7e193ce4", date: "2023-09-30 12:55:50 UTC", description: "Bump the tonic group with 2 updates", pr_number: 18714, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 9, deletions_count: 9},
		{sha: "27b2c93523afc25113128db1561684290baa594f", date: "2023-09-30 12:56:12 UTC", description: "Bump clap_complete from 4.4.2 to 4.4.3", pr_number: 18716, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "31d92c2746cd2797d8463bef64cfa035ff294481", date: "2023-09-30 13:02:58 UTC", description: "Bump hashbrown from 0.14.0 to 0.14.1", pr_number: 18731, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "eda0378d4b8d65fe2b674e3f96429ffcf325aa04", date: "2023-09-30 13:03:55 UTC", description: "Bump console-subscriber from 0.1.10 to 0.2.0", pr_number: 18732, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 12, deletions_count: 11},
		{sha: "539a40f2190ec4e0fe701691b33ade93a1baca15", date: "2023-09-30 13:29:44 UTC", description: "Bump mongodb from 2.6.1 to 2.7.0", pr_number: 18703, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 12, deletions_count: 22},
		{sha: "7dce29248f96e32737d8982ddbef74ecc068ac99", date: "2023-10-03 03:04:04 UTC", description: "Revet bump check-spelling/check-spelling from 0.0.21 to 0.0.22", pr_number: 18742, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "92d2be969e5bbc1af7f37abc417e003b03914348", date: "2023-10-04 06:59:28 UTC", description: "Bump postcss from 8.4.6 to 8.4.31 in /website", pr_number: 18750, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 8, deletions_count: 8},
		{sha: "4d98fdfa2131a216283bf73976514efb4de6241a", date: "2023-10-04 07:00:34 UTC", description: "Bump clap from 4.4.5 to 4.4.6", pr_number: 18715, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 24, deletions_count: 24},
		{sha: "047c7729f65bf741c7180ffea6f0af2d6723cb8e", date: "2023-10-04 07:43:19 UTC", description: "Bump async-nats from 0.32.0 to 0.32.1", pr_number: 18735, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c7482d059e6c590fa719ee23bc55849fd68d605d", date: "2023-10-04 07:43:57 UTC", description: "Bump memchr from 2.6.3 to 2.6.4", pr_number: 18736, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "f47df40104aaa60c637bce6ec06a6313de268199", date: "2023-10-04 07:46:43 UTC", description: "Bump indexmap from 2.0.1 to 2.0.2", pr_number: 18737, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 21, deletions_count: 21},
		{sha: "8e2032c407985a2dd77b7550b62d5fcfd04399d1", date: "2023-10-04 08:39:25 UTC", description: "Bump aws-actions/amazon-ecr-login from 1 to 2", pr_number: 18752, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "8e983219ea91831a7642c5112902940372aded7e", date: "2023-10-04 05:06:13 UTC", description: "Bump regex from 1.9.5 to 1.9.6", pr_number: 18739, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 10, deletions_count: 10},
		{sha: "f9e51e1c895389bae95064a4ad7811196839d8c0", date: "2023-10-04 12:06:58 UTC", description: "Bump toml from 0.8.1 to 0.8.2", pr_number: 18747, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 15, deletions_count: 15},
		{sha: "29e5e22aeb3e1f835940904813f54570c66ad085", date: "2023-10-04 05:09:01 UTC", description: "Add spec for `listen` option", pr_number: 18080, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 16, deletions_count: 0},
		{sha: "2b15c63b5f6a73f918f336b9f6acfaf4a6fd8f52", date: "2023-10-04 12:47:51 UTC", description: "Bump proptest from 1.2.0 to 1.3.1", pr_number: 18738, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 9, deletions_count: 9},
		{sha: "1452d54ac21ab69f8a3e0611913f348052be8f1b", date: "2023-10-05 06:49:54 UTC", description: "clean up VRL crate features", pr_number: 18740, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 28, deletions_count: 10},
		{sha: "39b9298a92f6c801b5f3be0d77f1b12bd240d6be", date: "2023-10-05 07:07:04 UTC", description: "native JSON serialization/deserialization for special f64 values", pr_number: 18650, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 167, deletions_count: 4},
		{sha: "afc166f37fd2704e474cec33f31d4f5bc224c94d", date: "2023-10-05 05:30:37 UTC", description: "the integration tests weren't actually validating anything", pr_number: 18754, scopes: ["datadog_metrics sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 5, insertions_count: 139, deletions_count: 119},
		{sha: "f300c85817f22bdd0187d11ae81bd50ebde170f5", date: "2023-10-06 01:27:22 UTC", description: "Remove @spencergilbert from CODEOWNERS", pr_number: 18778, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 26, deletions_count: 26},
		{sha: "54b54a5bd8c8a6ac0f13ee2cb607e2debd0befeb", date: "2023-10-06 03:03:10 UTC", description: "mark otlp_http_to_blackhole experiment erratic", pr_number: 18786, scopes: ["ci"], type: "chore", breaking_change: false, author: "Geoffrey Oxberry", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "dcbbb9b13ba97aee074f311f21a4bc42743db0de", date: "2023-10-07 01:08:45 UTC", description: "Bump memmap2 from 0.8.0 to 0.9.0", pr_number: 18765, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c531a3b3c7b1bff0f2b44a6a4dcc079ab26d5074", date: "2023-10-07 08:09:25 UTC", description: "Bump lru from 0.11.1 to 0.12.0", pr_number: 18767, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7cb8b52d80e6067482c85f23603fc75817e4b9df", date: "2023-10-07 08:09:43 UTC", description: "Bump csv from 1.2.2 to 1.3.0", pr_number: 18768, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "e9d2dae544dacb6e1b3835e6d84dd262bf3b916c", date: "2023-10-07 08:11:34 UTC", description: "Bump quanta from 0.11.1 to 0.12.0", pr_number: 18774, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 29, deletions_count: 4},
		{sha: "a784018715d700710f76257c5d337a65f8e6e145", date: "2023-10-07 08:12:07 UTC", description: "Bump bufbuild/buf-setup-action from 1.26.1 to 1.27.0", pr_number: 18783, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "ec5238ee9d868e01864fa9d68c895de8dcceb093", date: "2023-10-07 08:12:24 UTC", description: "Bump syn from 2.0.37 to 2.0.38", pr_number: 18789, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 40, deletions_count: 40},
		{sha: "f0adce73efe1db4d52235911a3093f661e7f93bc", date: "2023-10-07 02:25:24 UTC", description: "improve creation of Origin metadata structures", pr_number: 18788, scopes: ["metrics"], type: "chore", breaking_change: false, author: "neuronull", files_count: 5, insertions_count: 160, deletions_count: 103},
		{sha: "0e76fe06ac8c0c0a63d2cf319a4fb3a807b1dec3", date: "2023-10-07 04:25:24 UTC", description: "update quickstart.md to use YAML", pr_number: 18796, scopes: ["docs"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 39, deletions_count: 30},
		{sha: "98aa157a141a62a7827d29eef91758b39ff3b07e", date: "2023-10-07 01:35:57 UTC", description: "Group csv crate updates", pr_number: 18797, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 3, deletions_count: 0},
		{sha: "bac60add0d6f314bc77e1483b4554fcdb754768c", date: "2023-10-07 08:44:37 UTC", description: "Bump reqwest from 0.11.20 to 0.11.22", pr_number: 18760, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 27, deletions_count: 4},
		{sha: "ae117dc727284c09f3a8f60a876b14fa05a150bd", date: "2023-10-07 04:14:26 UTC", description: "Run deny check nightly instead of on every PR", pr_number: 18799, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 4, insertions_count: 82, deletions_count: 7},
		{sha: "570bd52dd6a1ea34b2128ad6bc53e66314db1ccb", date: "2023-10-07 04:21:32 UTC", description: "Bump dd-rust-license-tool to 1.0.2", pr_number: 18711, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "c9804f0c9e5a0931bbaaffe1270021d9c960fcb8", date: "2023-10-07 11:37:09 UTC", description: "Bump aws-actions/configure-aws-credentials from 4.0.0 to 4.0.1", pr_number: 18771, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "bc3b3a2bd1efded61dd3b2d72ed2b466f52f21bb", date: "2023-10-11 04:06:06 UTC", description: "Bump proc-macro2 from 1.0.67 to 1.0.69", pr_number: 18803, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 73, deletions_count: 73},
		{sha: "4701bb96d170d17cf3d611f97faf69c1853cbb71", date: "2023-10-11 04:09:14 UTC", description: "Bump tspascoal/get-user-teams-membership from 2 to 3", pr_number: 18808, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "76971bd9696f3498faa5bae4e586c0beb6e1f5e9", date: "2023-10-11 04:09:18 UTC", description: "Bump tokio from 1.32.0 to 1.33.0", pr_number: 18809, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 12, deletions_count: 12},
		{sha: "91221c6605526d42a6eeab1746e6938399a9b4a5", date: "2023-10-11 04:09:21 UTC", description: "Bump bstr from 1.6.2 to 1.7.0", pr_number: 18810, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 13, deletions_count: 7},
		{sha: "d9aca80873f1b5dc7863b483ab3426a5767a723b", date: "2023-10-11 04:09:28 UTC", description: "Bump semver from 1.0.19 to 1.0.20", pr_number: 18811, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "67c4beb8fbff4cdb0f988da16c10ff720fb36e05", date: "2023-10-11 00:04:08 UTC", description: "fix tokio unstable", pr_number: 18776, scopes: ["observability"], type: "chore", breaking_change: false, author: "Doug Smith", files_count: 2, insertions_count: 8, deletions_count: 2},
		{sha: "9d1a676626101208fd673a6b413e48e52c6d8626", date: "2023-10-12 00:41:29 UTC", description: "Remove unusued Dockerfile", pr_number: 18824, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 20},
		{sha: "b107ff706535a2a374cfb4d418f2c86d98628b3a", date: "2023-10-12 04:32:54 UTC", description: "panic when http server receives metric events", pr_number: 18781, scopes: ["http_server source"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 64, deletions_count: 58},
		{sha: "774094ec1f8972c01e26d3f8a35429bea2091e01", date: "2023-10-12 12:04:06 UTC", description: "add an example of parsing upstreaminfo with parse_nginx_log", pr_number: 18815, scopes: ["vrl"], type: "docs", breaking_change: false, author: "ex5", files_count: 1, insertions_count: 31, deletions_count: 0},
		{sha: "4002ef0458ac2f28a25d7321409732c702c52bac", date: "2023-10-12 13:29:46 UTC", description: "Bump libc from 0.2.148 to 0.2.149", pr_number: 18800, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c4fbc2579790dc77b3a7d19915cc5eb186456a70", date: "2023-10-12 13:30:04 UTC", description: "Bump num-traits from 0.2.16 to 0.2.17", pr_number: 18802, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "abb9101cc33200bd10ae3c7d1872d72501e93d27", date: "2023-10-12 13:30:13 UTC", description: "Bump serde-toml-merge from 0.3.2 to 0.3.3", pr_number: 18804, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "4d02abf0656e85ae7910b5eae8fbed356d9a5804", date: "2023-10-12 13:30:20 UTC", description: "Bump regex from 1.9.6 to 1.10.0", pr_number: 18812, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 17, deletions_count: 17},
		{sha: "99643ca32faa254a8cb863eb1ff71b3b5b3baf42", date: "2023-10-12 13:30:32 UTC", description: "Bump ordered-float from 4.1.0 to 4.1.1", pr_number: 18818, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 13, deletions_count: 13},
		{sha: "331c5a09c3d82a9fcf96a703d7a36b51639ccaa1", date: "2023-10-12 14:07:24 UTC", description: "Bump cached from 0.45.1 to 0.46.0", pr_number: 18660, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 13, deletions_count: 4},
		{sha: "f2efb1ac53f45621fff3f2ea7628cc1082040b5e", date: "2023-10-12 10:46:15 UTC", description: "fix acknowledgement handling during shutdown and rebalance events", pr_number: 17497, scopes: ["kafka source"], type: "fix", breaking_change: false, author: "j chesley", files_count: 2, insertions_count: 1023, deletions_count: 163},
		{sha: "92268e47692c9ab8a45cf44c05df658c4c74c953", date: "2023-10-12 23:42:07 UTC", description: "rewrite to stream based sink", pr_number: 18749, scopes: ["gcp_stackdriver_metrics sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 9, insertions_count: 703, deletions_count: 317},
		{sha: "3485f2c53617270403317f4c21bb076a8f53eeee", date: "2023-10-13 02:20:41 UTC", description: "support and migrate to the `v2` series API endpoint", pr_number: 18761, scopes: ["datadog_metrics sink"], type: "chore", breaking_change: false, author: "neuronull", files_count: 8, insertions_count: 681, deletions_count: 166},
		{sha: "96ef9eeed036f8723b4265f0c782e6423cbf6341", date: "2023-10-13 05:47:54 UTC", description: "Bump the zstd group with 1 update", pr_number: 18826, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 21, deletions_count: 3},
		{sha: "3c4ae86ec14fe835947d8f84d8cf977dffb2fa29", date: "2023-10-13 23:53:45 UTC", description: "added integration test for TLS", pr_number: 18813, scopes: ["amqp"], type: "feat", breaking_change: false, author: "Stephen Wakely", files_count: 15, insertions_count: 326, deletions_count: 7},
		{sha: "0776cc0ee18cda5a30deaef51fd2e8191643ce74", date: "2023-10-14 00:17:31 UTC", description: "temporarily peg greptimedb to `v0.4.0`  to unblock CI", pr_number: 18838, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 9, deletions_count: 1},
		{sha: "efb0d1a59074ea6e097918a230d552204161c42a", date: "2023-10-14 02:55:00 UTC", description: "remove config/vector.toml", pr_number: 18833, scopes: [], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 44},
		{sha: "6ffb072f548fdeaec444de7064d76ebff2fe2f67", date: "2023-10-14 02:55:07 UTC", description: "Convert config/examples from TOML to YAML", pr_number: 18832, scopes: ["examples"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 25, insertions_count: 317, deletions_count: 291},
		{sha: "1fb0f0d9404c600baa40c2d3abd05001ca08d1d4", date: "2023-10-14 02:55:17 UTC", description: "convert all regression cases configs to YAML", pr_number: 18825, scopes: ["regression"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 53, insertions_count: 1031, deletions_count: 940},
		{sha: "11bc5d98b8de9def6f56541285fd08e065c8b09f", date: "2023-10-14 03:33:11 UTC", description: "add PR comment trigger for the workload checks workflow", pr_number: 18839, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 13, deletions_count: 1},
		{sha: "eca8c761308dc28e52d09d1f7f6bf569f76b0eb7", date: "2023-10-16 23:07:41 UTC", description: "Remove armv7 RPM package", pr_number: 18837, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 1, deletions_count: 18},
		{sha: "2deeba11a8312a90059f9120a48d6609aa2bf5c2", date: "2023-10-17 00:43:26 UTC", description: "add more event metadata to proto", pr_number: 18816, scopes: ["core"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 4099, insertions_count: 2731, deletions_count: 1136},
		{sha: "4a7d0c33fa5ffef2b510f78714e687c6e03a6cf1", date: "2023-10-17 06:02:00 UTC", description: "Update `PartitionBatcher` to use `BatchConfig`", pr_number: 18792, scopes: ["sinks"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 29, insertions_count: 152, deletions_count: 231},
		{sha: "85d2f17dc0a6dc592220e2d93ea1758bb39afd99", date: "2023-10-17 06:46:29 UTC", description: "Bump @babel/traverse from 7.17.0 to 7.23.2 in /website", pr_number: 18852, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 131, deletions_count: 12},
		{sha: "0d09898867bcb489244aaba4e9a257eed9e97437", date: "2023-10-17 02:01:28 UTC", description: "Bump serde from 1.0.188 to 1.0.189", pr_number: 18834, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 11, deletions_count: 11},
		{sha: "434849d54cab5312237c3831d1e9a98e42e5fcce", date: "2023-10-17 05:12:18 UTC", description: "Bump regex from 1.10.0 to 1.10.2", pr_number: 18858, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 14, deletions_count: 14},
		{sha: "b3889bcea835a00395928e0743a3c14582dc426d", date: "2023-10-17 05:12:29 UTC", description: "Bump flate2 from 1.0.27 to 1.0.28", pr_number: 18850, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8d82257e433da08daca71dc51c7b835b733e1bff", date: "2023-10-17 11:12:42 UTC", description: "Bump async-trait from 0.1.73 to 0.1.74", pr_number: 18849, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1a8a8ccfa91f9fa0eebdb1d40fef4bc0967ffbcc", date: "2023-10-17 11:12:57 UTC", description: "Bump async-compression from 0.4.3 to 0.4.4", pr_number: 18848, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "0568d7a50e0a0e8b899edf83333d88a2cd752b04", date: "2023-10-17 11:13:09 UTC", description: "Bump trust-dns-proto from 0.23.0 to 0.23.1", pr_number: 18846, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "1da9005b00c04686ae768d173504b907cc6cb409", date: "2023-10-17 22:05:18 UTC", description: "First batch of editorial edits for the Functions doc", pr_number: 18780, scopes: ["external docs"], type: "chore", breaking_change: false, author: "May Lee", files_count: 75, insertions_count: 171, deletions_count: 179},
		{sha: "dc729f580164a96712cef0dc7414ae8daf4ea5d2", date: "2023-10-17 22:50:20 UTC", description: "Update lading to 0.19.0", pr_number: 18861, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "8a5b67e452772887d5353ff245d33bd4e2ed19ba", date: "2023-10-18 01:30:49 UTC", description: "for incoming connection alpn negotiation should be done using set_alpn_select_callback", pr_number: 18843, scopes: ["tls"], type: "fix", breaking_change: false, author: "Anil Gupta", files_count: 2, insertions_count: 20, deletions_count: 5},
		{sha: "2d7c1bbea68dea90e552e71d8ba240db35e6115f", date: "2023-10-18 08:05:34 UTC", description: "Bump lading to 0.19.1", pr_number: 18869, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "811b7f7fb9874acacd3402f50a7b0b252d9cda99", date: "2023-10-18 06:22:41 UTC", description: "Bump bufbuild/buf-setup-action from 1.27.0 to 1.27.1", pr_number: 18866, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a0e2769983daa1cdc62fe1af9135b58f489a2681", date: "2023-10-18 22:25:39 UTC", description: "emit `ComponentEventsDropped` when source send is cancelled", pr_number: 18859, scopes: ["sources"], type: "fix", breaking_change: false, author: "Doug Smith", files_count: 5, insertions_count: 169, deletions_count: 9},
		{sha: "3ca32b865a49b7d17fe6252f52519cd0765bbc9d", date: "2023-10-19 09:23:26 UTC", description: "Bump rustix from 0.37.19 to 0.37.25", pr_number: 18879, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "0e61accd512743fc5323ed70947428fd56098640", date: "2023-10-19 04:51:56 UTC", description: "Bump the azure group with 4 updates", pr_number: 18773, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 46, deletions_count: 49},
		{sha: "33243accf958588a0c1494c1a14da11180d5fc5b", date: "2023-10-19 10:52:33 UTC", description: "Bump serde_with from 3.3.0 to 3.4.0", pr_number: 18874, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 13, deletions_count: 13},
		{sha: "1913ee5cd5260d1246013fe9a99e1a86218b9d48", date: "2023-10-19 10:52:38 UTC", description: "Bump goauth from 0.13.1 to 0.14.0", pr_number: 18872, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "53039e72d36844e8188a3de2c25344005ac41c2a", date: "2023-10-20 00:52:15 UTC", description: "add unixtime formats", pr_number: 18817, scopes: ["timestamp encoding"], type: "feat", breaking_change: false, author: "Scott Strickland", files_count: 40, insertions_count: 322, deletions_count: 127},
		{sha: "ab8f8d28e2535340dcd6b146bc6072e6ae124fd7", date: "2023-10-20 04:00:03 UTC", description: "Update *-release.md issue templates for vector.dev package release", pr_number: 18814, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 2, deletions_count: 0},
		{sha: "8b002145656b59484eab4db0d18f8d9343cb1a20", date: "2023-10-20 04:43:22 UTC", description: "convert test config to yaml", pr_number: 18856, scopes: [], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 18, deletions_count: 11},
		{sha: "a025caab1119116b7f0b8c387c696caa71b682bf", date: "2023-10-20 05:43:32 UTC", description: "Bump uuid from 1.4.1 to 1.5.0", pr_number: 18880, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "aebe8db2090e56954998de9222154b5fdc43a365", date: "2023-10-21 05:14:09 UTC", description: "Bump hashbrown from 0.14.1 to 0.14.2", pr_number: 18893, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "16df7ea298e90e36d947a7cd2546ab15d404653e", date: "2023-10-21 05:14:21 UTC", description: "Bump thiserror from 1.0.49 to 1.0.50", pr_number: 18892, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "691fdca246c845942ca264fb5ed2d9d58e7284c7", date: "2023-10-21 13:03:14 UTC", description: "Bump socket2 from 0.5.4 to 0.5.5", pr_number: 18902, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 9, deletions_count: 9},
		{sha: "22402ca6ae4352e7afb32633543f660c20bd4ad4", date: "2023-10-24 01:41:39 UTC", description: "Bump toml from 0.8.2 to 0.8.3", pr_number: 18909, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 19, deletions_count: 19},
		{sha: "e754dee7a61975636c2e87a608376d954a4878a1", date: "2023-10-24 01:41:44 UTC", description: "Bump base64 from 0.21.4 to 0.21.5", pr_number: 18907, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 25, deletions_count: 25},
		{sha: "7debc602f7c63e64cfde67f566acca2b1567c4c3", date: "2023-10-24 04:18:30 UTC", description: "Bump fakedata_generator from 0.2.4 to 0.4.0", pr_number: 18910, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 12, deletions_count: 4},
		{sha: "8b56a933b87b55222bc3dc43c153ffe7c55b6517", date: "2023-10-24 04:18:41 UTC", description: "Bump ratatui from 0.23.0 to 0.24.0", pr_number: 18908, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 13, deletions_count: 14},
		{sha: "96f4d73d3a8614d721bdb27845b7721e8d266bb8", date: "2023-10-24 04:28:24 UTC", description: "Refactor `vector-core::stream` into its own package", pr_number: 18900, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 47, insertions_count: 166, deletions_count: 140},
		{sha: "e4fd78c78c16f0ef6c859b79615fce7338923ed4", date: "2023-10-24 11:43:20 UTC", description: "Bump tracing-log from 0.1.3 to 0.1.4", pr_number: 18914, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "1eaf8b1ec759f77461cee072e793ef2457737c0d", date: "2023-10-24 11:43:27 UTC", description: "Bump trust-dns-proto from 0.23.1 to 0.23.2", pr_number: 18911, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "cddb83570d8accf5b528254f53502b00385ee268", date: "2023-10-25 03:57:18 UTC", description: "Bump the clap group with 1 update", pr_number: 18906, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "249330a3a01b132feea17cd3fbd4b0ed22429524", date: "2023-10-25 01:31:20 UTC", description: "Bump toml from 0.8.3 to 0.8.4", pr_number: 18913, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 17, deletions_count: 17},
		{sha: "26f430c77138ef2373e86182966d5d5085b68514", date: "2023-10-25 04:16:33 UTC", description: "remove unnecessary unwrap & emit event dropped errors", pr_number: 18923, scopes: ["amqp sink"], type: "fix", breaking_change: false, author: "Doug Smith", files_count: 3, insertions_count: 41, deletions_count: 21},
		{sha: "bf56ac5b98569902b5a58e94b8afecd846545d14", date: "2023-10-25 02:52:49 UTC", description: "filter team members from gardener issue comment workflow", pr_number: 18915, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 18, deletions_count: 1},
		{sha: "78934c211d085dabe3be4c183b804b05d49303c4", date: "2023-10-25 05:03:54 UTC", description: "Bump the clap group with 2 updates", pr_number: 18925, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 27, deletions_count: 27},
		{sha: "5c1707f5972ff37d6bcd5782a157afac89efaa3d", date: "2023-10-26 00:28:24 UTC", description: "remote write sink rewrite", pr_number: 18676, scopes: ["prometheus_remote_write sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 41, insertions_count: 1597, deletions_count: 959},
		{sha: "c9c184ea3b6785823c723a818eb2b804b429cc3e", date: "2023-10-25 23:23:35 UTC", description: "Set up internal topology API", pr_number: 18919, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 14, insertions_count: 193, deletions_count: 204},
		{sha: "00c40d7cbe90287a0ee22ed2707576948a89cfff", date: "2023-10-25 23:29:05 UTC", description: "Set up `vector-lib` wrapper crate with `vector-common`", pr_number: 18927, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 260, insertions_count: 488, deletions_count: 552},
		{sha: "08b45a576bedd302d8dd6e4914f43c873052a998", date: "2023-10-26 01:38:20 UTC", description: "add fallibility examples", pr_number: 18931, scopes: ["docs"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 16, deletions_count: 0},
		{sha: "239cf942c165f067b4b21b43912ff0dac579db50", date: "2023-10-26 01:57:44 UTC", description: "Add announcement for new repository URLs", pr_number: 18798, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 7, insertions_count: 155, deletions_count: 11},
		{sha: "270fdfd6002f58275610995e232aae241ada4822", date: "2023-10-26 03:16:52 UTC", description: "Wrap `vector-core` in `vector-lib`", pr_number: 18934, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 401, insertions_count: 677, deletions_count: 658},
		{sha: "ffed6f70603a1f6702e85a96c45096a98924564a", date: "2023-10-26 10:53:50 UTC", description: "Bump serde-wasm-bindgen from 0.6.0 to 0.6.1", pr_number: 18935, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "a9166056653cc3ed2d7598f40281a70aed78d074", date: "2023-10-26 07:03:05 UTC", description: "remove duplicate events", pr_number: 18932, scopes: ["amqp sink"], type: "fix", breaking_change: false, author: "Doug Smith", files_count: 2, insertions_count: 4, deletions_count: 106},
		{sha: "a75a043523cb3839c4c186719565aeeeceba01cf", date: "2023-10-26 11:53:29 UTC", description: "Bump openssl-src from 300.1.5+3.1.3 to 300.1.6+3.1.4", pr_number: 18936, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "30a1e2613f63eaeed3e8768ee6423dba568fca4d", date: "2023-10-26 12:22:19 UTC", description: "Bump tracing-log from 0.1.4 to 0.2.0", pr_number: 18941, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 0, deletions_count: 2},
		{sha: "f42751d84a086a104bb8135f4fc419476070455c", date: "2023-10-27 01:01:30 UTC", description: "remove deprecated config options", pr_number: 18940, scopes: ["datadog"], type: "chore", breaking_change: true, author: "Doug Smith", files_count: 12, insertions_count: 45, deletions_count: 139},
		{sha: "cb53588f95a832f9cba60c5cf20b5cbf8375e56c", date: "2023-10-27 00:46:29 UTC", description: "remove unused feature flag", pr_number: 18948, scopes: ["amqp sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "42beb3f099f34c20b890b2d5f9f9ee07dc5697de", date: "2023-10-27 05:02:11 UTC", description: "Wrap `vector-config` in `vector-lib` as `configurable`", pr_number: 18944, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 215, insertions_count: 359, deletions_count: 300},
		{sha: "e7b563d1955a0acc350364aba90e5dcd8c7b3c6e", date: "2023-10-27 07:39:39 UTC", description: "add subscriber_capacity option", pr_number: 18899, scopes: ["nats source"], type: "enhancement", breaking_change: false, author: "Doug Smith", files_count: 2, insertions_count: 44, deletions_count: 0},
		{sha: "c6f5d2b62520cb6b9b923e029298c1e794011a3f", date: "2023-10-27 08:07:23 UTC", description: "remove deprecated `component_name` metric tag", pr_number: 18942, scopes: ["observability"], type: "chore", breaking_change: true, author: "Doug Smith", files_count: 6, insertions_count: 6, deletions_count: 24},
		{sha: "73afddb4292b113f659e9dc82d3d32d8ff3bf98d", date: "2023-10-27 13:05:54 UTC", description: "Bump serde from 1.0.189 to 1.0.190", pr_number: 18945, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 8, insertions_count: 11, deletions_count: 11},
		{sha: "4a525a833432ce340e00e174b769ec8ab9a38abb", date: "2023-10-27 13:06:09 UTC", description: "Bump toml from 0.8.4 to 0.8.5", pr_number: 18950, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 15, deletions_count: 15},
		{sha: "741aec36702efb80ebb3fa1a58c21a61ff181642", date: "2023-10-27 13:06:19 UTC", description: "Bump futures-util from 0.3.28 to 0.3.29", pr_number: 18951, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 17, deletions_count: 17},
		{sha: "8a02b168d4904d0837028b1ef9fc9743d9dee345", date: "2023-10-27 11:10:19 UTC", description: "Wrap `vector-stream` in `vector-lib`", pr_number: 18953, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 35, insertions_count: 37, deletions_count: 36},
		{sha: "dc9d966120f3f518ac1f1a07f4716020b69772eb", date: "2023-10-27 22:39:11 UTC", description: "handle interval for non-rate series metrics", pr_number: 18889, scopes: ["datadog_agent source", "datadog_metrics sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 4, insertions_count: 169, deletions_count: 80},
		{sha: "c23efce846ab8aec9ce12e638ba16d11281ec203", date: "2023-10-28 01:23:27 UTC", description: "add dependabot group for futures", pr_number: 18954, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "5f30f74bbb15edeea13051703ddd16945479c6c8", date: "2023-10-28 02:10:23 UTC", description: "Add wrapper for `codecs` to `vector-lib`", pr_number: 18959, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 119, insertions_count: 242, deletions_count: 214},
		{sha: "a477d720e00217603cbd2e81f427f28187641fa0", date: "2023-10-28 03:12:02 UTC", description: "improve request size limiting", pr_number: 18903, scopes: ["datadog_traces sink"], type: "fix", breaking_change: false, author: "Luke Steensen", files_count: 1, insertions_count: 324, deletions_count: 245},
		{sha: "e77901970b4d56168e24f9255fa42ed1f0e4ec86", date: "2023-10-28 04:37:14 UTC", description: "add telemetry to http and grpc servers", pr_number: 18887, scopes: ["sources", "sinks"], type: "enhancement", breaking_change: false, author: "Doug Smith", files_count: 25, insertions_count: 490, deletions_count: 119},
		{sha: "cf7298f80d09ddf5aff9ff0aec9a6b1ca7f12918", date: "2023-10-28 05:41:57 UTC", description: "apply stricter file permissions to buffer data files when possible", pr_number: 18895, scopes: ["buffers"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 60, deletions_count: 14},
		{sha: "3b85b48165c58013b4767e5db4620d3b9331b950", date: "2023-10-28 03:08:42 UTC", description: "Don't report by default", pr_number: 18963, scopes: ["observability", "blackhole sink"], type: "chore", breaking_change: true, author: "Jesse Szwedko", files_count: 3, insertions_count: 11, deletions_count: 4},
		{sha: "2ee96b17a743c28a0998e66f618d411feaceadf0", date: "2023-10-28 11:55:06 UTC", description: "Bump serde_yaml from 0.9.25 to 0.9.27", pr_number: 18956, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 8, deletions_count: 8},
		{sha: "40961edf9ae84f8a42294ca2f9870331a062d2a2", date: "2023-10-28 11:55:14 UTC", description: "Bump tempfile from 3.8.0 to 3.8.1", pr_number: 18957, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 23, deletions_count: 14},
		{sha: "0a5e3dbd9f2c81036266785be2075970dae4142d", date: "2023-10-28 11:55:22 UTC", description: "Bump the futures group with 1 update", pr_number: 18961, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 42, deletions_count: 42},
		{sha: "2cef62c0ab8f1bfa39c12687cb6b8b36b20ef856", date: "2023-10-28 07:22:33 UTC", description: "Add wrapper for `vector-buffers` to `vector-lib`", pr_number: 18964, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 20, insertions_count: 20, deletions_count: 21},
		{sha: "5e7ae83e7f575ade97e57bf5710cc943041e63d5", date: "2023-10-31 00:22:54 UTC", description: "Bump toml from 0.8.5 to 0.8.6", pr_number: 18962, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 15, deletions_count: 15},
		{sha: "c9b6d45a6194cbcbf4d33147ce0440f1ca7bc3c8", date: "2023-10-31 00:25:12 UTC", description: "Bump num_enum from 0.7.0 to 0.7.1", pr_number: 18975, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 7, deletions_count: 7},
		{sha: "b55e436205bb08009c8e077206ce08d7fd11eaa8", date: "2023-10-31 00:31:26 UTC", description: "Bump cargo_toml from 0.16.3 to 0.17.0", pr_number: 18978, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e61f308aab380b1af9c6b1e99a57d66181270ee8", date: "2023-10-31 02:35:34 UTC", description: "Add wrapper for `enrichment` to `vector-lib`", pr_number: 18977, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 31, insertions_count: 83, deletions_count: 64},
		{sha: "b9447f613b724e52db57fa4eed25aee04f167967", date: "2023-10-31 05:15:07 UTC", description: "remove `peer_addr` internal metric tag", pr_number: 18982, scopes: ["observability"], type: "chore", breaking_change: true, author: "Doug Smith", files_count: 6, insertions_count: 40, deletions_count: 41},
		{sha: "17f4ed2eaf1347c2fbb725400a7f706f9f8f464a", date: "2023-10-31 07:18:19 UTC", description: "remove metrics replaced by component_errors_total", pr_number: 18965, scopes: ["observability"], type: "chore", breaking_change: true, author: "Doug Smith", files_count: 64, insertions_count: 104, deletions_count: 493},
		{sha: "0051ec0e72ca1636410b714cd4326770fe0dc929", date: "2023-10-31 06:05:57 UTC", description: "Update license-tool.toml webpki version", pr_number: 18986, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "36974a0d847758a10914ff060c59ea455f67c67d", date: "2023-10-31 06:14:35 UTC", description: "Typo in v0.33.1 release docs", pr_number: 18987, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "72560b1291f64dc2b34b36c1507deac9f9a6e650", date: "2023-10-31 07:28:23 UTC", description: "Bump bufbuild/buf-setup-action from 1.27.1 to 1.27.2", pr_number: 18981, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "3ead10f8518914fbc9c8877cee5a181ea85c6f3c", date: "2023-10-31 08:08:12 UTC", description: "Update dependencies", pr_number: 18971, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 915, deletions_count: 930},
		{sha: "f44da167b24d926d47168167e599b79515919a44", date: "2023-10-31 10:03:10 UTC", description: "Add wrapper for `file-source` in `vector-lib`", pr_number: 18984, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 9, insertions_count: 23, deletions_count: 20},
		{sha: "7f44b4c846638b85ff09c44bce32ac8b0a1066e4", date: "2023-11-01 01:05:00 UTC", description: "Bump chrono-tz from 0.8.3 to 0.8.4", pr_number: 18979, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 3, deletions_count: 3},
		{sha: "4a4eb61c345b22ef471729cb79a8f322ef4e0b77", date: "2023-11-01 01:09:54 UTC", description: "Bump async-graphql from 5.0.10 to 6.0.9", pr_number: 18988, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 10, deletions_count: 80},
		{sha: "1eb418bedd34a2ac8efe9f2980e2263e42e84abe", date: "2023-10-31 22:42:33 UTC", description: "Add `vector-lib` wrapper for three more libs", pr_number: 18992, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 17, insertions_count: 53, deletions_count: 38},
		{sha: "164f1e9b716743569c3ede5ec5936a28a6882142", date: "2023-11-01 01:04:25 UTC", description: "Add wrapper for `lookup` in `vector-lib`", pr_number: 18995, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 103, insertions_count: 188, deletions_count: 273},
		{sha: "9893b8697e2e3ab30edf85d90fbb0b45244c8f6e", date: "2023-11-01 03:04:29 UTC", description: "Follow redirects for `sh.vector.dev`", pr_number: 19000, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 7, insertions_count: 8, deletions_count: 8},
		{sha: "5e9dd1d00aa26e27b82c376a775ac58b2e5b8a50", date: "2023-11-01 03:17:58 UTC", description: "Regenerate manifests from 0.27.0 chart", pr_number: 19001, scopes: ["kubernetes"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "43f5913153dd0129c67fabac7850cea0bd5ba8e9", date: "2023-11-02 00:42:25 UTC", description: "Bump pulsar from 6.0.1 to 6.1.0", pr_number: 19004, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 252, deletions_count: 32},
		{sha: "21f741dbba034b382881ba1c5efeef265e2fa5c8", date: "2023-11-02 03:34:50 UTC", description: "Add required fields to documentation examples", pr_number: 18998, scopes: ["gcp_pubsub source"], type: "docs", breaking_change: false, author: "Richard Tweed", files_count: 2, insertions_count: 4, deletions_count: 2},
		{sha: "371580c902822ada9f7bcb501c40ec6ddc6bb51b", date: "2023-11-01 21:42:46 UTC", description: "Remove usages of atty", pr_number: 18985, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 6, insertions_count: 10, deletions_count: 7},
		{sha: "051de5afbd29ded9bf9cb321fa21d80d8ed39700", date: "2023-11-02 06:22:11 UTC", description: "Bump quanta from 0.12.0 to 0.12.1", pr_number: 19005, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "74051dc85388ca3683e5b932653c0e6a4511a702", date: "2023-11-02 20:26:35 UTC", description: "Add emit_cursor option", pr_number: 18882, scopes: ["journald source"], type: "feat", breaking_change: false, author: "Samuel Roberts", files_count: 2, insertions_count: 43, deletions_count: 5},
		{sha: "be9f229dd56b483b5223c3da57ac9f690ddc0a13", date: "2023-11-02 02:29:49 UTC", description: "Note the version to remove the v1 metrics support from the Datadog Metrics sink", pr_number: 19017, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "ff7b95fb7192a7f56cb4cc53a020e39d32de2d72", date: "2023-11-02 02:42:56 UTC", description: "Remove deprecation action item for armv7 RPMs", pr_number: 19018, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "63bb9e497f4ca9cefdc1221371fa883d0bd1d529", date: "2023-11-03 00:13:53 UTC", description: "Bump wasm-bindgen from 0.2.87 to 0.2.88", pr_number: 19026, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 10, deletions_count: 10},
		{sha: "602f630ded590380993fd109fc23b5e7c5cb7e64", date: "2023-11-03 00:14:50 UTC", description: "Bump mongodb from 2.7.0 to 2.7.1", pr_number: 19023, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "72eacf510c4d4efbc07672b584aeb5af677aa483", date: "2023-11-03 00:15:24 UTC", description: "Bump inventory from 0.3.12 to 0.3.13", pr_number: 19024, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c4f2d0e41054729a9427b0861532f7984eb32be4", date: "2023-11-03 00:16:13 UTC", description: "Bump openssl from 0.10.57 to 0.10.58", pr_number: 19025, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "88194e76df9e063236777f1e2166624cbd348d1b", date: "2023-11-02 21:52:13 UTC", description: "Update cargo-deb", pr_number: 19009, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "2cdf6547b0fa0d6ac867f95e8470b26168e1f7e8", date: "2023-11-02 22:35:19 UTC", description: "Unmark regression tests as erratic now", pr_number: 19020, scopes: ["regression"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 0, deletions_count: 3},
		{sha: "250104965db6cccbe922d6bc0cd1f73a6177f8c6", date: "2023-11-02 23:00:22 UTC", description: "Fix changelog not for kafka fix in 0.33.1", pr_number: 19032, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "2bba40a0baad268335bf725a327fcf20e9f6ec9b", date: "2023-11-03 00:01:01 UTC", description: "Remove legacy OpenSSL provider flags", pr_number: 19015, scopes: ["security"], type: "chore", breaking_change: true, author: "Jesse Szwedko", files_count: 5, insertions_count: 10, deletions_count: 57},
		{sha: "9d006c7e345645051a06affa9130305d85003cbf", date: "2023-11-03 03:13:47 UTC", description: "Setup preview site workflows", pr_number: 18924, scopes: ["websites"], type: "chore", breaking_change: false, author: "Devin Ford", files_count: 8, insertions_count: 144, deletions_count: 137},
		{sha: "df4921b904a0310d8f9d48bdc456ab513594ebb0", date: "2023-11-03 03:50:23 UTC", description: "Add a CLI flag to allow for empty configs", pr_number: 19021, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 8, insertions_count: 59, deletions_count: 14},
		{sha: "223dd7b22967369669734e7c6f476559c7de6533", date: "2023-11-03 03:14:45 UTC", description: "Detail the format of DEPRECATIONS.md file", pr_number: 19016, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 13, deletions_count: 1},
		{sha: "f5ea28500ebc255630318895e2926029165afb1e", date: "2023-11-03 06:47:06 UTC", description: "Update VRL to 0.8.1", pr_number: 19011, scopes: ["deps"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 14, insertions_count: 138, deletions_count: 48},
		{sha: "b4ca866bd6ae30dae4a27c83a2838d50486001ac", date: "2023-11-04 00:04:11 UTC", description: "Workflow updates", pr_number: 19036, scopes: ["websites"], type: "chore", breaking_change: false, author: "Devin Ford", files_count: 4, insertions_count: 34, deletions_count: 17},
		{sha: "5655f7674b27b09e24782d291a9613ff1216c58e", date: "2023-11-04 13:51:43 UTC", description: "Bump OpenDAL to v0.41", pr_number: 19039, scopes: ["deps"], type: "chore", breaking_change: false, author: "Xuanwo", files_count: 4, insertions_count: 12, deletions_count: 34},
		{sha: "8ba28e007381c22783d0a82abe4e6a312682fdc8", date: "2023-11-04 05:54:11 UTC", description: "Bump dyn-clone from 1.0.14 to 1.0.16", pr_number: 19040, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "f33dce27a3743cb37a6e6f7c07889003427b5b54", date: "2023-11-04 03:36:15 UTC", description: "deprecate obsolete http metrics", pr_number: 18972, scopes: ["observability"], type: "chore", breaking_change: false, author: "Doug Smith", files_count: 2, insertions_count: 16, deletions_count: 0},
		{sha: "fb63f8e0545332faa3bf4ff00de894c8f851deda", date: "2023-11-04 05:29:02 UTC", description: "Workflow fixes", pr_number: 19046, scopes: ["website"], type: "chore", breaking_change: false, author: "Devin Ford", files_count: 2, insertions_count: 20, deletions_count: 4},
		{sha: "1c864aa2753b497392f802dfce194dec5e41803a", date: "2023-11-04 08:27:04 UTC", description: "remove gh token call", pr_number: 19047, scopes: ["ci"], type: "fix", breaking_change: false, author: "Devin Ford", files_count: 2, insertions_count: 1, deletions_count: 6},
		{sha: "0cc9389822bdbf677dd41fb59fc0c074d788f40d", date: "2023-11-04 09:05:38 UTC", description: "add highlight post for secrets in disk buffers", pr_number: 18994, scopes: ["docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 83, deletions_count: 0},
		{sha: "0f0a0b4e8b67a0ed692992acdeef504f9331b024", date: "2023-11-07 02:23:34 UTC", description: "Bump syn from 2.0.38 to 2.0.39", pr_number: 19056, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 43, deletions_count: 43},
		{sha: "ee232f8cccc3a7b6835dde6a97cd7c7a4991f1ab", date: "2023-11-07 02:28:24 UTC", description: "Bump openssl from 0.10.58 to 0.10.59", pr_number: 19054, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "4622ef6a6129d40cb6b69ffec14e31b8d569fc58", date: "2023-11-07 02:30:43 UTC", description: "Bump wiremock from 0.5.19 to 0.5.21", pr_number: 19055, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7a16ee2c78fb9d5beb89734f9d7b9835fb0df1c7", date: "2023-11-07 02:44:54 UTC", description: "Bump bitmask-enum from 2.2.2 to 2.2.3", pr_number: 19057, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "611a652cfd5b056cde09bc2100674b4a758ae1f3", date: "2023-11-07 02:46:40 UTC", description: "Bump libc from 0.2.149 to 0.2.150", pr_number: 19059, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "4f613ce3c1c498fd61c8c60cb282a81e83174521", date: "2023-11-07 01:18:57 UTC", description: "Move some macros into `vector-core`", pr_number: 19002, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 62, insertions_count: 88, deletions_count: 146},
		{sha: "51c6b579059494f667081612eb31cb041dac7a75", date: "2023-11-07 07:49:09 UTC", description: "Bump cached from 0.46.0 to 0.46.1", pr_number: 19058, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "53ffbc3a1e3acf2df02918739de6b5d6cef1e900", date: "2023-11-07 07:51:44 UTC", description: "Bump the azure group with 4 updates", pr_number: 19052, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 60, deletions_count: 28},
		{sha: "7a7b53b1c6958f346648042de84d53f6c7357064", date: "2023-11-07 08:13:35 UTC", description: "fix healthcheck uri", pr_number: 19067, scopes: ["clickhouse sink"], type: "fix", breaking_change: false, author: "Doug Smith", files_count: 1, insertions_count: 26, deletions_count: 1},
	]
}
