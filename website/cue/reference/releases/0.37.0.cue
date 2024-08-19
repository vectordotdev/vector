package metadata

releases: "0.37.0": {
	date:     "2024-03-26"
	codename: ""

	whats_next: []

	description: """
		The Vector team is pleased to announce version 0.37.0!

		Be sure to check out the [upgrade guide](/highlights/2023-03-26-0-37-0-upgrade-guide) for
		breaking changes in this release.

		In addition to the usual enhancements and bug fixes, this release also includes

		- ARMv6 builds of Vector including Debian archives and container images. The Debian
		  archives, for now, are only hosted as release assets and are not published to the Debian
		  repository. We are looking into publishing them there in the future. No RPM packages are
		  built at this time. Kudos to [@wtaylor](https://github.com/wtaylor) for this contribution.
		- A new `mqtt` sink to emit events from Vector using the MQTT protocol. A source is [in the
		  works](https://github.com/vectordotdev/vector/pull/19931). Kudos to the contributors that
		  pushed this forward: [@astro](https://github.com/astro),
		  [@zamazan4ik](https://github.com/zamazan4ik), and [@mladedav](https://github.com/mladedav).
		- The `dnstap` source now supports reading events over TCP. Kudos to
		  [@esensar](https://github.com/esensar) for this contribution.
		- A new `mmdb` enrichment table type for loading arbitrary mmdb databases and not just GeoIP
		  ones. Kudos to [@esensar](https://github.com/esensar) for this contribution.
		- A new `pulsar` source for receiving events from Pulsar. Kudos to
		  [@zamazan4ik](https://github.com/zamazan4ik) and [@WarmSnowy](https://github.com/WarmSnowy)
		  for this contribution.
		"""

	known_issues: [
		"""
			The `geoip2` enrichment table type stopped handling `GeoLite2-City` mmdb types. This is fixed in v0.37.1.
			""",
		"""
			The `parse_ddtags` setting added to the `datadog_agent` source incorrectly parses the
			tags into an object instead of an array. The `datadog_logs` sink also fails to
			reconstruct the parsed tags. This will be fixed in v0.38.0.
			""",
	]

	changelog: [
		{
			type: "enhancement"
			description: """
				ARMv6 builds are now provided as binaries, `.deb` archives and container images (alpine and debian).
				"""
			contributors: ["wtaylor"]
		},
		{
			type: "enhancement"
			description: """
				A new configuration option `rotate_wait_secs` was added to the `file` and `kubernetes_logs` sources. `rotate_wait_secs` determines for how long Vector keeps trying to read from a log file that has been deleted. Once that time span has expired, Vector stops reading from and closes the file descriptor of the deleted file, thus allowing the OS to reclaim the storage space occupied by the file.
				"""
			contributors: ["syedriko"]
		},
		{
			type: "feat"
			description: """
				Vector can send logs to a MQTT broker through the new mqtt sink.
				"""
			contributors: ["astro", "zamazan4ik", "StephenWakely", "mladedav"]
		},
		{
			type: "enhancement"
			description: """
				A new `EXPRESS_ONEZONE` option was added to `storage_class` for `aws_s3` sink.
				"""
			contributors: ["siavashs"]
		},
		{
			type: "chore"
			description: """
				Added support for TCP mode for DNSTAP source. As the `dnstap` source now supports multiple socket types, you will need to update your configuration to specify which type - either `mode: unix` for the existing unix sockets mode or `mode: tcp` for the new tcp mode.
				"""
			contributors: ["esensar"]
		},
		{
			type: "enhancement"
			description: """
				Added support for more DNS record types (HINFO, CSYNC, OPT, DNSSEC CDS, DNSSEC CDNSKEY, DNSSEC KEY)
				"""
			contributors: ["esensar"]
		},
		{
			type: "feat"
			description: """
				Added support for parsing EDNS EDE (Extended DNS errors) options
				"""
			contributors: ["esensar"]
		},
		{
			type: "enhancement"
			description: """
				Improves TLS support for greptimedb sink. `tls.ca_file` is no longer required for enabling TLS. Just use `tls = {}` in toml configuration when your server is hosting a public CA.
				"""
			contributors: ["sunng87"]
		},
		{
			type: "fix"
			description: """
				Fixed gzip and zlib compression performance degradation introduced in v0.34.0.
				"""
			contributors: ["Hexta"]
		},
		{
			type: "feat"
			description: """
				Added `lowercase_hostnames` option to `dnstap` source, to filter hostnames in DNS records and
				lowercase them for consistency.
				"""
			contributors: ["esensar"]
		},
		{
			type: "feat"
			description: """
				Added support for `permit_origin` config option for all sources with TCP mode (`fluent`, `logstash`, `statsd`, `syslog`).
				"""
			contributors: ["esensar"]
		},
		{
			type: "feat"
			description: """
				Added support for custom MMDB enrichment tables. GeoIP enrichment tables will no longer fall back to
				City type for unknown types and will instead return an error. New MMDB enrichment table should be
				used for such types.
				"""
			contributors: ["esensar"]
		},
		{
			type: "chore"
			description: """
				When end-to-end acknowledgments are enabled, outgoing requests in the ClickHouse sink that encounter 500-level errors will now correctly report an errored (retriable) status, rather than a rejected (permanent) status, to Vector's clients.
				"""
		},
		{
			type: "enhancement"
			description: """
				The `datadog_agent` source now contains a configuration setting `parse_ddtags`, which is disabled by default.

				When enabled, the `ddtags` field (a comma separated list of key-value strings) is parsed and expanded into an
				object in the event.
				"""
		},
		{
			type: "fix"
			description: """
				The `datadog_agent` source now correctly calculates the value for the metric `component_received_event_bytes_total` before enriching the event with Vector metadata.

				The source also now adheres to the Component Specification by incrementing `component_errors_total` when a request succeeded in decompression but JSON parsing failed.
				"""
		},
		{
			type: "fix"
			description: """
				The `datadog_logs` sink no longer requires  a semantic meaning input definition for `message` and `timestamp` fields.

				While the Datadog logs intake does handle these fields if they are present, they aren't required.

				The only impact is that configurations which enable the [Log Namespace](https://vector.dev/blog/log-namespacing/) feature and use a Source input to this sink which does not itself define a semantic meaning for `message` and `timestamp`, no longer need to manually set the semantic meaning for these two fields through a remap transform.

				Existing configurations that utilize the Legacy namespace are unaffected, as are configurations using the Vector namespace where the input source has defined the `message` and `timestamp` semantic meanings.
				"""
		},
		{
			type: "chore"
			description: """
				The default of `--strict-env-vars` has been changed to `true`. This option has been deprecated. In
				a future version it will be removed and Vector will have the behavior it currently has when set
				to `true` which is that missing environment variables will cause Vector to fail to start up with an
				error instead of a warning. Set `--strict-env-vars=false` to opt into deprecated behavior.
				"""
		},
		{
			type: "fix"
			description: """
				An error log for the Elasticsearch sink that logs out the response body when errors occur. This was
				a log that used to exist in Vector v0.24.0, but was removed in v0.25.0. Some users were depending on
				this log to count the number of errors so it was re-added.
				"""
		},
		{
			type: "fix"
			description: """
				The `fingerprint.ignored_header_bytes` option on the `file` source now has a default of `0`.
				"""
		},
		{
			type: "enhancement"
			description: """
				A new configuration option `include_paths_glob_patterns` has been introduced in the Kubernetes Logs source. This option works alongside the existing `exclude_paths_glob_patterns` to help narrow down the selection of logs to be considered. `include_paths_glob_patterns` is evaluated before `exclude_paths_glob_patterns`.
				"""
			contributors: ["syedriko"]
		},
		{
			type: "feat"
			description: """
				A new source has been added that can receive logs from Apache Pulsar.
				"""
			contributors: ["zamazan4ik", "WarmSnowy"]
		},
		{
			type: "enhancement"
			description: """
				The `remap` component no longer filters out the file contents from error messages when the VRL
				program is passed in via the `file` option.
				"""
		},
		{
			type: "fix"
			description: """
				The `splunk_hec_logs` sink when configured with the `raw` endpoint target, was removing the timestamp from the event. This was due to a bug in the handling of the `auto_extract_timestamp` configuration option, which is only supposed to apply to the `event` endpoint target.
				"""
		},
		{
			type: "fix"
			description: """
				We now correctly calculate the estimated JSON size in bytes for the metric `component_received_event_bytes_total` for the `splunk_hec` source.

				Previously this was being calculated after event enrichment. It is now calculated before enrichment, for both `raw` and `event` endpoints.
				"""
		},
	]

	commits: [
		{sha: "7c0072689fba435640e26e63d46343064c477b0f", date: "2024-02-13 01:04:56 UTC", description: "Add a note that GH usernames shouldn't start with @", pr_number: 19859, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "8d897af2f621a0402678141a3a94e1196ea56037", date: "2024-02-13 01:17:57 UTC", description: "Fix API address example", pr_number: 19858, scopes: ["api"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "e0d5f1e4dbd433165c525e941c95dd8eea2ebee6", date: "2024-02-13 10:09:28 UTC", description: "Bump the aws group with 2 updates", pr_number: 19848, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "1637e566c08f5dc2b09e5c85ad49a93762647c06", date: "2024-02-13 02:25:25 UTC", description: "Bump manifists to chart v0.30.2", pr_number: 19860, scopes: ["kubernetes"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 28, deletions_count: 24},
		{sha: "79ab38947f5869afe154f83cf15868c01b43ac4b", date: "2024-02-13 08:20:01 UTC", description: "expose VRL deserializer options", pr_number: 19862, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "e8401c473fb0334c36ac91a411392f1ac7ae9ce5", date: "2024-02-13 09:06:15 UTC", description: "expose test utils (feature flag)", pr_number: 19863, scopes: ["tests"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 9, deletions_count: 9},
		{sha: "99c2207932894d362975fa81000b4819d5e7bb52", date: "2024-02-13 22:43:08 UTC", description: "Bump chrono-tz from 0.8.5 to 0.8.6", pr_number: 19866, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 6, deletions_count: 6},
		{sha: "c654207d5a41c8ec9fff4ac497ac3cec7a40c55c", date: "2024-02-13 22:43:19 UTC", description: "Bump crc32fast from 1.3.2 to 1.4.0", pr_number: 19867, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0922c3f67f57e2d8c29029a91e1f60ab4d699f50", date: "2024-02-14 06:43:34 UTC", description: "Bump ratatui from 0.26.0 to 0.26.1", pr_number: 19868, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0a89cb13714876da089ea09d4881e98a890b3976", date: "2024-02-14 07:04:04 UTC", description: "add rotate_wait_ms config option", pr_number: 18904, scopes: ["file source", "kubernetes_logs source"], type: "enhancement", breaking_change: false, author: "Sergey Yedrikov", files_count: 10, insertions_count: 94, deletions_count: 6},
		{sha: "f88316cce7665c6dbf83a81a8261fa126b50542e", date: "2024-02-15 10:41:26 UTC", description: "add MQTT sink", pr_number: 19813, scopes: ["mqtt sink"], type: "feat", breaking_change: false, author: "David Mládek", files_count: 24, insertions_count: 1328, deletions_count: 2},
		{sha: "a935c30785ad50adfea5a3344e2fb3673fffb73c", date: "2024-02-15 03:44:56 UTC", description: "Bump manifests to chart v0.36.0", pr_number: 19877, scopes: ["kubernetes"], type: "feat", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "b91be34a3c890505e7faeaeffa4a1bea54944ebf", date: "2024-02-15 04:33:09 UTC", description: "Bump development version to v0.37.0", pr_number: 19874, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "342b48c0f7c0aa1147a3a2a1b00089a482436560", date: "2024-02-16 01:42:36 UTC", description: "Bump darling from 0.20.5 to 0.20.6", pr_number: 19882, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 13, deletions_count: 13},
		{sha: "2f1c7850fbc039a894f51b844e919adf2fdc925d", date: "2024-02-16 08:02:24 UTC", description: "RFC for return expression", pr_number: 19828, scopes: ["vrl"], type: "enhancement", breaking_change: false, author: "David Mládek", files_count: 1, insertions_count: 74, deletions_count: 0},
		{sha: "4f0dbf4d2792dc266e0b9ea74158a6a96a1adccb", date: "2024-02-16 08:49:31 UTC", description: "Bump openssl-src from 300.2.2+3.2.1 to 300.2.3+3.2.1", pr_number: 19869, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "c89099768af4ee63542dcb8c039e35bd7a6f2832", date: "2024-02-16 05:01:47 UTC", description: "expose more test utils", pr_number: 19885, scopes: ["tests"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "f920675d2658d5ea410847390d7ba3be435a932a", date: "2024-02-16 10:09:29 UTC", description: "Bump enumflags2 from 0.7.8 to 0.7.9", pr_number: 19870, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "a7fe0dbfbd41197bb09fb6a8f2d8562a22384c99", date: "2024-02-16 05:16:09 UTC", description: "add support for include_paths_glob_patterns", pr_number: 19521, scopes: ["kubernetes"], type: "enhancement", breaking_change: false, author: "Sergey Yedrikov", files_count: 5, insertions_count: 127, deletions_count: 15},
		{sha: "9a0a5e4784bf80af8be7a7e8cfa8516a70d39704", date: "2024-02-16 10:16:40 UTC", description: "Update HttpRequest struct to pass additional metadata", pr_number: 19780, scopes: ["http sink"], type: "enhancement", breaking_change: false, author: "Sebastian Tia", files_count: 22, insertions_count: 271, deletions_count: 302},
		{sha: "448c9d19148c3707af54c7e2be90440de3a0316c", date: "2024-02-16 06:32:58 UTC", description: "Bump MSRV from 1.71.1 to 1.74", pr_number: 19884, scopes: ["deps"], type: "chore", breaking_change: false, author: "Sebastian Tia", files_count: 0, insertions_count: 0, deletions_count: 0},
		{sha: "2b0f06eb5de6dc008bd4c98e49ce82a5f0837942", date: "2024-02-17 06:52:33 UTC", description: "Bump syn from 2.0.48 to 2.0.49", pr_number: 19890, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 47, deletions_count: 47},
		{sha: "8223dca26efd790ec4fdbf5eb7626f2cc32d99a2", date: "2024-02-17 07:24:15 UTC", description: "Bump roaring from 0.10.2 to 0.10.3", pr_number: 19889, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 3, deletions_count: 16},
		{sha: "788f0c30ee259d5e918be074d059085107bd69bc", date: "2024-02-17 07:46:52 UTC", description: "Bump the aws group with 4 updates", pr_number: 19888, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 13, deletions_count: 12},
		{sha: "5d8160d72743df1e02fff9f69a8d4e37e1f2577a", date: "2024-02-17 09:32:26 UTC", description: "add express one zone storage class", pr_number: 19893, scopes: ["s3 sink"], type: "enhancement", breaking_change: false, author: "Siavash Safi", files_count: 5, insertions_count: 12, deletions_count: 3},
		{sha: "a798f681d392e761d3e1e185ca9d7e8075a892c5", date: "2024-02-17 08:39:40 UTC", description: "update HTTP request builder to return error", pr_number: 19886, scopes: ["http sink"], type: "enhancement", breaking_change: false, author: "Sebastian Tia", files_count: 8, insertions_count: 69, deletions_count: 39},
		{sha: "50a0c9bc118ee282144b14b3ed49f84cb5ce7c93", date: "2024-02-17 05:41:48 UTC", description: "Add a timeout to all CI jobs", pr_number: 19895, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 29, insertions_count: 69, deletions_count: 1},
		{sha: "78f0e31c8445355203fb5295224af7da1de19e1b", date: "2024-02-21 22:53:03 UTC", description: "Bump the aws group with 1 update", pr_number: 19919, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "6a76be2173ad5a3d919e20e0661a7f3fc543427d", date: "2024-02-22 06:55:56 UTC", description: "Bump mock_instant from 0.3.1 to 0.3.2", pr_number: 19900, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "837c64cffd3624e32178a1e5078ed5ed3e6ebc8a", date: "2024-02-22 06:56:27 UTC", description: "Bump serde_yaml from 0.9.31 to 0.9.32", pr_number: 19907, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 10, deletions_count: 10},
		{sha: "bb4190b028f24c51fa6296830aa6036f68c5596b", date: "2024-02-22 06:58:13 UTC", description: "Bump assert_cmd from 2.0.13 to 2.0.14", pr_number: 19908, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7311c0aaa01cac20d4cdc71c21c516de7326405c", date: "2024-02-22 06:58:35 UTC", description: "Bump serde from 1.0.196 to 1.0.197", pr_number: 19910, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "b8d89a03459a32f9c227b6fab21b5081c75d934f", date: "2024-02-22 06:58:49 UTC", description: "Bump semver from 1.0.21 to 1.0.22", pr_number: 19911, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "1d91742e70a3c5ef4ae3a86c26a6d89846e35157", date: "2024-02-22 06:59:00 UTC", description: "Bump ryu from 1.0.16 to 1.0.17", pr_number: 19912, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "7fb4513424aa9c3d19fa0e43c7be2360d2ac412d", date: "2024-02-22 06:59:18 UTC", description: "Bump anyhow from 1.0.79 to 1.0.80", pr_number: 19914, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "282a58d410a05f2bf0def7cfcca98e84342134ff", date: "2024-02-22 07:22:44 UTC", description: "Update release instructions for deploying vector.dev", pr_number: 19925, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 4, deletions_count: 2},
		{sha: "a32895ec096c5c55c449c8d3ad6bed658d69b71b", date: "2024-02-22 15:48:57 UTC", description: "Bump the clap group with 3 updates", pr_number: 19899, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 33, deletions_count: 26},
		{sha: "4cd4b6a26de5f70a687b934df7193aa9ba2d46f7", date: "2024-02-22 15:49:05 UTC", description: "Bump serde_json from 1.0.113 to 1.0.114", pr_number: 19909, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c9e24003095f3a6271aa9a3d50c83c3b6f857014", date: "2024-02-22 15:49:08 UTC", description: "Bump syn from 2.0.49 to 2.0.50", pr_number: 19913, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 47, deletions_count: 47},
		{sha: "23ffe8812cd7df603cf3cf310773ee356c96c002", date: "2024-02-22 15:49:13 UTC", description: "Bump myrotvorets/set-commit-status-action from 2.0.0 to 2.0.1", pr_number: 19924, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 12, insertions_count: 29, deletions_count: 29},
		{sha: "a68a0b5c6a1ddd33682b578163727403dd9ef296", date: "2024-02-22 08:16:12 UTC", description: "Update CONTRIBUTING.md docs regarding how to have website…", pr_number: 19926, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 1},
		{sha: "3f59886a39321570e459ba65469d933a968876f2", date: "2024-02-23 03:21:04 UTC", description: "Add pre-requisite for vdev", pr_number: 19668, scopes: [], type: "docs", breaking_change: false, author: "Harold Dost", files_count: 1, insertions_count: 6, deletions_count: 0},
		{sha: "695f847d1711923261acdec0ad029185c7826521", date: "2024-02-23 02:52:44 UTC", description: "expose test utilities", pr_number: 19894, scopes: ["tests"], type: "chore", breaking_change: false, author: "Sebastian Tia", files_count: 4, insertions_count: 68, deletions_count: 35},
		{sha: "a6da1d8f4357513161520ae4c9fac96859d7de24", date: "2024-02-23 01:47:41 UTC", description: "add sink error path validation + multi config", pr_number: 18062, scopes: ["component validation"], type: "feat", breaking_change: false, author: "neuronull", files_count: 12, insertions_count: 277, deletions_count: 86},
		{sha: "bb1b8571070f38f7eee385dad92807249236d063", date: "2024-02-24 14:46:24 UTC", description: "Initial pulsar source", pr_number: 18475, scopes: ["sources"], type: "feat", breaking_change: false, author: "WarmSnowy", files_count: 12, insertions_count: 1328, deletions_count: 9},
		{sha: "5d03bf0e00b3f235cd2dfa9c88e77d7a162c0180", date: "2024-02-27 00:03:46 UTC", description: "Bump serde-wasm-bindgen from 0.6.3 to 0.6.4", pr_number: 19934, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "e2e5253ff42339f8c66226580a8aadf9b729e10d", date: "2024-02-27 00:04:11 UTC", description: "Bump the aws group with 6 updates", pr_number: 19936, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 20, deletions_count: 20},
		{sha: "ae5b06bff08d062216a1beab2f764b6b39b04b71", date: "2024-02-27 05:51:37 UTC", description: "Bump lru from 0.12.2 to 0.12.3", pr_number: 19945, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7bb9716ebc46bb2842e8df4b2c20775c1897d631", date: "2024-02-27 05:51:47 UTC", description: "Bump socket2 from 0.5.5 to 0.5.6", pr_number: 19947, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 12, deletions_count: 12},
		{sha: "fb11980b98b5ad3358124b5ecfb24d136c6f8903", date: "2024-02-27 05:52:03 UTC", description: "Bump cached from 0.48.1 to 0.49.2", pr_number: 19948, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "4634e2f167f47c6f9cfe0221cb7238b976f76091", date: "2024-02-27 08:06:08 UTC", description: "Bump openssl from 0.10.63 to 0.10.64", pr_number: 19906, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 7, deletions_count: 7},
		{sha: "070e38c555d7a7aaf9dda67e7dd468cfbfb949b9", date: "2024-02-27 09:09:23 UTC", description: "add support for EDNS EDE fields", pr_number: 19937, scopes: ["dnsmsg_parser"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 10, insertions_count: 292, deletions_count: 6},
		{sha: "f33169d6aa7d130f8a6a47a7060eeb3c69e22e98", date: "2024-02-27 08:35:16 UTC", description: "PulsarErrorEvent only occurs for the source", pr_number: 19950, scopes: ["pulsar source"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 6, deletions_count: 1},
		{sha: "b9c4544d83c9c4042c49b4153cb94ba062f9dfdb", date: "2024-02-27 23:57:38 UTC", description: "Bump bstr from 1.9.0 to 1.9.1", pr_number: 19946, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 5, deletions_count: 5},
		{sha: "3091443aa82b31ba04ecd3727c1f6bb37a6abbb0", date: "2024-02-27 23:57:49 UTC", description: "Bump darling from 0.20.6 to 0.20.8", pr_number: 19949, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 13, deletions_count: 13},
		{sha: "5f43cde7aa6165e55091ec8372e301a03426a3e5", date: "2024-02-28 04:57:59 UTC", description: "Bump syn from 2.0.50 to 2.0.51", pr_number: 19953, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 47, deletions_count: 47},
		{sha: "565d93d35cca13c77e3105e6fa376761b23251d2", date: "2024-02-28 04:58:09 UTC", description: "Bump dyn-clone from 1.0.16 to 1.0.17", pr_number: 19954, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "906cd65bb315cf658cc6c8a597c93e34de228d74", date: "2024-02-28 04:58:27 UTC", description: "Bump typetag from 0.2.15 to 0.2.16", pr_number: 19956, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "11f6491f77bd9fc98c3e19859d87aa036184a1d3", date: "2024-02-28 05:08:42 UTC", description: "Bump actions/add-to-project from 0.5.0 to 0.6.0", pr_number: 19960, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "cae37e99d8dba79c943e9cdf6af862523141f71c", date: "2024-02-28 05:08:58 UTC", description: "Bump docker/setup-buildx-action from 3.0.0 to 3.1.0", pr_number: 19961, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "b1a2ca11c156aa9f66125c56009e7f05bbe65d2f", date: "2024-02-28 23:33:08 UTC", description: "Bump tempfile from 3.10.0 to 3.10.1", pr_number: 19955, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "26ec8f432394b966e5c48da97634738f30c949d7", date: "2024-02-28 23:33:19 UTC", description: "Bump the aws group with 1 update", pr_number: 19965, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c1d6529225b3c9dd1c3e00957361acab89fa4d50", date: "2024-02-29 04:33:31 UTC", description: "Bump serde-wasm-bindgen from 0.6.4 to 0.6.5", pr_number: 19966, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "d4cf2bf6989eee92a41e7312b63b8522fdb0444b", date: "2024-02-29 05:07:02 UTC", description: "Bump rumqttc from 0.23.0 to 0.24.0", pr_number: 19967, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 103, deletions_count: 36},
		{sha: "43a91293c61e67305ee175e3cf135adeec0b51b1", date: "2024-02-29 04:15:48 UTC", description: "robustly synchronize component validation framework tasks", pr_number: 19927, scopes: ["observability"], type: "chore", breaking_change: false, author: "neuronull", files_count: 11, insertions_count: 246, deletions_count: 218},
		{sha: "c71d5d16493f1662187ed6e7a11c8a88fbc4e133", date: "2024-03-01 03:22:55 UTC", description: "expose component validation framework", pr_number: 19964, scopes: ["testing"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 162, deletions_count: 165},
		{sha: "44150403903915f0fa8b31e8fd20b2d8cb33b480", date: "2024-03-01 03:28:46 UTC", description: "add component validation", pr_number: 19932, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "9acc151516e8db9b8798eb80b10cee8f843b6da7", date: "2024-03-02 05:35:45 UTC", description: "Bump log from 0.4.20 to 0.4.21", pr_number: 19977, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "29a9167c8554befaa5a56a188b3c44e18d08c638", date: "2024-03-02 05:35:55 UTC", description: "Bump syn from 2.0.51 to 2.0.52", pr_number: 19979, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 47, deletions_count: 47},
		{sha: "6ef50922b302519518937008b99cba9f97a7283c", date: "2024-03-02 05:36:04 UTC", description: "Bump mlua from 0.9.5 to 0.9.6", pr_number: 19985, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "69e84b335edef665264aab16a5895c3877b99b5e", date: "2024-03-02 05:36:15 UTC", description: "Bump confy from 0.6.0 to 0.6.1", pr_number: 19986, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e2d8ad468ba7fa96598cf8cd3cc80641861d8b30", date: "2024-03-02 05:36:24 UTC", description: "Bump indexmap from 2.2.3 to 2.2.5", pr_number: 19987, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 23, deletions_count: 23},
		{sha: "4677102f189dfb9f3f63ea2f03ad4008fa01b30e", date: "2024-03-04 23:05:42 UTC", description: "Bump opendal from 0.45.0 to 0.45.1", pr_number: 19996, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 14},
		{sha: "02bb9b2e7eda2326f4da9d6500c76f1b6e812b28", date: "2024-03-04 23:05:57 UTC", description: "Bump arc-swap from 1.6.0 to 1.7.0", pr_number: 19997, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "8ca10a0232889fc8195911409d78469e50e76e12", date: "2024-03-05 07:07:36 UTC", description: "Bump the aws group with 3 updates", pr_number: 19976, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 9, deletions_count: 9},
		{sha: "312056c39178c3f40369d3aeefaf059dc9611626", date: "2024-03-05 07:42:16 UTC", description: "Bump bollard from 0.15.0 to 0.16.0", pr_number: 19998, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 153, deletions_count: 50},
		{sha: "c7e4e33ca0c479cd9c8b0c5af72f6bc804d287fe", date: "2024-03-05 02:19:24 UTC", description: "extend component validation framework for more flexible test case building", pr_number: 19941, scopes: ["observability"], type: "chore", breaking_change: false, author: "neuronull", files_count: 9, insertions_count: 117, deletions_count: 28},
		{sha: "676318aa258e9b211fd6bd8330eb900788f0473f", date: "2024-03-05 03:08:25 UTC", description: "expose deduping logic", pr_number: 19992, scopes: ["dedupe transform"], type: "chore", breaking_change: false, author: "neuronull", files_count: 6, insertions_count: 270, deletions_count: 243},
		{sha: "f34738e6737e79f77dc6aa9aecb8d00430f64d99", date: "2024-03-05 03:48:15 UTC", description: "increase timeout for `cross` workflow", pr_number: 20002, scopes: ["ci"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "fa99d6c2cdc6457d6f70f00dccf8e03d57ffce3a", date: "2024-03-06 08:31:54 UTC", description: "don't remove timestamp for `raw` endpoint", pr_number: 19975, scopes: ["splunk_hec_logs sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 8, insertions_count: 120, deletions_count: 50},
		{sha: "3b6066d9f93e753c0c4989173eaced46b1d2c519", date: "2024-03-07 00:47:55 UTC", description: "Remove optionality from topology controller reload", pr_number: 20010, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 25, deletions_count: 26},
		{sha: "d75f74cd9f28621f676e5c93aefbdccd279662af", date: "2024-03-07 06:54:08 UTC", description: "Bump cargo_toml from 0.19.1 to 0.19.2", pr_number: 20007, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c1141f9288007ec79c140d551a5ddfef483c40c5", date: "2024-03-07 06:54:33 UTC", description: "Bump wasm-bindgen from 0.2.91 to 0.2.92", pr_number: 20009, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 10, deletions_count: 10},
		{sha: "cbebdb2689600b8515dc34430703c8281cf7caa0", date: "2024-03-07 06:54:43 UTC", description: "Bump pin-project from 1.1.4 to 1.1.5", pr_number: 20015, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "8db6288b4cc2ecf070649e0dc53879f267f41c32", date: "2024-03-07 01:02:25 UTC", description: "calculate `EstimatedJsonSizeOf` for `component_received_event_bytes_total` before enrichment", pr_number: 19942, scopes: ["splunk_hec source"], type: "fix", breaking_change: false, author: "neuronull", files_count: 4, insertions_count: 137, deletions_count: 44},
		{sha: "eb3099657f53c8de5584b20fbb68f05c342f93c7", date: "2024-03-07 01:13:03 UTC", description: "Use gzip compression for datadog_logs regression tests", pr_number: 20020, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 0},
		{sha: "ea377f007e0657d65915f90b46e602ad6a149708", date: "2024-03-07 03:54:40 UTC", description: "add component spec validation tests for `datadog_logs` sink", pr_number: 19887, scopes: ["observability"], type: "chore", breaking_change: false, author: "neuronull", files_count: 6, insertions_count: 74, deletions_count: 16},
		{sha: "44ed0d146e274c9593db17f8e9fe74de3833e58f", date: "2024-03-07 05:40:46 UTC", description: "caller resolves the component validation framework test case path", pr_number: 20021, scopes: ["tests"], type: "chore", breaking_change: false, author: "neuronull", files_count: 1, insertions_count: 4, deletions_count: 2},
		{sha: "0f472db2b153566df47caec0c50b2f26ba0a2197", date: "2024-03-07 07:53:51 UTC", description: "Add missing `TraceEvent::remove` function", pr_number: 20023, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "a3bedbd70b6b297e3d7cf9868a7c82f87a86d548", date: "2024-03-07 07:05:05 UTC", description: "only compile ValidatableComponent in test runs", pr_number: 20024, scopes: ["testing"], type: "chore", breaking_change: false, author: "neuronull", files_count: 8, insertions_count: 273, deletions_count: 269},
		{sha: "eb690d4343e74078e4debd9f9984bcf0e89ad8a5", date: "2024-03-08 04:26:37 UTC", description: "add TCP mode to DNSTAP source", pr_number: 19892, scopes: ["sources"], type: "feat", breaking_change: true, author: "Ensar Sarajčić", files_count: 26, insertions_count: 1658, deletions_count: 299},
		{sha: "482ed3cb7a9de9763d7e623c8a691ac4d9911638", date: "2024-03-08 08:41:19 UTC", description: "add support for more record types (HINFO, CSYNC, OPT, missing DNSSEC types)", pr_number: 19921, scopes: ["dnsmsg_parser"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 4, insertions_count: 186, deletions_count: 53},
		{sha: "d505045620cc5272be54b42fdd01abb8c0486d50", date: "2024-03-08 05:56:54 UTC", description: "Update statsd doc to mention timing conversion", pr_number: 20033, scopes: ["statsd source"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 7, deletions_count: 0},
		{sha: "d5c8a77b5751c4d2277cee6ee76a1903873c5873", date: "2024-03-08 08:32:11 UTC", description: "add `parse_ddtags` config setting to parse the `ddtags` log event field into an object", pr_number: 20003, scopes: ["datadog_agent source"], type: "enhancement", breaking_change: false, author: "neuronull", files_count: 5, insertions_count: 185, deletions_count: 2},
		{sha: "485dea71725511b997586698650e202add499183", date: "2024-03-09 09:17:23 UTC", description: "add `lowercase_hostnames` option to `dnstap` source", pr_number: 20035, scopes: ["sources"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 5, insertions_count: 495, deletions_count: 299},
		{sha: "55a962a3c55d7b9437ec6b4b36ca42172bc9b953", date: "2024-03-09 02:26:04 UTC", description: "Update VRL to v0.12.0", pr_number: 20037, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 17, deletions_count: 10},
		{sha: "c83e36dd447ef9a4ebe8270bc295743ca3053bb6", date: "2024-03-09 10:58:13 UTC", description: "Bump the clap group with 1 update", pr_number: 20026, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 15, deletions_count: 15},
		{sha: "bd2f0a33e75e624bb75cb2c311bcbfa620ab699a", date: "2024-03-09 12:17:13 UTC", description: "integrate Cargo package dependency info", pr_number: 19933, scopes: ["website"], type: "feat", breaking_change: false, author: "Hugo Hromic", files_count: 7, insertions_count: 30, deletions_count: 18},
		{sha: "37a19fab442b06be3dc73c6962578e2f083f9d88", date: "2024-03-09 03:54:22 UTC", description: "Remove mention of handwriting changelog for patch release", pr_number: 20040, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 3},
		{sha: "56f167629049f879429506ce34321b534cfd79da", date: "2024-03-11 22:58:23 UTC", description: "Bump base64 from 0.21.7 to 0.22.0", pr_number: 19999, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 5, deletions_count: 5},
		{sha: "04f78584d7dd10e98d81e3065fbb17483009d60f", date: "2024-03-12 07:31:33 UTC", description: "fix tests for currently unknown rdata types", pr_number: 20052, scopes: ["dnstap source"], type: "chore", breaking_change: false, author: "Ensar Sarajčić", files_count: 1, insertions_count: 17, deletions_count: 5},
		{sha: "6d0961347b7c36115da101ab993f66a532493a16", date: "2024-03-12 07:55:07 UTC", description: "add docs for new validate flag in punycode functions", pr_number: 19923, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 2, insertions_count: 28, deletions_count: 0},
		{sha: "cbcb874a9944801e8a89d42e44ecf551db55071a", date: "2024-03-12 14:59:16 UTC", description: "improve tls support for greptimedb sink", pr_number: 20006, scopes: ["greptimedb sink"], type: "feat", breaking_change: false, author: "Ning Sun", files_count: 5, insertions_count: 41, deletions_count: 57},
		{sha: "d2aca62f1edcedd76bb818dc936a54b0928b0786", date: "2024-03-12 02:11:01 UTC", description: "Use `component_kind` rather than `kind` for Hugo", pr_number: 20058, scopes: ["docs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 110, insertions_count: 112, deletions_count: 112},
		{sha: "b35eaf53315532a7668cd36342f72af2d4e00488", date: "2024-03-12 02:56:13 UTC", description: "Regenerate k8s manifests for Helm chart v0.31.1", pr_number: 20060, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 18, insertions_count: 25, deletions_count: 23},
		{sha: "e9815e1f328a4ef59099c3d07918f167947c2e1f", date: "2024-03-12 11:06:13 UTC", description: "Add ARMv6 builds", pr_number: 19192, scopes: ["platforms"], type: "feat", breaking_change: false, author: "William Taylor", files_count: 12, insertions_count: 195, deletions_count: 3},
		{sha: "38acf37f1d5d33f46af93f24034475e450f04b29", date: "2024-03-12 04:06:24 UTC", description: "Update banner to use past tense for repository decommissioning", pr_number: 20059, scopes: ["docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4804e1745170dab2075fe6ef27534d57033ec2f7", date: "2024-03-12 05:30:26 UTC", description: "Use `component_kind` rather than `kind` in templates", pr_number: 20063, scopes: ["docs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "f0d3037541b99bfcebfabdb1796200992f0747a8", date: "2024-03-12 22:15:19 UTC", description: "Default env vars for enterprise_http_to_http regression case", pr_number: 20073, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "a7c3dbc453dc63dd4499b8f0c3dce15f16839f46", date: "2024-03-12 23:45:52 UTC", description: "Update default for --strict-env-vars to true", pr_number: 20062, scopes: ["cli"], type: "chore", breaking_change: true, author: "Jesse Szwedko", files_count: 4, insertions_count: 27, deletions_count: 10},
		{sha: "6a6c159da14b441df6dde0a3a9997a787910087a", date: "2024-03-13 04:23:34 UTC", description: "Update changelog generation script to handle authors and whitespace", pr_number: 20075, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 8, deletions_count: 8},
		{sha: "52d72dae521be48260c82a5e9fdb9ef81629e24c", date: "2024-03-13 12:09:53 UTC", description: "Bump docker/build-push-action from 5.1.0 to 5.2.0", pr_number: 20057, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "bcc6e40862ee16f4cec75b8f752c54a399bd6cbc", date: "2024-03-13 12:10:06 UTC", description: "Bump toml from 0.8.10 to 0.8.11", pr_number: 20067, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 18, deletions_count: 9},
		{sha: "98df316fedbdffcf475b3ca9c51ab5ad4bdaa1ae", date: "2024-03-13 12:10:19 UTC", description: "Bump serde_with from 3.6.1 to 3.7.0", pr_number: 20068, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 14, deletions_count: 14},
		{sha: "34d3aa5b23b859d0e9e0c566c2ae3ec5bf79ceca", date: "2024-03-13 12:10:32 UTC", description: "Bump thiserror from 1.0.57 to 1.0.58", pr_number: 20069, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "8811e218d9d691d0d5e600d0cd2cd50cacb02c0a", date: "2024-03-13 12:10:42 UTC", description: "Bump proc-macro2 from 1.0.78 to 1.0.79", pr_number: 20070, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 71, deletions_count: 71},
		{sha: "fe23c97ae6a45115c9924a3ea6410c62018c5060", date: "2024-03-13 16:38:04 UTC", description: "Bump anyhow from 1.0.80 to 1.0.81", pr_number: 20066, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "aa04ac86707ee0f1df8e7b77acbd459834ca1fa4", date: "2024-03-14 04:45:15 UTC", description: "add `permit_origin` config option for all tcp sources", pr_number: 20051, scopes: ["sources"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 16, insertions_count: 85, deletions_count: 29},
		{sha: "0ec279d2a1b6a113f6e62d1f755a29a371862307", date: "2024-03-13 20:48:07 UTC", description: "Bump bufbuild/buf-setup-action from 1.29.0 to 1.30.0", pr_number: 20056, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "de4687ff51eda7c67a66ebe86138ab9ad7ceb54c", date: "2024-03-14 03:48:18 UTC", description: "Bump the aws group with 4 updates", pr_number: 20079, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 14, deletions_count: 15},
		{sha: "c62ec39ab159b964ec0069db5b528f0954a66c43", date: "2024-03-14 03:48:28 UTC", description: "Bump reqwest from 0.11.24 to 0.11.26", pr_number: 20080, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "62de4218e00a9907bc3c79b9e36c01066b772bb5", date: "2024-03-14 03:48:38 UTC", description: "Bump serde-toml-merge from 0.3.4 to 0.3.5", pr_number: 20081, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "d23730e3138c20fac276178357234135f1fc52bd", date: "2024-03-14 03:48:52 UTC", description: "Bump os_info from 3.7.0 to 3.8.0", pr_number: 20082, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "ebdc64dbfc0ac71a1ff73ab9080849eca718a442", date: "2024-03-14 00:12:04 UTC", description: "Readd error log for elasticsearch sink", pr_number: 19846, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 19, deletions_count: 0},
		{sha: "f7380e45e4e1af63dd1bb3ecefac50ff45376a3c", date: "2024-03-14 02:29:15 UTC", description: "Set ignored_header_bytes default to `0`", pr_number: 20076, scopes: ["file source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 6, deletions_count: 0},
		{sha: "ccaa7e376d0167d187573c4b9b478f1c2778e359", date: "2024-03-14 05:18:39 UTC", description: "Update CODEOWNERS to reflect consolidation", pr_number: 20087, scopes: ["dev"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 98, deletions_count: 98},
		{sha: "d511e893ad0e594231e06f25a9d35ab70248bedc", date: "2024-03-15 05:41:54 UTC", description: "add support for custom MMDB types", pr_number: 20054, scopes: ["enrichment_tables"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 9, insertions_count: 436, deletions_count: 15},
		{sha: "4671ccbf0a6359ef8b752fa99fae9eb9c60fdee5", date: "2024-03-14 22:28:24 UTC", description: "Use correct how_it_works section for Vector sink", pr_number: 20095, scopes: ["docs"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "fafe8c50a4721fa3ddbea34e0641d3c145f14388", date: "2024-03-15 16:38:43 UTC", description: "remove repetitive words", pr_number: 20091, scopes: [], type: "chore", breaking_change: false, author: "teslaedison", files_count: 8, insertions_count: 9, deletions_count: 9},
		{sha: "0be97cdae0d97d9ccd9fb2e14501c9dd82fb6e10", date: "2024-03-16 05:28:19 UTC", description: "relax required input semantic meanings", pr_number: 20086, scopes: ["datadog_logs sink"], type: "fix", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 82, deletions_count: 16},
		{sha: "ad8a8690b7707540dd24a85e8ada8c51bab150fe", date: "2024-03-16 08:02:19 UTC", description: "Bump tokio-test from 0.4.3 to 0.4.4", pr_number: 20101, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "88606447dd9f874f27f06dc17c3e2f0b2083e221", date: "2024-03-18 22:48:54 UTC", description: "Bump the aws group with 1 update", pr_number: 20089, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 3},
		{sha: "494d7e2a7bff5c7bebb90925b5f451a99e3f0d5c", date: "2024-03-18 22:49:07 UTC", description: "Bump docker/setup-buildx-action from 3.1.0 to 3.2.0", pr_number: 20097, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "cb4a5e6257508534295dc79c8af2768c7e74284d", date: "2024-03-19 02:49:18 UTC", description: "Bump docker/build-push-action from 5.2.0 to 5.3.0", pr_number: 20098, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8737b24807ee6b00a20663f951ec0ce53682530e", date: "2024-03-19 02:51:47 UTC", description: "Bump syn from 2.0.52 to 2.0.53", pr_number: 20111, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 47, deletions_count: 47},
		{sha: "7e3e60fa447eab3b73f27e2c98ed1f2c4d19fe94", date: "2024-03-19 02:51:57 UTC", description: "Bump os_info from 3.8.0 to 3.8.1", pr_number: 20112, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "068b19918fd723e26b9fc5c6de289493d9ad55de", date: "2024-03-19 02:52:13 UTC", description: "Bump async-recursion from 1.0.5 to 1.1.0", pr_number: 20114, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a1902c2897c23e40d18dc96df333461c0f65ef4a", date: "2024-03-19 02:52:23 UTC", description: "Bump async-trait from 0.1.77 to 0.1.78", pr_number: 20115, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "3e8c6a48451233fb7b60b4ca0a5139986745f80e", date: "2024-03-19 02:52:32 UTC", description: "Bump serde_yaml from 0.9.32 to 0.9.33", pr_number: 20116, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 12, deletions_count: 12},
		{sha: "5c33628279443068365616783b6a2d5466e8a548", date: "2024-03-19 02:52:46 UTC", description: "Bump mongodb from 2.8.1 to 2.8.2", pr_number: 20117, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7c9b4c59c06a49c46e1f0f84faa6114dcce5c642", date: "2024-03-19 03:26:17 UTC", description: "Bump the clap group with 1 update", pr_number: 20108, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 23, deletions_count: 16},
		{sha: "4c7becebe8ec38f2a60d25a97bafa3d6c9a12fd7", date: "2024-03-19 05:16:11 UTC", description: "Bump tokio-stream from 0.1.14 to 0.1.15", pr_number: 20100, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "5e7248cfaa787126cb7654e0523d6ced8c06f245", date: "2024-03-19 05:24:16 UTC", description: "do not filter out file contents from error logs", pr_number: 20125, scopes: ["remap"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 6, deletions_count: 42},
		{sha: "12c1866214e55869275afa5fc0741f2af8baa0fd", date: "2024-03-19 05:01:37 UTC", description: "further adjustments to component validation framework", pr_number: 20043, scopes: ["testing"], type: "chore", breaking_change: false, author: "neuronull", files_count: 3, insertions_count: 56, deletions_count: 5},
		{sha: "80f63bb6b52561ae4a9f98783ae98472c0798845", date: "2024-03-19 11:24:11 UTC", description: "Bump the graphql group with 2 updates", pr_number: 20107, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 16, deletions_count: 19},
		{sha: "ad6a48efc0f79b2c18a5c1394e5d8603fdfd1bab", date: "2024-03-19 07:04:35 UTC", description: "bugs in internal component metric reporting", pr_number: 20044, scopes: ["datadog_agent source"], type: "fix", breaking_change: false, author: "neuronull", files_count: 7, insertions_count: 155, deletions_count: 6},
		{sha: "62297dcb8caba651ed60f154c36b5a4e1a63046b", date: "2024-03-20 00:28:08 UTC", description: "Bump VRL to v0.13.0", pr_number: 20126, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 13, deletions_count: 3},
		{sha: "58a4a2ef52e606c0f9b9fa975cf114b661300584", date: "2024-03-20 00:52:12 UTC", description: "Move host_metrics feature gate", pr_number: 20134, scopes: ["api"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "b184196d9760539db31a5238ee7b7254329b7c8d", date: "2024-03-21 02:44:33 UTC", description: "Bump uuid from 1.7.0 to 1.8.0", pr_number: 20131, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "2a88fc06b7c958f9787a3e050c677cbe5860d62d", date: "2024-03-21 02:44:43 UTC", description: "Bump the aws group with 2 updates", pr_number: 20129, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "04bff918cfcba087c18766ef81a8e2316b8790f4", date: "2024-03-22 05:30:21 UTC", description: "Bump smallvec from 1.13.1 to 1.13.2", pr_number: 20145, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "db9c681fd99234f6cd4799185bace2f351e0712d", date: "2024-03-22 09:31:23 UTC", description: "Bump actions/add-to-project from 0.6.0 to 0.6.1", pr_number: 20137, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e012a80bb5d8e4f318fb4408d9e2ab6242a8883b", date: "2024-03-22 09:31:26 UTC", description: "Bump serde-toml-merge from 0.3.5 to 0.3.6", pr_number: 20132, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "20e56d3080ec3cb04c750966c2722799ed920225", date: "2024-03-22 09:31:37 UTC", description: "Bump toml from 0.8.11 to 0.8.12", pr_number: 20130, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "3f83ea32e06c8e3575e6b82bdf8e25a7eb97dcc0", date: "2024-03-22 09:31:48 UTC", description: "Bump h2 from 0.4.2 to 0.4.3", pr_number: 20110, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "4c68f9699749d17fa926983e2a90bdeec92b112a", date: "2024-03-22 11:05:32 UTC", description: "add documentation for `sieve` function", pr_number: 20000, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 1, insertions_count: 65, deletions_count: 0},
		{sha: "abd776d7c74ae48968fa34829d3683f68115a9e0", date: "2024-03-22 22:48:19 UTC", description: "Bump Rust to 1.77.0", pr_number: 20149, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 14, insertions_count: 40, deletions_count: 42},
		{sha: "314ea367302fb95a3ec0c2fcdfbe19df6a0e7603", date: "2024-03-23 10:54:00 UTC", description: "add `uuid_v7` function", pr_number: 20048, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Philipp Paulweber", files_count: 4, insertions_count: 72, deletions_count: 0},
		{sha: "4d23e66dc22c499ad8263b937c21800d1b68d1c7", date: "2024-03-23 06:45:38 UTC", description: "Update TLS docs for `verify_certificate`", pr_number: 20153, scopes: ["security"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 68, insertions_count: 210, deletions_count: 210},
		{sha: "4279bf0018055de68f59dffe9532fab96c80d3ac", date: "2024-03-26 06:35:53 UTC", description: "Add documentation for parse_proto and encode_proto", pr_number: 20139, scopes: ["vrl"], type: "docs", breaking_change: false, author: "Flávio Cruz", files_count: 5, insertions_count: 121, deletions_count: 0},
	]
}
