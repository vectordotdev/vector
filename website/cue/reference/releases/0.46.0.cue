package metadata

releases: "0.46.0": {
	date:     "2025-04-04"
	codename: ""

	whats_next: []

	known_issues: [
		"""
			Several AWS integrations such as `aws_kinesis_firehose` sink, `aws_kinesis_streams` sink and `aws_s3` source stopped working.
			This was reported in [issue 22840](https://github.com/vectordotdev/vector/issues/22840).
			""",
	]

	description: """
		The Vector team is pleased to announce version `0.46.0`!

		Release highlights:

		- A new `postgres` sink is now available and it supports logs, metrics and traces!
		  - Thanks to [jorgehermo9](https://github.com/jorgehermo9) for this sizable contribution.
		- `vector top` now supports filtering out components by their component ID.
		  - Again thanks to [jorgehermo9](https://github.com/jorgehermo9).
		- A new global option `expire_metrics_per_metric_set` is now available and it enables more fine-grained control over individual metric sets.
		  - Thanks to [esensar](https://github.com/esensar) and [Quad9DNS](https://github.com/Quad9DNS).
		"""

	vrl_changelog: """
		VRL was updated to v0.23.0. This includes the following changes:

		#### Breaking Changes

		- The `ip_cidr_contains` function now validates the cidr argument during the compilation phase if it is a constant string or array. Previously, invalid constant CIDR values would only trigger an error during execution.

		Previously, if an invalid CIDR was passed as a constant, an error was thrown at runtime:

		```text
		error[E000]: function call error for "ip_cidr_contains" at (0:45): unable to parse CIDR: couldn't parse address in network: invalid IP address syntax
		┌─ :1:1
		│
		1 │ ip_cidr_contains!("INVALID", "192.168.10.32")
		│ ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ unable to parse CIDR: couldn't parse address in network: invalid IP address syntax
		│
		= see language documentation at https://vrl.dev
		= try your code in the VRL REPL, learn more at https://vrl.dev/examples
		```

		Now, we see a compilation error:

		```text
		error[E610]: function compilation error: error[E403] invalid argument
		┌─ :1:1
		│
		1 │ ip_cidr_contains!("INVALID", "192.168.10.32")
		│ ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
		│ │
		│ invalid argument "ip_cidr_contains"
		│ error: "cidr" must be valid cidr
		│ received: "INVALID"
		│
		= learn more about error code 403 at https://errors.vrl.dev/403
		= see language documentation at https://vrl.dev
		= try your code in the VRL REPL, learn more at https://vrl.dev/examples
		```

		This change improves error detection by identifying invalid CIDR values earlier, reducing unexpected failures at runtime and provides better performance.

		#### New Features

		- Support for encoding and decoding lz4 block compression with the `encode_lz4` and `decode_lz4` functions.

		#### Enhancements

		- The `encode_proto` function was enhanced to automatically convert integer, float, and boolean values when passed to string proto fields. (https://github.com/vectordotdev/vrl/pull/1304)
		- The `parse_user_agent` method now uses the [ua-parser](https://crates.io/crates/ua-parser) library
		which is much faster than the previous library. The method's output remains unchanged.
		- Added support for excluded_boundaries in the `snakecase()` function. This allows users to leverage the same function `snakecase()` that they're already leveraging but tune it to handle specific scenarios where default boundaries are not desired.

		For example,

		```rust
		snakecase("s3BucketDetails", excluded_boundaries: ["digit_lower", "lower_digit", "upper_digit"])
		// Output: s3_bucket_details
		```

		#### Fixes

		- The `parse_nginx_log` function can now parse `delaying requests` error messages.
		"""

	changelog: [
		{
			type: "feat"
			description: """
				`vector top` now supports filtering out components by their component ID using glob patterns with a new `--components` option.
				This is very similar to `vector tap` `--outputs-of` and `--inputs-of` options. This can be useful
				in cases where you have a lot of components and they don't fit in the terminal (as scrolling is not supported yet in `vector top`).
				By default, all components are shown with a glob pattern of `*`.

				The glob pattern semantics can be found in the [`glob` crate documentation](https://docs.rs/glob/latest/glob/).

				Example usage: `vector top --components "demo*"` will only show the components that match the glob pattern `demo*`.
				"""
			contributors: ["jorgehermo9"]
		},
		{
			type: "feat"
			description: """
				Add a new `postgres` sink which allows to send log, metric and trace events to a postgres database.
				"""
			contributors: ["jorgehermo9"]
		},
		{
			type: "fix"
			description: """
				Prevent overflow when calculating the next refresh interval for Google Cloud Storage tokens.
				"""
			contributors: ["graphcareful"]
		},
		{
			type: "enhancement"
			description: """
				The TLS `crt_file` and `key_file` from `http` sinks are now watched when `--watch_config` is enabled and therefore changes to those files will trigger a config reload without the need to restart Vector.
				"""
			contributors: ["gllb"]
		},
		{
			type: "feat"
			description: """
				Added a new optional global option `expire_metrics_per_metric_set`, enabling configuration of metrics expiration, similar to `expire_metrics_secs`, but enables defining different values per metric set, defined with a name and/or set of labels. `expire_metrics_secs` is used as a global default for sets not matched by this.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: """
				Add the ability to run `memory` `enrichment_table` as a source, periodically dumping all the stored
				data and optionally removing it from the table.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: """
				Add message buffering feature to `websocket_server` sink, that enables replaying missed messages to
				newly connected clients.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "feat"
			description: """
				`websocket_server` server sink component can now have customizable additional tags on metrics that
				it generates. They can hold fixed values, headers, client IPs and more.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: """
				Prevent panic when an enrichment table has the same name as one of components.
				"""
			contributors: ["esensar", "Quad9DNS"]
		},
		{
			type: "fix"
			description: """
				Fix an issue where using `length_delimited` encoder in `framing.method`, the last or only message is not correctly framed.
				"""
			contributors: ["callum-ryan"]
		},
		{
			type: "fix"
			description: """
				Fixed a bug in the `aws_kinesis_firehose` source, where the `store_access_key` option did not correctly store the access key.
				"""
			contributors: ["noibu-gregory"]
		},
		{
			type: "fix"
			description: """
				Fixed a bug in the `aws_s3` sink, where the `endpoint_url` field in AWS_CONFIG_FILE was not respected by Vector.
				"""
			contributors: ["ganelo"]
		},
		{
			type: "fix"
			description: """
				Fixes a `vector top` bug was introduced in version 0.45 which prevented connections from being established.
				"""
			contributors: ["JakubOnderka"]
		},
		{
			type: "enhancement"
			description: """
				Add support for static labels in `gcp_stackdriver_logs` sink.
				This enhancement enables users to define static labels directly in the
				gcp_stackdriver_logs sink configuration. Static labels are key-value pairs
				that are consistently applied to all log entries sent to Google Cloud Logging,
				improving log organization and filtering capabilities.
				"""
			contributors: ["stackempty"]
		},
		{
			type: "enhancement"
			description: """
				Add support for dynamic labels in `gcp_stackdriver_logs` sink via `labels_key`.
				This enhancement allows Vector to automatically map fields from structured
				log entries to Google Cloud LogEntry labels. When a structured log contains
				fields matching the configured `labels_key`, Vector will populate the
				corresponding labels in the Google Cloud LogEntry, enabling better log
				organization and filtering in Google Cloud Logging.
				"""
			contributors: ["stackempty"]
		},
		{
			type: "fix"
			description: """
				Fix a bug in the `host_metrics` source which caused the `process_cpu_usage` metric to always stay 0.
				"""
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: """
				Fix potential panic in the `host_metrics` source when collecting TCP metrics.
				"""
			contributors: ["pront"]
		},
		{
			type: "feat"
			description: """
				Adds a `force_path_style` option to the `aws_s3` source, matching support added in the `aws_s3` sink previously, that allows users to configure usage of virtual host-style bucket addressing. The value defaults to `true` to maintain existing (path-based addressing) behavior.
				"""
			contributors: ["sbalmos"]
		},
		{
			type: "feat"
			description: """
				The sample transform now accepts a ratio via a new `ratio` configuration parameter.
				This allow expressing the rate of forwarded events as a percentage.
				"""
			contributors: ["graphcareful"]
		},
	]

	commits: [
		{sha: "92f09ee495e57bb9ee82813abc043fd97e5de414", date: "2025-02-21 03:20:59 UTC", description: "automate release preperation steps (part 1)", pr_number: 22485, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 6, insertions_count: 128, deletions_count: 33},
		{sha: "a0b6cadbb77905589663a7b05c1a8b6239a9000b", date: "2025-02-21 20:50:39 UTC", description: "introduce debugging guide", pr_number: 22417, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 7, insertions_count: 828, deletions_count: 1},
		{sha: "aadfa189a8899311e33dfc632ccffa9e2789fe4e", date: "2025-02-22 04:17:46 UTC", description: "Add `--components` option in `vector top` to filter out components", pr_number: 22392, scopes: ["cli"], type: "feat", breaking_change: false, author: "Jorge Hermo", files_count: 5, insertions_count: 181, deletions_count: 32},
		{sha: "8dce2dd6a68fdcb839a222d5600100fb8536fdeb", date: "2025-02-22 06:25:47 UTC", description: "update vector tap help", pr_number: 22490, scopes: ["external"], type: "docs", breaking_change: false, author: "Jorge Hermo", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "1519c3e5d56b3129db2060f5c88b4126cbbaaa0e", date: "2025-02-24 16:31:03 UTC", description: "Mark RUSTSEC-2025-0007 as ignored", pr_number: 22493, scopes: ["ci"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "e30227456bc2f09bb9537a3c8fc7e086da3aa4ef", date: "2025-02-25 02:53:22 UTC", description: "Bump the patches group across 1 directory with 15 updates", pr_number: 22497, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 10, insertions_count: 63, deletions_count: 62},
		{sha: "9d59440ba191987ab884360675ee296a1dda335b", date: "2025-02-24 21:56:37 UTC", description: "update publish-homebrew token", pr_number: 22502, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4854f060ce1c80f703f2061dd8a1a09ee3c3275b", date: "2025-02-25 02:07:23 UTC", description: "cargo vdev build manifests", pr_number: 22505, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 18, insertions_count: 22, deletions_count: 22},
		{sha: "6db059e39ddd4a76f31f7d374138ca0d358c7cb7", date: "2025-02-25 08:00:26 UTC", description: "Bump rand from 0.8.5 to 0.9.0", pr_number: 22403, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 32, insertions_count: 91, deletions_count: 92},
		{sha: "8a55813266aa2b2ea64e96f6fc9d7e9a213a8991", date: "2025-02-25 20:35:59 UTC", description: "Complete v0.45.0 release", pr_number: 22506, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 31, insertions_count: 537, deletions_count: 86},
		{sha: "d19ac2a6226cb4220df9a6a70714a51c8dde4ff9", date: "2025-02-25 22:14:53 UTC", description: "Update ring", pr_number: 22511, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 16, deletions_count: 10},
		{sha: "88ac3e53f2435871e8d39e3dc245a8acbfe80852", date: "2025-02-26 21:23:45 UTC", description: "install released wasm-pack version 0.13.1", pr_number: 22526, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 6},
		{sha: "e99bf9e5bae9ba5d86f3503bb664688674be1f14", date: "2025-02-26 23:39:44 UTC", description: "tweaks to highlights blog post", pr_number: 22512, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 4, deletions_count: 1},
		{sha: "fe700dca4d4ecf0b7802e13e9292852ed5a254a4", date: "2025-02-27 00:45:02 UTC", description: "use single system instance", pr_number: 22513, scopes: ["host_metrics source"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 13, deletions_count: 7},
		{sha: "6caf5f8a2cbeeb99c0383e46d3a73ff43317c3f2", date: "2025-02-27 08:23:08 UTC", description: "prevent panic when attaching inputs to sinks with name of a table", pr_number: 22528, scopes: ["enriching"], type: "fix", breaking_change: false, author: "Ensar Sarajčić", files_count: 3, insertions_count: 10, deletions_count: 1},
		{sha: "e686aeaad592b46c07a55fe88c69f43e3b4f2a5f", date: "2025-02-27 03:42:01 UTC", description: "switch to Rust 1.85", pr_number: 22525, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 26, insertions_count: 70, deletions_count: 89},
		{sha: "e3653c38f6a2f66d9108d16814b68003aaa0631a", date: "2025-02-27 21:49:20 UTC", description: "Support vhost-style S3 bucket addressing", pr_number: 22475, scopes: ["aws_s3 source"], type: "feat", breaking_change: false, author: "Scott Balmos", files_count: 4, insertions_count: 26, deletions_count: 3},
		{sha: "add77d0876436f2ed7f25ad55b54c0e58add48d1", date: "2025-02-28 06:40:04 UTC", description: "add documentation for `to_syslog_facility_code` vrl function", pr_number: 22241, scopes: ["vrl"], type: "docs", breaking_change: false, author: "simplepad", files_count: 1, insertions_count: 32, deletions_count: 0},
		{sha: "b362704a4fb7bcaa90414c4e44c1b66c77e177a3", date: "2025-02-28 12:59:01 UTC", description: "Remove redundant loop in expand schema references", pr_number: 22508, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Huang Chen-Yi", files_count: 1, insertions_count: 44, deletions_count: 72},
		{sha: "8937349d551e4b59e5758fd97a649f37bb6c8721", date: "2025-02-28 06:56:01 UTC", description: "add message buffering for `websocket_server`", pr_number: 22479, scopes: ["websocket_server sink"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 7, insertions_count: 367, deletions_count: 7},
		{sha: "d49c542930267cc69d577e8d3b86a6c119fcf331", date: "2025-02-28 01:21:31 UTC", description: "`ubuntu-20.04` is deprecated - migrate to `ubuntu-24.04`", pr_number: 22527, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 24, insertions_count: 78, deletions_count: 68},
		{sha: "da674594f9fcc41b288c5ae2679fae1dc9e3ad74", date: "2025-02-28 02:31:54 UTC", description: "Update workflow from bash to gh scripts", pr_number: 22503, scopes: ["website"], type: "chore", breaking_change: false, author: "Devin Ford", files_count: 2, insertions_count: 129, deletions_count: 38},
		{sha: "baffdbfd8bda0ca3beae4fec38c35a748176ed29", date: "2025-03-01 00:55:18 UTC", description: "Support additional top level labels for gcp_stackdriver_logs sink", pr_number: 22473, scopes: ["gcp_stackdriver_logs sink"], type: "feat", breaking_change: false, author: "Damilola Akinsiku", files_count: 5, insertions_count: 161, deletions_count: 6},
		{sha: "3cf1eccc8dc5cf508a80fa47a251236862221f4c", date: "2025-03-04 00:49:30 UTC", description: "ignore flakey tcp metrics tests", pr_number: 22544, scopes: ["host_metrics source"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "e1dce44355c7dcf0b0d0880111a67f15b997b5d4", date: "2025-03-04 19:36:01 UTC", description: "`prepare.sh` should install the toolchain", pr_number: 22572, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 7, deletions_count: 2},
		{sha: "620ff9747c66a974b5145eb0d47561cf2b9ac463", date: "2025-03-05 01:49:54 UTC", description: "Bump docker/setup-qemu-action from 3.4.0 to 3.6.0", pr_number: 22578, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "4fdbe8e9dc5762388427abad39e7fc3c076c90db", date: "2025-03-05 01:54:15 UTC", description: "Bump docker/metadata-action from 5.6.1 to 5.7.0", pr_number: 22577, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "5ad8b89124a1a11b6d53d6cffa854f62c872cfaf", date: "2025-03-05 01:55:32 UTC", description: "Bump docker/build-push-action from 6.13.0 to 6.15.0", pr_number: 22575, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c6ee08ab33d1833fd0a982517d39af04f6d7944c", date: "2025-03-05 01:56:37 UTC", description: "Bump ossf/scorecard-action from 2.4.0 to 2.4.1", pr_number: 22574, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "e5f1b1d26a525283e8746a62aaf465141bfeaa9d", date: "2025-03-05 02:02:40 UTC", description: "Bump docker/setup-buildx-action from 3.9.0 to 3.10.0", pr_number: 22576, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "d39ca7386d98fbf914da6aa71406015bcfc38460", date: "2025-03-05 03:12:49 UTC", description: "add missing Quad9 credits in previous releases", pr_number: 22569, scopes: ["external"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 9, insertions_count: 19, deletions_count: 19},
		{sha: "30995ea87315acaf57b6f1dcfe8ffd1462d104b3", date: "2025-03-05 03:13:18 UTC", description: "add missing Quad9 credits in changelog entries", pr_number: 22568, scopes: ["external"], type: "docs", breaking_change: false, author: "Ensar Sarajčić", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "faab8586d7dad34035bae1f764f6e9882ec504eb", date: "2025-03-04 21:14:03 UTC", description: "delete unused script", pr_number: 22583, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 0, deletions_count: 13},
		{sha: "c3d0f080af57f7da950d335e0b16925decfcb28c", date: "2025-03-05 11:20:17 UTC", description: "Fix dead link on commit sub-categories", pr_number: 22520, scopes: ["internal docs"], type: "docs", breaking_change: false, author: "Shin Seunghun", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "09d795a4c64dc7f4537da738e9e01a960bffddd6", date: "2025-03-05 02:59:39 UTC", description: "Bump similar-asserts from 1.6.1 to 1.7.0", pr_number: 22562, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 10, deletions_count: 10},
		{sha: "a698327ccce69f13e638f73aa67d47233656f252", date: "2025-03-05 03:05:39 UTC", description: "Bump owo-colors from 4.1.0 to 4.2.0", pr_number: 22565, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "1dcc0df4e303994d7ff6ea686fc42040cd1224c9", date: "2025-03-05 03:39:27 UTC", description: "Bump bytesize from 1.3.2 to 2.0.1", pr_number: 22563, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 4, deletions_count: 4},
		{sha: "5b7c485e290a57142bca2925b9a42ad4e1c2c4dd", date: "2025-03-05 03:41:43 UTC", description: "cargo update -p vrl", pr_number: 22591, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 4, deletions_count: 99},
		{sha: "9fd77616b687c7a49472a6b95eecc26eae69fb7d", date: "2025-03-05 08:48:17 UTC", description: "Bump the patches group with 7 updates", pr_number: 22551, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 31, deletions_count: 25},
		{sha: "d0d770fbae5106511c1c88852599c82cdfcf32df", date: "2025-03-05 19:15:19 UTC", description: "Update permissions and revert id call", pr_number: 22580, scopes: ["website"], type: "fix", breaking_change: false, author: "Devin Ford", files_count: 2, insertions_count: 151, deletions_count: 125},
		{sha: "cba85ca47704033f6c0001d3f7f807074ab226a4", date: "2025-03-05 21:53:29 UTC", description: "install rust toolchain for macos nightly targets", pr_number: 22596, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 5, deletions_count: 1},
		{sha: "7650fa2c8c548e5ef5f22843b456b8297a60ee08", date: "2025-03-06 05:09:18 UTC", description: "`LengthDelimitedEncoder` fix last message framing ", pr_number: 22536, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Callum Ryan", files_count: 9, insertions_count: 197, deletions_count: 4},
		{sha: "207e5d47b48f532671d2858cd898674d0d30a517", date: "2025-03-06 00:17:03 UTC", description: "WEB-5803 | Remove netifly references from Privacy markdown", pr_number: 22598, scopes: ["website"], type: "chore", breaking_change: false, author: "Devin Ford", files_count: 1, insertions_count: 1, deletions_count: 3},
		{sha: "07f75629710746cb94a711401dd259b2eb5dc471", date: "2025-03-06 21:20:25 UTC", description: "Bump uuid from 1.12.0 to 1.15.1", pr_number: 22561, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 14, deletions_count: 21},
		{sha: "5e34f9b4f569a819072e73785830247caabd09ce", date: "2025-03-07 07:36:49 UTC", description: "Add postgres sink", pr_number: 21248, scopes: ["postgres sink"], type: "feat", breaking_change: false, author: "Jorge Hermo", files_count: 26, insertions_count: 1728, deletions_count: 46},
		{sha: "559d9f22dfe8cd2c67a16538ece86dd366566a29", date: "2025-03-07 18:54:44 UTC", description: "update smp to 0.21.0", pr_number: 22606, scopes: ["ci"], type: "chore", breaking_change: false, author: "George Hahn", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "d17c09948831363646dd9f6d79545cfe82481f11", date: "2025-03-07 23:29:16 UTC", description: "add defensive check to prevent panics", pr_number: 22604, scopes: ["host_metrics source"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 92, deletions_count: 32},
		{sha: "5d9c71a3c516f795b976d6d2ba4e8e0320bdd2b1", date: "2025-03-10 21:34:51 UTC", description: "Bump tempfile from 3.17.1 to 3.18.0", pr_number: 22623, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 26, deletions_count: 7},
		{sha: "b109e5c1220bba694ea1c599763296eaafb74798", date: "2025-03-10 21:37:12 UTC", description: "Bump aws-smithy-http from 0.60.12 to 0.61.1 in the aws group", pr_number: 22620, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 43, deletions_count: 22},
		{sha: "f4c6835a3032c810e005dc986a6c88849d3b30e0", date: "2025-03-10 17:49:20 UTC", description: "automate more pre-release steps", pr_number: 22614, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 201, deletions_count: 42},
		{sha: "055fde6967fd5d3be0d1ee4b413e4cf279399560", date: "2025-03-10 21:25:04 UTC", description: "Bump tokio from 1.43.0 to 1.44.0", pr_number: 22625, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 16, deletions_count: 16},
		{sha: "fbee7310b01697a2ce4fff12884d11ae81b473c0", date: "2025-03-11 04:27:08 UTC", description: "deny check fixes", pr_number: 22627, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 26, deletions_count: 28},
		{sha: "0a6c26c3c48030f8ec48f4fa4836abcc0ab8ce78", date: "2025-03-11 05:44:33 UTC", description: "Bump the patches group across 1 directory with 19 updates", pr_number: 22628, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 13, insertions_count: 385, deletions_count: 379},
		{sha: "0bc9c58b04a31d40e2f05407a438dcc3e4984779", date: "2025-03-11 10:30:19 UTC", description: "Bump rstest from 0.24.0 to 0.25.0", pr_number: 22564, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "e050e479b7189e9cc6460ef2678851110612f298", date: "2025-03-11 22:03:43 UTC", description: "Bump axios from 1.7.5 to 1.8.2 in /website in the npm_and_yarn group across 1 directory", pr_number: 22610, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "6fe2317d0b1bbc477fafd9ff80f39de55d36dffc", date: "2025-03-11 23:37:04 UTC", description: "add basic oci labels", pr_number: 22546, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Birger Johan Nordølum", files_count: 4, insertions_count: 21, deletions_count: 0},
		{sha: "15b28ed10b8fd58d59f50bd9f8368b1781edabdf", date: "2025-03-11 22:13:08 UTC", description: "add retries to k8s test job", pr_number: 22632, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 7, deletions_count: 1},
		{sha: "00852466425c736ec94b7135f57555a634d96894", date: "2025-03-12 02:57:40 UTC", description: "Bump the azure group with 4 updates", pr_number: 22621, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 151, deletions_count: 82},
		{sha: "7644658475ffb00358eb0b8658505541661e5944", date: "2025-03-12 20:00:25 UTC", description: "add new unmaintained dependency", pr_number: 22637, scopes: ["ci"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 9, deletions_count: 9},
		{sha: "2fa6f147ccbaa9b97553ecf708ce55edefabb8bb", date: "2025-03-13 00:20:23 UTC", description: "Remove link checker command from CI commands", pr_number: 22645, scopes: ["website"], type: "fix", breaking_change: false, author: "Devin Ford", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "c17746def3891ff4d7b69e2cfa5bbb6c484e4565", date: "2025-03-13 00:37:59 UTC", description: "fix enrichment table link", pr_number: 22646, scopes: ["website"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 4, deletions_count: 1},
		{sha: "522fe02b17e814bfd8136f4094a38546c7af4bf2", date: "2025-03-13 00:49:53 UTC", description: "update CODEOWNERS for website related PRs", pr_number: 22647, scopes: ["administration"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 8, deletions_count: 1},
		{sha: "9e8445ea8c2852b23a161ae0b69f9f406f4b6eb0", date: "2025-03-13 01:38:02 UTC", description: "Prevent overflow for gcp token expiry calculation", pr_number: 22639, scopes: ["gcp service"], type: "fix", breaking_change: false, author: "Rob Blafford", files_count: 2, insertions_count: 16, deletions_count: 12},
		{sha: "c9d5b907c2dd30c60d18e6d21648632ce9b4477a", date: "2025-03-14 07:35:05 UTC", description: "fix typo in csv enrichment guide", pr_number: 22652, scopes: ["external"], type: "docs", breaking_change: false, author: "Shin Seunghun", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "9668f2da58ba9b8030a5f70f141be82b834d0cd0", date: "2025-03-13 16:20:39 UTC", description: "add `alpha` definition", pr_number: 22635, scopes: ["website"], type: "docs", breaking_change: false, author: "Nick Wang", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "709f49d0518083c08deecffb533d4cb680654253", date: "2025-03-13 21:51:16 UTC", description: "correctly parse and store access key", pr_number: 22629, scopes: ["aws_kinesis_firehose source"], type: "fix", breaking_change: false, author: "noibu-gregory", files_count: 2, insertions_count: 61, deletions_count: 1},
		{sha: "b79b30828dfa803b471b1f216b96989e8205abbf", date: "2025-03-17 18:12:10 UTC", description: "Bump indexmap from 2.7.1 to 2.8.0", pr_number: 22667, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 26, deletions_count: 26},
		{sha: "b23feb371d2a99428b32f693ff0830a4aff09dcc", date: "2025-03-17 18:13:40 UTC", description: "Bump uuid from 1.15.1 to 1.16.0", pr_number: 22666, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "b0cab4fa330d2234063ff648d63e6386dddac489", date: "2025-03-17 18:30:56 UTC", description: "Bump tempfile from 3.18.0 to 3.19.0", pr_number: 22669, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 4},
		{sha: "81140cd76e20ec6022452026867a9c9d89c4caeb", date: "2025-03-17 23:16:25 UTC", description: "Bump the aws group with 2 updates", pr_number: 22663, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 39, deletions_count: 20},
		{sha: "a51cd8d6638a9de7319583b31bba52b026fc6b6a", date: "2025-03-17 20:42:22 UTC", description: "Bump the npm_and_yarn group across 1 directory with 2 updates", pr_number: 22671, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 35, deletions_count: 30},
		{sha: "3e0c157f3091ac4c3660a5f24749dd7fd0c7a277", date: "2025-03-17 20:54:18 UTC", description: "Update if statement for preview trigger", pr_number: 22673, scopes: ["website"], type: "chore", breaking_change: false, author: "Devin Ford", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "4762af5c3ddd670394360504260ca3797cebfa87", date: "2025-03-17 21:00:21 UTC", description: "add the npm package ecosystem for dependabot", pr_number: 22674, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 17, deletions_count: 0},
		{sha: "5e392ade8d2080d136b32d02992534190b985668", date: "2025-03-18 02:12:18 UTC", description: "Handle reload based on referenced file change", pr_number: 22539, scopes: ["cli"], type: "feat", breaking_change: false, author: "Guillaume Le Blanc", files_count: 14, insertions_count: 261, deletions_count: 53},
		{sha: "090017566704ffba9757ee6a261db1317919e24f", date: "2025-03-18 01:15:49 UTC", description: "Bump the patches group across 1 directory with 7 updates", pr_number: 22672, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 116, deletions_count: 116},
		{sha: "c37f0a87ab305cda544d37e8c7fa6763e13b9cf5", date: "2025-03-17 23:07:00 UTC", description: "Allow S3Sink builder to accept a type that implements Partitioner", pr_number: 22658, scopes: ["sinks"], type: "chore", breaking_change: false, author: "Rob Blafford", files_count: 1, insertions_count: 15, deletions_count: 8},
		{sha: "f5674617ad5d14186a092e379b8712bb9b8918ae", date: "2025-03-18 01:49:09 UTC", description: "document 'open_files' metric", pr_number: 22676, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 11, deletions_count: 0},
		{sha: "a3d1f7b382e61f12c5a16bf948435d5095c3a024", date: "2025-03-19 04:15:12 UTC", description: "Clarify Homebrew support documentation", pr_number: 22684, scopes: ["external"], type: "docs", breaking_change: false, author: "nemobis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "1c722e677e865539f7bdeb68e4dd99283179529f", date: "2025-03-20 00:35:01 UTC", description: "Update search index for developer guides", pr_number: 22685, scopes: ["website"], type: "fix", breaking_change: false, author: "Devin Ford", files_count: 3, insertions_count: 8, deletions_count: 2},
		{sha: "d5afc84c2b5de5955508a2ada3fc62333909ba5f", date: "2025-03-20 19:57:27 UTC", description: "`scripts/environment/Dockerfile` now based on ubuntu-24.04", pr_number: 22699, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "63db03579d786d2d82e58e1b1a78f4a8e1304586", date: "2025-03-20 21:00:00 UTC", description: "automate the rest of the release prep steps", pr_number: 22688, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 6, insertions_count: 255, deletions_count: 20},
		{sha: "0c873ecd658636a15bcaa82cc6b4c1867bf93cde", date: "2025-03-21 22:16:54 UTC", description: "Respect endpoint_url field in AWS_CONFIG_FILE", pr_number: 22687, scopes: ["aws_s3 sink"], type: "fix", breaking_change: false, author: "Orri Ganel", files_count: 6, insertions_count: 119, deletions_count: 69},
		{sha: "f095e154a8b47c32685a3a25722de9dffcf90fc4", date: "2025-03-24 21:11:12 UTC", description: "Bump the patches group with 8 updates", pr_number: 22711, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 31, deletions_count: 31},
		{sha: "6a7d9dd6d646b3ecaeb5b085a777bcc3a6979f91", date: "2025-03-24 21:13:02 UTC", description: "Bump the aws group with 5 updates", pr_number: 22712, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 36, deletions_count: 35},
		{sha: "48348ff1f0517566e6dd228ae96d1649066a925e", date: "2025-03-24 22:51:59 UTC", description: "Update `typesense-sync` depedency, search config", pr_number: 22707, scopes: ["website"], type: "chore", breaking_change: false, author: "Brian Deutsch", files_count: 4, insertions_count: 158, deletions_count: 22},
		{sha: "56fd46215cc2080651fa3dc466b6fc8cebd75e50", date: "2025-03-24 23:00:29 UTC", description: "publish build images to the GitHub Container Registry", pr_number: 22694, scopes: ["ci"], type: "feat", breaking_change: false, author: "Pavlos Rontidis", files_count: 3, insertions_count: 28, deletions_count: 12},
		{sha: "e44cd9106f84a9a9e87f2e82506113b970f40d0d", date: "2025-03-26 06:25:39 UTC", description: "fix incorrect example about the contains function", pr_number: 22724, scopes: ["website"], type: "docs", breaking_change: false, author: "Shin Seunghun", files_count: 1, insertions_count: 11, deletions_count: 2},
		{sha: "1b2a413a462c17c8c125c0172e97b76292d8d211", date: "2025-03-26 23:03:48 UTC", description: "fix DD agent docs", pr_number: 22731, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 9, deletions_count: 5},
		{sha: "5b490dabc6aac5de534862438a96a3b52354ce09", date: "2025-03-27 00:13:25 UTC", description: "Implement Display trait for Discriminant type", pr_number: 22732, scopes: ["internal"], type: "enhancement", breaking_change: false, author: "ArunPiduguDD", files_count: 1, insertions_count: 34, deletions_count: 0},
		{sha: "44562f6b3b8a1682009f0a34dc9fb61f3e668dbf", date: "2025-03-27 00:55:40 UTC", description: "Refactor throttle/rate limiter logic into reusable wrapper", pr_number: 22719, scopes: ["throttle transform"], type: "enhancement", breaking_change: false, author: "ArunPiduguDD", files_count: 4, insertions_count: 224, deletions_count: 154},
		{sha: "a2728ebae5568f0999add804bcebbb45c845ee09", date: "2025-03-27 23:08:45 UTC", description: "improve debugging guide", pr_number: 22735, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 215, deletions_count: 6},
		{sha: "0be41370135167ec75278db09b5729c0003af1f2", date: "2025-03-28 19:29:41 UTC", description: "Sample by percent rate", pr_number: 22727, scopes: ["sample transform"], type: "feat", breaking_change: false, author: "Rob Blafford", files_count: 6, insertions_count: 231, deletions_count: 44},
		{sha: "8bc821f992207e2f6b5372cb7e9fc17aed0de62b", date: "2025-03-29 00:33:05 UTC", description: "Revert add `headers` option\"", pr_number: 22741, scopes: ["vector source"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 1, deletions_count: 47},
		{sha: "645989dc42dbe6c667107f5c138c51246b05fc2c", date: "2025-03-31 23:20:13 UTC", description: "Bump the patches group with 7 updates", pr_number: 22753, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 45, deletions_count: 33},
		{sha: "e1bfa9e648feafcdeb90c574797cbeeb0925739d", date: "2025-04-01 08:36:08 UTC", description: "fix dead link on web playground README", pr_number: 22681, scopes: ["internal docs"], type: "docs", breaking_change: false, author: "Shin Seunghun", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "43cb82a1301dd0c6e115c2958a71be92e49de01c", date: "2025-04-01 01:09:29 UTC", description: "Bump the aws group across 1 directory with 4 updates", pr_number: 22766, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 18, deletions_count: 15},
		{sha: "ebb582998f716cc32c968cecf1f191482b8cf70e", date: "2025-04-01 12:29:12 UTC", description: "Fix wrong links in the guide to Parsing CSV logs with Lua", pr_number: 22745, scopes: ["external docs"], type: "fix", breaking_change: false, author: "Shin Seunghun", files_count: 2, insertions_count: 3, deletions_count: 8},
		{sha: "e7c4995c482fe854fae3132f47f81ae00d869282", date: "2025-04-01 00:15:11 UTC", description: "fix is_nullish docs", pr_number: 22765, scopes: ["external"], type: "docs", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 9, deletions_count: 2},
		{sha: "3ff60f215fe78040dfa418861d094217d6422800", date: "2025-04-02 07:04:26 UTC", description: "use explicit cast because deranged crate may break compile", pr_number: 22747, scopes: ["dev"], type: "fix", breaking_change: false, author: "Suika", files_count: 1, insertions_count: 1, deletions_count: 3},
		{sha: "61d545b263170383f352ec7c8bc6667aa167f984", date: "2025-04-01 22:30:01 UTC", description: "Bump vrl from `2d5e2df` (edited by pront)", pr_number: 22668, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 7, insertions_count: 20, deletions_count: 35},
		{sha: "8d4c82ed6b5f91d800958dd8f5eab7e13ca585a7", date: "2025-04-02 02:50:59 UTC", description: "Bump async-nats from 0.33.0 to 0.40.0", pr_number: 22759, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 86, deletions_count: 31},
		{sha: "5307b3dab262bf7283f262c46cee3c3cbfe414fc", date: "2025-04-02 05:37:21 UTC", description: "vector top is not refreshing", pr_number: 22748, scopes: ["cli"], type: "fix", breaking_change: false, author: "Jakub Onderka", files_count: 3, insertions_count: 9, deletions_count: 112},
		{sha: "6bc5dce33a3ac898e5dea45f90674ac77c83e160", date: "2025-04-03 01:45:51 UTC", description: "update `hickory_proto` to 0.25.0", pr_number: 21759, scopes: ["deps"], type: "chore", breaking_change: false, author: "Ensar Sarajčić", files_count: 6, insertions_count: 166, deletions_count: 96},
		{sha: "4d8f03ed9b00bf276d88092395ec28cad1b0a5d4", date: "2025-04-03 09:16:06 UTC", description: "update guide to use new config instead of deprecated one", pr_number: 22775, scopes: ["external"], type: "docs", breaking_change: false, author: "Shin Seunghun", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "89008b6efe89a9dbe25bb1d4a4b26c4e2c1b7a9f", date: "2025-04-02 21:49:07 UTC", description: "PR template enhancements", pr_number: 22779, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 8, deletions_count: 3},
		{sha: "5ecf5d6b0a799e2bc2a36c83f38b7ebea481d8b7", date: "2025-04-02 23:30:53 UTC", description: "run IT suite when cargo files are modified", pr_number: 22771, scopes: ["ci"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 4, insertions_count: 8, deletions_count: 2},
		{sha: "ea1cb23d789a0c17aa109a0b7afb986a6aa3c875", date: "2025-04-04 01:39:24 UTC", description: "Update kustomization.yaml", pr_number: 22776, scopes: ["kubernetes platform"], type: "chore", breaking_change: false, author: "Yurii Vlasov", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "3c073f9bf91ff36a308f5edb988e657ed753a551", date: "2025-04-04 01:02:09 UTC", description: "run memory enrichment table as a source too", pr_number: 22466, scopes: ["enriching"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 15, insertions_count: 626, deletions_count: 89},
		{sha: "a6d3addd61587372bcc65ac0e0ae6a05614a7a13", date: "2025-04-04 04:43:09 UTC", description: "add simple customizable extra metrics tags for `websocket_server`", pr_number: 22484, scopes: ["websocket_server sink"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 7, insertions_count: 466, deletions_count: 33},
		{sha: "9a0afba675b84a3e8f0b77a80532a10e763237d6", date: "2025-04-04 04:44:16 UTC", description: "separate `expire_metrics_secs` configuration per metric set", pr_number: 22409, scopes: ["metrics"], type: "feat", breaking_change: false, author: "Ensar Sarajčić", files_count: 11, insertions_count: 967, deletions_count: 242},
		{sha: "fef395dba603cd5a24083bcdff702509efb940dd", date: "2025-04-04 19:07:38 UTC", description: "vdev perpare fixes", pr_number: 22790, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "8d5c996b290d96d4d1a1cb033078fa0fdb86b0d6", date: "2025-04-04 20:41:42 UTC", description: "vdev prepare refactoring", pr_number: 22791, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 330, deletions_count: 272},
		{sha: "8a06205f131e0348cd510b3674ab4db327c6040f", date: "2025-04-04 20:56:58 UTC", description: "release.rb now fully resolves paths", pr_number: 22792, scopes: ["releasing"], type: "fix", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "d8c5e0cc521bfd15882ff38116810f43d617f09b", date: "2025-04-04 21:25:19 UTC", description: "vdev prepare tweaks", pr_number: 22794, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 2},
		{sha: "992213ec59e7110ceb7aedcaf2e7614ff2cc853d", date: "2025-04-04 21:36:27 UTC", description: "use build licenses instead of running the tool directly", pr_number: 22795, scopes: ["dev"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "34328b354585eb35938c28e0f5e45df5922339a8", date: "2025-04-04 21:56:29 UTC", description: "add debug statements", pr_number: 22796, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 15, deletions_count: 6},
		{sha: "4cda79849ec5a3424e7c7857141dccdd54e2b312", date: "2025-04-04 22:02:58 UTC", description: "repo root fixes", pr_number: 22797, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 1, insertions_count: 3, deletions_count: 3},
		{sha: "1301b96e5e0fd9c7f76a1a27c8f43f2abd2e8a65", date: "2025-04-04 22:29:52 UTC", description: "vdev prepare add before commit", pr_number: 22798, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Pavlos Rontidis", files_count: 2, insertions_count: 8, deletions_count: 0},
	]
}
