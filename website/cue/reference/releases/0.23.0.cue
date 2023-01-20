package metadata

releases: "0.23.0": {
	date:     "2022-07-11"
	codename: ""

	whats_next: [
		{
			title: "OpenTelemetry support"
			description: """
				We plan to focus on adding Open Telemetry support to Vector in the form of an `opentelemetry` source and sink in
				Q3 (starting from the [contribution](https://github.com/vectordotdev/vector/pull/13320) from
				[caibirdme](https://github.com/caibirdme))!.
				"""
		},
		{
			title: "Improving Vector's delivery guarantees"
			description: """
				Another focus of Q3 for us will be shoring up Vector's delivery guarantees to eliminate the possibilities
				of Vector unintentionally dropping data once it has accepted it (when end-to-end acknowledgements are
				enabled, which we also intend to make the default eventually!).
				"""
		},
	]

	known_issues: [
		"Vector shuts down when a configured source codec (`decoding.codec`) receives invalid data. Fixed in v0.23.1.",
		"The `elasticsearch` sink doesn't evaluate templated configuration options like the `index` configuration before applying the `only_fields` and `except_fields` options, causing templates to fail to be evaluated if they used a field that was dropped. Fixed in v0.23.1.",
		"The `datadog_traces` sink APM stats calculation does not correctly aggregate the stats in the way that is expected by the APM backend of Datadog, causing incorrect individual span metrics observed in the Datadog UI. Fixed in v0.25.2.",
	]

	description: """
		The Vector team is pleased to announce version 0.23.0!

		Be sure to check out the [upgrade guide](/highlights/2022-07-07-0-23-0-upgrade-guide) for breaking changes in
		this release.

		In addition to the new features, enhancements, and fixes listed below, this release adds:

		- Support for loading secrets from an external process. See the [release
		  highlight](/highlights/2022-07-07-secrets-management) for details.
		- Support for new encoding options to all sinks that support codecs, that mirror the decoding options available
		  on sources. This allows for more codecs (like `json` and `logfmt`) and framings (like newline-delimited and
		  length-delimited) to be used on more sinks. See the [release highlight](/highlights/2022-07-07-sink-codecs)
		  for details.
		"""

	changelog: [
		{
			type: "feat"
			scopes: ["sinks", "codecs"]
			description: """
				Support was added for new encoding options to all sinks that support codecs, that mirror the decoding
				options available on sources. This allows for more codecs (like `json` and `logfmt`) and framings (like
				newline-delimited and octet-framing) to be used on more sinks. See the [release
				highlight](/highlights/2022-07-07-sink-codecs) for details.
				"""
			pr_numbers: []
		},
		{
			type: "fix"
			scopes: ["kubernetes_logs source"]
			description: """
				The `kubernetes_logs` source no longer leaks resources (Tokio tasks) during configuration reload.
				"""
			pr_numbers: [12766]
			contributors: ["nabokihms"]
		},
		{
			type: "feat"
			scopes: ["config"]
			description: """
				Vector now has a mechanism for loading secrets into configuration by executing an external program. See
				the [release highlight](/highlights/2022-07-07-secrets-management) for details.
				"""
			pr_numbers: [11985]
		},
		{
			type: "enhancement"
			scopes: ["vrl stdlib"]
			description: """
				VRL's [`is_json`](/docs/reference/vrl/functions/#is_json) function now takes a `variant` argument making
				it easier to assert the type of the JSON value; for example that the text is a JSON object.
				"""
			contributors: ["nabokihms"]
			pr_numbers: [12797]
		},
		{
			type: "enhancement"
			scopes: ["kubernetes_logs source"]
			description: """
				The `kubernetes_logs` source now annotates logs with node labels. This requires updating to the version
				>= 0.11.0 of the helm chart or adding the `node` resource to the allowed actions for the Vector
				pod. See the [upgrade
				guide](/highlights/2022-07-07-0-23-0-upgrade-guide#kubernetes-logs-list-watch-nodes) for more
				details.
				"""
			contributors: ["nabokihms"]
			breaking: true
			pr_numbers: [12730]
		},
		{
			type: "enhancement"
			scopes: ["vrl stdlib"]
			description: """
				VRL's `parse_nginx_log` function now parses out the `upstream` value if it exists in the log line.
				"""
			contributors: ["nabokihms"]
			pr_numbers: [12819]
		},
		{
			type: "fix"
			scopes: ["releasing"]
			description: """
				The Vector SystemD unit file installed by the Debian package no longer automatically starts Vector. This
				seems to be more expected by users, as the default configuration is only useful as an example, and
				matches the behavior of the RPM.
				"""
			contributors: ["akx"]
			breaking: true
			pr_numbers: [12650]
		},
		{
			type: "enhancement"
			scopes: ["geoip transform"]
			description: """
				The `geoip` transform now includes the following additional fields:

				* `country_name`
				* `region_code`
				* `region_name`
				* `metro_code`

				This brings the transform up to parity with the fields enriched by Logstash's geoip filter (though the
				field names are not the same).
				"""
			pr_numbers: [12803]
		},
		{
			type: "enhancement"
			scopes: ["datadog_metrics sink"]
			description: """
				The `datadog_metrics` sink now achieves a better compression ratio, when compression is enabled, due to
				sorting the metrics before compression when transmitting them.
				"""
			pr_numbers: [12810]
		},
		{
			type: "enhancement"
			scopes: ["azure_blob sink"]
			description: """
				The `azure_blob` sink now supports loading credentials from environment variables and via the managed
				identity service. To use this, set the new [`storage_account`
				parameter](/docs/reference/configuration/sinks/azure_blob/#storage_account).
				"""
			pr_numbers: [12821, 12959]
			contributors: ["yvespp"]
		},
		{
			type: "enhancement"
			scopes: ["prometheus_scrape source"]
			description: """
				The `prometheus_scrape` source now sets the `Accept` header to `text/plain` when requesting metrics.
				This improves compatibility with Prometheus exporters like keyclock which require this header.
				"""
			pr_numbers: [12870]
		},
		{
			type: "enhancement"
			scopes: ["datadog_agent source"]
			description: """
				The `datadog_agent` source is now able to accept traces from newer Datadog Agents (version >= 7.33).
				"""
			pr_numbers: [12658]
		},
		{
			type: "enhancement"
			scopes: ["vrl stdlib"]
			description: """
				VRL's `parse_nginx_log` function now correctly parses:

				* rate limit errors
				* log entries including the user field
				"""
			contributors: ["nabokihms"]
			pr_numbers: [12905, 13332]
		},
		{
			type: "fix"
			scopes: ["vector source"]
			description: """
				The `vector` source now reports the correct number of bytes received in `component_received_bytes_total`.
				"""
			pr_numbers: [12910]
		},
		{
			type: "fix"
			scopes: ["aws_ec2_metadata transform"]
			description: """
				The `aws_ec2_metadata` transform now has a lower default request timeout, 1 second rather than 60
				seconds, to allow Vector to fail more quickly if the IMDSv2 is unavailable. This can be configured via
				the new `refresh_timeout_secs` option.
				"""
			pr_numbers: [12920]
		},
		{
			type: "feat"
			scopes: ["aws_cloudwatch_logs sink"]
			description: """
				The `aws_cloudwatch_logs` sink now allows configuration of request headers via the `headers` option.
				This was primarily added to allow setting the `x-amzn-logs-format` header when sending [Embedded Metric
				Format](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/CloudWatch_Embedded_Metric_Format_Specification.html#CloudWatch_Embedded_Metric_Format_Specification_PutLogEvents)
				logs to AWS CloudWatch Logs.
				"""
			contributors: ["hencrice"]
			pr_numbers: [12866]
		},
		{
			type: "enhancement"
			scopes: ["vrl"]
			description: """
				VRL's diagnostic error messages have had a number of improvements in this release which should help
				users more quickly identify errors in their VRL code.

				See the [originating
				RFC](https://github.com/vectordotdev/vector/blob/78b0a76c8826d62ad71a2b323e27dbb3b322ed09/rfcs/2021-08-22-7204-vrl-error-diagnostic-improvements.md)
				for full details about the improvements that were made.
				"""
			pr_numbers: [12863, 12880, 12890]
		},
		{
			type: "enhancement"
			scopes: ["vrl", "observability"]
			description: """
				VRL's `log` function now correctly independently rate limits multiple `log` calls in a single `remap`
				transform. Previously they were mistakenly rate limited all together.
				"""
			pr_numbers: [12952]
		},
		{
			type: "fix"
			scopes: ["tag_cardinality_limit transform"]
			description: """
				The `tag_cardinality_limit` now correctly deserializes the `action` option. Previously it would return an
				error when trying to configure this option.
				"""
			pr_numbers: [12979]
		},
		{
			type: "enhancement"
			scopes: ["splunk_hec_log sink"]
			description: """
				The `splunk_hec_logs` sink now has a new option to configure Vector to not set a timestamp on the event
				when sending it to Splunk: `suppress_timestamp`. This allows Splunk to set the timestamp during
				ingestion.
				"""
			pr_numbers: [12946]
		},
		{
			type: "fix"
			scopes: ["vrl"]
			description: """
				There were some situations where VRL didn't calculate the correct type definition of values which were
				fixed in this release. In some cases this can cause VRL compilation errors when upgrading if the code
				relied on the previous behavior due to unneeded type assertions. The VRL diagnostic error messages
				should guide you towards resolving them.

				This affects the following:

				- the "merge" operator (`|` or `|=`) on objects that share keys with different types
				- if statements
				- nullability checking for most expressions (usually related to if statements)
				- expressions that contain the `abort` expression
				- the `del` function
				- closure arguments

				See the [upgrade guide](/highlights/2022-07-07-0-23-0-upgrade-guide) for more details.
				"""
			pr_numbers: [12981, 13201, 13208, 13164]
		},
		{
			type: "fix"
			scopes: ["file source", "kafka source", "journald source", "delivery"]
			description: """
				When end-to-end acknowledgements are enabled, the following sources now correctly handle negative acknowledgements by halting processing :

				* `kafka`
				* `journald`
				* `file`

				Previously these sources would continue processing, potentially resulting in dropped data.
				"""
			pr_numbers: [12901, 12913, 12936]
		},
		{
			type: "chore"
			scopes: ["vrl stdlib"]
			description: """
				The `parse_grok` function now always omits fields from the pattern that did not match the input, dropping the `remove_empty` parameter.

				See the [upgrade guide](/highlights/2022-07-07-0-23-0-upgrade-guide#vrl-parse_grok) for details.
				"""
			breaking: true
			pr_numbers: [13008]
		},
		{
			type: "enhancement"
			scopes: ["delivery", "console sink", "nats sink", "websocket sink"]
			description: """
				The following sinks now allow configuration of end-to-end acknowledgements (via `acknowledgements`):

				* `console`
				* `nats`
				* `websocket`
				"""
			pr_numbers: [13022, 13147, 13289]
		},
		{
			type: "fix"
			scopes: ["datadog_traces sink"]
			description: """
				The `datadog_traces` sink now calculates statistics from incoming and forwards them to Datadog for use by the APM product.
				"""
			pr_numbers: [12806]
		},
		{
			type: "fix"
			scopes: ["buffers"]
			description: """
				The disk buffers no longer panic on recoverable errors when reading from the buffer.
				"""
			pr_numbers: [13180]
		},
		{
			type: "fix"
			scopes: ["datadog_agent source"]
			description: """
				The `datadog_agent` source now correctly parses the namespace of incoming metrics from the agent by
				looking for the first `.`. For example a metric of `system.bytes_read` would have a namespace of
				`system` and a name of `bytes_read`. This fixes interoperability issues with the `datadog_metrics` sink
				which is capable of adding a default namespace if the incoming metrics do not have one.
				"""
			pr_numbers: [13176]
		},
		{
			type: "fix"
			scopes: ["vrl stdlib"]
			description: """
				VRL's `parse_aws_cloudwatch_log_subscription_message` type definition was corrected so the `.events`
				field is correctly identified as an array of objects rather than an object.
				"""
			pr_numbers: [13189]
		},
		{
			type: "feat"
			scopes: ["socket source", "syslog source", "fluent source", "logstash source"]
			description: """
				TCP-based sources like the `socket` source can now be configured to annotate events with the TLS client
				certificate of the connection the events came from via setting the `tls.peer_key` configuration option.
				"""
			contributors: ["JustinKnueppel"]
			pr_numbers: [11905]
		},
		{
			type: "fix"
			scopes: ["vrl stdlib"]
			description: """
				VRL's `parse_int` function now correctly parses the string `0` as `0` without setting the base.
				Previously it would return an error.
				"""
			contributors: ["shenxn"]
			pr_numbers: [13216]
		},
		{
			type: "feat"
			scopes: ["splunk_hec_logs sink"]
			description: """
				The `splunk_hec_logs` sink can now be configured to send to the [Splunk HEC raw
				endpoint](https://docs.splunk.com/Documentation/Splunk/8.0.0/RESTREF/RESTinput#services.2Fcollector.2Fraw)
				via the added `endpoint_target` option. The default is still the [event
				endpoint](https://docs.splunk.com/Documentation/Splunk/8.0.0/RESTREF/RESTinput#services.2Fcollector.2Fevent).
				"""
			pr_numbers: [13041]
		},
		{
			type: "fix"
			scopes: ["syslog source", "vrl stdlib", "codecs"]
			description: """
				Vector's parsing of Syslog messages now preserves empty structured data elements rather than dropping
				them. This affects the `syslog` source, the `syslog` codec, and the `parse_syslog` VRL function.
				"""
			pr_numbers: [13256]
		},
		{
			type: "fix"
			scopes: ["gcp_pubsub source"]
			description: """
				The `gcp_pubsub` source now sends heartbeats to the server to avoid inactivity timeouts. The default for
				this is 75 seconds but can be configured via the added `keepalive_secs` parameter.
				"""
			pr_numbers: [13224]
		},
		{
			type: "fix"
			scopes: ["gcp_pubsub source"]
			description: """
				The `gcp_pubsub` source configuration options ending in `_seconds` were renamed to end in `_secs` to
				match other Vector configuration options that take a number of seconds. The original names are aliased,
				but deprecated so configuration should be updated to use the new names.
				"""
			pr_numbers: [13224]
		},
		{
			type: "fix"
			scopes: ["datadog_logs sink"]
			description: """
				The `datadog_logs` sink now correctly retries requests due to aborted connections.
				"""
			pr_numbers: [13130]
		},
		{
			type: "enhancement"
			scopes: ["pulsar sink"]
			description: """
				The `pulsar` sink now allows authentication via OAuth2 via new `auth.oauth2` configuration option.
				"""
			contributors: ["fantapsody"]
			pr_numbers: [10463]
		},
		{
			type: "fix"
			scopes: ["pulsar sink"]
			description: """
				The `pulsar` sink now sends the event timestamp as the message timestamp, if the event has one.
				"""
			contributors: ["fantapsody"]
			pr_numbers: [10463]
		},
		{
			type: "enhancement"
			scopes: ["gcp_pubsub source"]
			description: """
				The performance of the `gcp_pubsub` source has been improved by automatically scaling up the number of
				consumers within Vector to a maximum of the newly added `max_concurrency` option (defaults to
				10).
				"""
			pr_numbers: [13240]
		},
		{
			type: "enhancement"
			scopes: ["gcp provider"]
			description: """
				All GCP components now correctly apply the `auth.api_key` option. This was previously only supported by
				the `gcp_pubsub` source and sink and ignored by all other GCP components.
				"""
			pr_numbers: [13324]
		},
		{
			type: "fix"
			scopes: ["reload", "shutdown"]
			description: """
				Vector no longer becomes unresponsive when receiving more than two SIGHUPs, instead it will warn when
				the signal handler channel has overflowed.
				"""
			contributors: ["wjordan"]
			pr_numbers: [13241]
		},
		{
			type: "fix"
			scopes: ["buffers"]
			description: """
				Disk buffers now correctly apply the maximum size configured. Previously it could write an additional
				128 MB. As a part of this, the minimum configurable disk buffer size is now 256 MB.
				"""
			pr_numbers: [13356]
		},
		{
			type: "fix"
			scopes: ["dnstap source"]
			description: """
				Bring supported dnstap proto definitions up-to-date with upstream adding support for DoT, DoH and
				DNSCrypt SocketProtocol values.
				"""
			contributors: ["franklymrshankley"]
			pr_numbers: [13227]
		},
		{
			type: "fix"
			scopes: ["vrl stdlib", "syslog source"]
			breaking: true
			description: """
				The `syslog` source and VRL's `parse_syslog` structured data fields were made consistent in their
				handling. See the [upgrade guide](/highlights/2022-07-07-0-23-0-upgrade-guide#parse-syslog) for more
				details.
				"""
			pr_numbers: [12433]
		},
		{
			type:     "chore"
			breaking: true
			scopes: ["releasing"]
			breaking: true
			description: """
				Due to changes to the [tool we use for cross-compiling Vector](https://github.com/cross-rs/cross),
				support for operating systems with old versions of `libc` and `libstdc++` were dropped for the
				`x86-unknown_linux-gnu` target. Vector now requires that the host system has `libc` >= 2.18 and
				`libstdc++` >= 3.4.21 with support for ABI version 1.3.8.

				Known OSes that this affects:

				- Amazon Linux 1
				- Ubuntu 14.04
				- CentOS 7

				We will be looking at options to [re-add support for these
				OSes](http://github.com/vectordotdev/vector/issues/13183) in the future.
				"""
			pr_numbers: []
		},
	]

	commits: [
		{sha: "274df231ea01438cfa28088daa78596b4825a9c2", date: "2022-05-19 12:08:20 UTC", description: "abort reflectors on vector reload", pr_number:                                                               12766, scopes: ["kubernetes_logs"], type:                              "fix", breaking_change:         false, author: "Maksim Nabokikh", files_count:       1, insertions_count:   16, deletions_count:   2},
		{sha: "43b5bc70d59cc0ee69931d0eb5b7ef80bcc40bb3", date: "2022-05-19 04:19:57 UTC", description: "RFC for log namespacing", pr_number:                                                                         12351, scopes: [], type:                                               "chore", breaking_change:       false, author: "Nathan Fox", files_count:            2, insertions_count:   438, deletions_count:  0},
		{sha: "ba312f3bcb82cec8929af91fe9400fa59ff95e71", date: "2022-05-19 09:25:43 UTC", description: "bump EmbarkStudios/cargo-deny-action from 1.2.17 to 1.3.0", pr_number:                                       12772, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   2, deletions_count:    2},
		{sha: "dc23db81844c541d1c789698b33430aac61dec8b", date: "2022-05-19 12:10:44 UTC", description: "introduce external secret management", pr_number:                                                            11985, scopes: ["config"], type:                                       "feat", breaking_change:        false, author: "Pierre Rognant", files_count:        17, insertions_count:  708, deletions_count:  35},
		{sha: "c7127eddee03ca528a82d8297300178d937954c0", date: "2022-05-19 06:24:29 UTC", description: "Update AWS dependencies to 0.12.0/0.42.0", pr_number:                                                        12775, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         2, insertions_count:   62, deletions_count:   64},
		{sha: "bbd2cb0928a47f0e9d22b1873462149d0c01a193", date: "2022-05-19 16:10:32 UTC", description: "bump test-case from 2.0.2 to 2.1.0", pr_number:                                                              12784, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   11, deletions_count:   2},
		{sha: "6d06df6d4daa97a85679bce8ed8d8c778a6d314b", date: "2022-05-19 16:40:39 UTC", description: "bump syn from 1.0.93 to 1.0.95", pr_number:                                                                  12783, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   3, deletions_count:    3},
		{sha: "8ad1517ac44f8ddbc26a5e9a734ff713cc771b4c", date: "2022-05-19 16:42:59 UTC", description: "bump schemars from 0.8.8 to 0.8.10", pr_number:                                                              12782, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       3, insertions_count:   8, deletions_count:    19},
		{sha: "50dd7a7c655cbc3c7903b58213adb34a70cf7ba6", date: "2022-05-19 16:53:15 UTC", description: "bump libc from 0.2.125 to 0.2.126", pr_number:                                                               12788, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    3},
		{sha: "8e640f1a867d92e313461f18178e375d1d4c4dad", date: "2022-05-19 21:00:34 UTC", description: "bump infer from 0.7.0 to 0.8.0", pr_number:                                                                  12787, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   4, deletions_count:    4},
		{sha: "6a76a2f248df0b2d097e7b3e0a4af4435c59e4a1", date: "2022-05-20 05:02:10 UTC", description: "bump schannel from 0.1.19 to 0.1.20", pr_number:                                                             12791, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   47, deletions_count:   4},
		{sha: "2cca4fde11115329e54a706ab4084dd5c4418a53", date: "2022-05-19 23:21:51 UTC", description: "Remove simple `btreemap! {}` instances", pr_number:                                                          12758, scopes: [], type:                                               "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:    19, insertions_count:  185, deletions_count:  156},
		{sha: "cf0864873c03edd937c1e7796468809e0d52e764", date: "2022-05-20 00:40:40 UTC", description: "Revise test.yml concurrency", pr_number:                                                                     12798, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         1, insertions_count:   6, deletions_count:    2},
		{sha: "83bd797ff6a05fb3246a2442a701db3a85e323b5", date: "2022-05-20 07:10:17 UTC", description: "Allow log events to have a non-object root value", pr_number:                                                12705, scopes: ["core"], type:                                         "chore", breaking_change:       false, author: "Nathan Fox", files_count:            64, insertions_count:  887, deletions_count:  1576},
		{sha: "e20f40b4e52e059ac06a5c845cd08f2437b3f0ff", date: "2022-05-21 06:52:56 UTC", description: "add variant enum argument to the is_json function", pr_number:                                               12797, scopes: ["vrl"], type:                                          "feat", breaking_change:        false, author: "Maksim Nabokikh", files_count:       3, insertions_count:   175, deletions_count:  7},
		{sha: "31e35fb6fa7f4f0c751e5d424f840bebe4304054", date: "2022-05-21 15:11:13 UTC", description: "Fix indention in a lua transformer example", pr_number:                                                      12808, scopes: [], type:                                               "docs", breaking_change:        false, author: "Haitao Li", files_count:             1, insertions_count:   23, deletions_count:   23},
		{sha: "7afe16e193152a3ea041f0a1a63ac24b5d700d55", date: "2022-05-20 22:23:48 UTC", description: "bump once_cell from 1.10.0 to 1.11.0", pr_number:                                                            12801, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       9, insertions_count:   10, deletions_count:   10},
		{sha: "424b6b123fa922ca4fc4b3a204a49c8b1221dcd5", date: "2022-05-21 07:47:25 UTC", description: "bump cidr-utils from 0.5.6 to 0.5.7", pr_number:                                                             12809, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    3},
		{sha: "c2ba151ac037a01584c1493d876af8e16f31b5bf", date: "2022-05-21 09:23:30 UTC", description: "bump EmbarkStudios/cargo-deny-action from 1.3.0 to 1.3.1", pr_number:                                        12811, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   2, deletions_count:    2},
		{sha: "46a91d623ade84b5480c9fcf027cdabe2df2f111", date: "2022-05-24 03:06:36 UTC", description: "Implement `EncodingConfig`/`EncodingConfigWithFraming`", pr_number:                                          12765, scopes: ["codecs"], type:                                       "chore", breaking_change:       false, author: "Pablo Sichert", files_count:         9, insertions_count:   229, deletions_count:  41},
		{sha: "3b7f75de203154fccf192aaa249aa8695a185920", date: "2022-05-23 23:30:13 UTC", description: "Update lading", pr_number:                                                                                   12776, scopes: [], type:                                               "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:    1, insertions_count:   1, deletions_count:    1},
		{sha: "f3603d40e5fffe270958d50bb7d0085972f8e22d", date: "2022-05-24 11:55:45 UTC", description: "Annotate logs with node labels", pr_number:                                                                  12730, scopes: ["kubernetes_logs"], type:                              "feat", breaking_change:        false, author: "Maksim Nabokikh", files_count:       9, insertions_count:   307, deletions_count:  23},
		{sha: "57d4ca8a7cbbed4636c5fabceffda21eb857de89", date: "2022-05-24 14:29:31 UTC", description: "parse_nginx_log upstream label", pr_number:                                                                  12819, scopes: ["vrl"], type:                                          "fix", breaking_change:         false, author: "Maksim Nabokikh", files_count:       2, insertions_count:   21, deletions_count:   0},
		{sha: "b530718e961b6d4a0a4ba9428fdcccb41a8a8e18", date: "2022-05-24 18:14:24 UTC", description: "Don't enable & start service by default on Debian", pr_number:                                               12650, scopes: ["releasing"], type:                                    "fix", breaking_change:         false, author: "Aarni Koskela", files_count:         2, insertions_count:   41, deletions_count:   0},
		{sha: "259c87fbaf0fa088f9de0cf8d243b86fa7085b7a", date: "2022-05-25 00:32:05 UTC", description: "bump console-subscriber from 0.1.5 to 0.1.6", pr_number:                                                     12828, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   5, deletions_count:    5},
		{sha: "498a4c823282e772221911864772ece7b4df258f", date: "2022-05-25 00:33:19 UTC", description: "bump nats from 0.20.0 to 0.20.1", pr_number:                                                                 12829, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    3},
		{sha: "e0227dfa2e007151216b5973eb1b3c12d30c60ef", date: "2022-05-24 23:18:20 UTC", description: "Bring up to parity with Logstash geoip filter", pr_number:                                                   12803, scopes: ["geoip transform"], type:                              "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:         2, insertions_count:   130, deletions_count:  13},
		{sha: "967539fc486ce0c25f346a35a1178a9333746182", date: "2022-05-25 02:21:22 UTC", description: " Sort metrics for better HTTP compression", pr_number:                                                       12810, scopes: ["datadog_metrics sink"], type:                         "fix", breaking_change:         false, author: "Nathan Fox", files_count:            6, insertions_count:   107, deletions_count:  8},
		{sha: "8f75baa41f17e1f55afbc7119f8ee64795fd2c4c", date: "2022-05-24 23:21:46 UTC", description: "bump goauth from 0.12.0 to 0.13.0", pr_number:                                                               12816, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   5, deletions_count:    5},
		{sha: "5a75bcde8716fb711661f31bb37e91bfa3bd4b87", date: "2022-05-24 23:22:03 UTC", description: "bump regex from 1.5.5 to 1.5.6", pr_number:                                                                  12817, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       5, insertions_count:   8, deletions_count:    8},
		{sha: "9cd7248db7e9ce1f86c0c321c9edecec3d8fddaa", date: "2022-05-24 23:22:14 UTC", description: "bump smpl_jwt from 0.7.0 to 0.7.1", pr_number:                                                               12818, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   1, deletions_count:    1},
		{sha: "babf7534adef015bd858e7b417137a60e37dbe09", date: "2022-05-25 09:28:21 UTC", description: "bump prost-build from 0.10.3 to 0.10.4", pr_number:                                                          12840, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       4, insertions_count:   9, deletions_count:    9},
		{sha: "d728e2ed05ce96614e0703bb3adf7edf29b5dac9", date: "2022-05-25 09:29:13 UTC", description: "bump once_cell from 1.11.0 to 1.12.0", pr_number:                                                            12837, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       9, insertions_count:   10, deletions_count:   10},
		{sha: "7ebdc7cb2e4fef62c7a8632f096a76aa6efc9742", date: "2022-05-25 12:03:14 UTC", description: "bump prost from 0.10.3 to 0.10.4", pr_number:                                                                12841, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       5, insertions_count:   15, deletions_count:   15},
		{sha: "6e4b6b4f00dbf136d405f9ecd7364ff574429cfb", date: "2022-05-25 23:08:41 UTC", description: "bump tikv-jemallocator from 0.4.3 to 0.5.0", pr_number:                                                      12854, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       3, insertions_count:   6, deletions_count:    6},
		{sha: "31e1ea213923ee04396834f0f35e7aa207e8f688", date: "2022-05-26 02:53:41 UTC", description: "bump uuid from 1.0.0 to 1.1.0", pr_number:                                                                   12855, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   9, deletions_count:    9},
		{sha: "131e7af4712cb4be1cfe07e09cd68d08d864710c", date: "2022-05-26 06:56:25 UTC", description: "bump syn from 1.0.94 to 1.0.95", pr_number:                                                                  12861, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   12, deletions_count:   6},
		{sha: "246d29a3c22c87c833e6e4e0b0ac878ba669043d", date: "2022-05-26 23:47:09 UTC", description: "bump lru from 0.7.5 to 0.7.6", pr_number:                                                                    12867, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    3},
		{sha: "b06c459885efb7006a9cfda55ecb2bc8144f2855", date: "2022-05-27 00:50:01 UTC", description: "Set Accept header", pr_number:                                                                               12870, scopes: ["prometheus_scrape source"], type:                     "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:         1, insertions_count:   33, deletions_count:   0},
		{sha: "af1905a0648c56f0e82f315b4a60845ca4438616", date: "2022-05-27 04:02:27 UTC", description: "Clarify error boundaries in instrumentation spec", pr_number:                                                12822, scopes: [], type:                                               "chore", breaking_change:       false, author: "Ben Johnson", files_count:           5, insertions_count:   219, deletions_count:  119},
		{sha: "de733460d4d648c45558fb969cf2ee0db5f98243", date: "2022-05-27 02:57:28 UTC", description: "Allow println in tests", pr_number:                                                                          12876, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         1, insertions_count:   4, deletions_count:    1},
		{sha: "7862b98aa2d184cd22f0690e09f7b5d2857d03e7", date: "2022-05-27 04:08:32 UTC", description: "Mark `Write::write` as disallowed", pr_number:                                                               12875, scopes: [], type:                                               "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         3, insertions_count:   6, deletions_count:    1},
		{sha: "c58122aa6451b6ba56a20fc38e47ce0f077a1d87", date: "2022-05-27 04:27:53 UTC", description: "Use default deps for `goauth`", pr_number:                                                                   12874, scopes: ["deps", "gcp service"], type:                          "fix", breaking_change:         false, author: "Bruce Guenter", files_count:         1, insertions_count:   1, deletions_count:    1},
		{sha: "954a2e53dd9c6ce314068a696c10b56769992fa5", date: "2022-05-27 05:43:13 UTC", description: "Brush off markdown files", pr_number:                                                                        12877, scopes: [], type:                                               "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         11, insertions_count:  329, deletions_count:  379},
		{sha: "a8415c4ef00ece7de9bf5716d52f7d107daa3695", date: "2022-05-28 04:09:08 UTC", description: "add schema requirements", pr_number:                                                                         11743, scopes: ["schemas", "datadog_logs sink"], type:                 "chore", breaking_change:       false, author: "Jean Mertz", files_count:            24, insertions_count:  1412, deletions_count: 151},
		{sha: "edc4f329dd7dea7dc8e9911b9cdd0ad62435bb7d", date: "2022-05-28 02:00:06 UTC", description: "Improve readability", pr_number:                                                                             12879, scopes: ["various"], type:                                      "docs", breaking_change:        false, author: "Ryan Russell", files_count:          14, insertions_count:  33, deletions_count:   33},
		{sha: "8fa2718f34a095347827240703c3bb8cebc38123", date: "2022-05-28 00:08:55 UTC", description: "fix markdown for component.md", pr_number:                                                                   12887, scopes: [], type:                                               "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         1, insertions_count:   6, deletions_count:    6},
		{sha: "06e2c92bb29b40aa09b7dd7cc6dbb2f82bb6f316", date: "2022-05-31 08:13:09 UTC", description: "newer trace format support (incl. `datadog_traces` sink update)", pr_number:                                 12658, scopes: ["datadog_agent source"], type:                         "chore", breaking_change:       false, author: "prognant", files_count:              8, insertions_count:   486, deletions_count:  236},
		{sha: "1c689f85a7630596499c615674e5c7269b824a72", date: "2022-05-31 22:55:07 UTC", description: "bump parking_lot from 0.12.0 to 0.12.1", pr_number:                                                          12902, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   14, deletions_count:   14},
		{sha: "2906d7e822c79d2cdc0ba26aa6f4f009d930af75", date: "2022-05-31 22:57:16 UTC", description: "bump serde_with from 1.13.0 to 1.14.0", pr_number:                                                           12893, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    4},
		{sha: "0a8ea3128525c02f050d7cc42a1d1024bebd560c", date: "2022-05-31 22:58:34 UTC", description: "bump indexmap from 1.8.1 to 1.8.2", pr_number:                                                               12894, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       7, insertions_count:   8, deletions_count:    8},
		{sha: "675d9e1d580cefa35f1884cad624e2130daedfb9", date: "2022-05-31 23:11:51 UTC", description: "bump flate2 from 1.0.23 to 1.0.24", pr_number:                                                               12895, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    5},
		{sha: "3f40f7f23062cb8a235b01f7acefcd9a3928da3a", date: "2022-05-31 21:58:39 UTC", description: "Use domain qualified image names in all `Dockerfile`s", pr_number:                                           12898, scopes: [], type:                                               "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         7, insertions_count:   9, deletions_count:    9},
		{sha: "f2a091da09eaf08a83b0875f02d9ec7c3ac55db1", date: "2022-06-01 05:22:52 UTC", description: "bump listenfd from 0.5.0 to 1.0.0", pr_number:                                                               12904, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   4, deletions_count:    4},
		{sha: "0b948148aee5b21d65c061dbe3f21d52c68d7dbd", date: "2022-06-01 01:32:13 UTC", description: "Deny updates to inventory dependency", pr_number:                                                            12907, scopes: [], type:                                               "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:       1, insertions_count:   2, deletions_count:    0},
		{sha: "57bbac9ab0320b54cf4fbd7e1f010ec3ff1dc21b", date: "2022-06-01 10:19:05 UTC", description: "parse nginx rate limit errors", pr_number:                                                                   12905, scopes: ["vrl"], type:                                          "fix", breaking_change:         false, author: "Maksim Nabokikh", files_count:       2, insertions_count:   46, deletions_count:   15},
		{sha: "eae3838d7671597e65c7a6fcd11db7e59834902f", date: "2022-06-01 00:34:19 UTC", description: "Use a journalctl test script to improve testing", pr_number:                                                 12908, scopes: ["journald source"], type:                              "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         2, insertions_count:   96, deletions_count:   123},
		{sha: "acc246ddcecb8f27fc4435264208316cb60957d4", date: "2022-06-01 08:43:12 UTC", description: "fix the `BytesReceived` event for v2 variant", pr_number:                                                    12910, scopes: ["vector source"], type:                                "chore", breaking_change:       false, author: "Toby Lawrence", files_count:         5, insertions_count:   397, deletions_count:  48},
		{sha: "f82542138392372a49b51d783cdf3a6ef6508b26", date: "2022-06-01 14:50:21 UTC", description: "bump hyper from 0.14.18 to 0.14.19", pr_number:                                                              12896, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    3},
		{sha: "6ee85a526ab45f14dfef206778fd0404f55a7f6e", date: "2022-06-01 23:14:23 UTC", description: "bump uuid from 1.1.0 to 1.1.1", pr_number:                                                                   12925, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   10, deletions_count:   10},
		{sha: "80a2e09bafda2cc86d0cb4274251436e51a7c700", date: "2022-06-01 23:16:47 UTC", description: "bump rust_decimal from 1.23.1 to 1.24.0", pr_number:                                                         12926, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   2, deletions_count:    2},
		{sha: "9b51c3e6d6850a1ee6c7af3e8817a5fd9a3d44e4", date: "2022-06-02 00:34:49 UTC", description: "Fix clippy flag application again", pr_number:                                                               12922, scopes: [], type:                                               "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         2, insertions_count:   3, deletions_count:    3},
		{sha: "7df584d66ce11e87df372472e18662995968b197", date: "2022-06-02 00:08:19 UTC", description: "Lower request timeout", pr_number:                                                                           12920, scopes: ["aws_ec2_metadata transform"], type:                   "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:         2, insertions_count:   57, deletions_count:   10},
		{sha: "13ecdbd806809a78170e1c6408caa03036566e1e", date: "2022-06-02 00:57:16 UTC", description: "Fix overriding Vector fields in enterprise logs", pr_number:                                                 12929, scopes: ["observability"], type:                                "fix", breaking_change:         false, author: "Will", files_count:                  1, insertions_count:   6, deletions_count:    8},
		{sha: "5b0f4737bb9612c02b63f25d990ec41a30d69303", date: "2022-06-02 05:33:53 UTC", description: "add `json/emf` support", pr_number:                                                                          12866, scopes: ["aws_cloudwatch_logs sink"], type:                     "enhancement", breaking_change: false, author: "Yenlin Chen", files_count:           5, insertions_count:   158, deletions_count:  44},
		{sha: "c1b14b78934ca5b5832299850f740b90730d8b16", date: "2022-06-02 11:34:55 UTC", description: "Add missing import", pr_number:                                                                              12935, scopes: ["ci"], type:                                           "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:         1, insertions_count:   4, deletions_count:    1},
		{sha: "ec3e9a7ed57835b7314104a35c209a4538a35ea2", date: "2022-06-02 11:35:55 UTC", description: "Bump development version to v0.23.0", pr_number:                                                             12933, scopes: ["releasing"], type:                                    "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         2, insertions_count:   2, deletions_count:    2},
		{sha: "de81404f17e59f154a371f49258a7866c1cb2fa6", date: "2022-06-03 06:43:57 UTC", description: "prevent incorrect error diagnostics", pr_number:                                                             12863, scopes: ["vrl"], type:                                          "feat", breaking_change:        false, author: "Jean Mertz", files_count:            12, insertions_count:  333, deletions_count:  252},
		{sha: "311761cde7e8fdae702e6cdb2a130cb5e8de05af", date: "2022-06-03 03:30:11 UTC", description: "Remove VM runtime", pr_number:                                                                               12888, scopes: ["vrl"], type:                                          "chore", breaking_change:       false, author: "Nathan Fox", files_count:            177, insertions_count: 28, deletions_count:   3147},
		{sha: "fc85596cf2e2554ffbe756e86ce6739eb746e860", date: "2022-06-03 23:55:50 UTC", description: "bump syn from 1.0.95 to 1.0.96", pr_number:                                                                  12956, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   2, deletions_count:    2},
		{sha: "276e1a6b31331d9ff8b57d62bd60e5f1671ccb27", date: "2022-06-03 23:56:12 UTC", description: "bump float_eq from 0.7.0 to 1.0.0", pr_number:                                                               12957, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    3},
		{sha: "6a6dcc395ee004c25f6af527e115946d9ce06a4b", date: "2022-06-03 23:56:40 UTC", description: "bump async-trait from 0.1.53 to 0.1.56", pr_number:                                                          12955, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    3},
		{sha: "7a26b9b8183058802f0f2119c76c7205b7aa1914", date: "2022-06-04 09:08:26 UTC", description: "update azure sdk to pick up managed identity enhancement", pr_number:                                        12959, scopes: ["azure_blob sink"], type:                              "enhancement", breaking_change: false, author: "Yves Peter", files_count:            2, insertions_count:   12, deletions_count:   12},
		{sha: "ae36d4f7e35c50fd96f063ea61759054b120769d", date: "2022-06-04 01:03:28 UTC", description: "Fix rate limiting for `log` function", pr_number:                                                            12952, scopes: ["vrl"], type:                                          "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:         1, insertions_count:   68, deletions_count:   1},
		{sha: "995b2fcb27d14bc4d56a97c94064ee174f72f9f3", date: "2022-06-04 03:19:15 UTC", description: "Update the mold linker to 1.2 from 1.1", pr_number:                                                          12970, scopes: [], type:                                               "chore", breaking_change:       false, author: "Brian L. Troutwine", files_count:    3, insertions_count:   3, deletions_count:    4},
		{sha: "45fe0058ae8827b6b95c20eb034bc53c4bd24965", date: "2022-06-04 04:45:45 UTC", description: "Fix syslog soaks", pr_number:                                                                                12974, scopes: ["soaks"], type:                                        "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         2, insertions_count:   2, deletions_count:    16},
		{sha: "d88b4146534c7e875b941611ae463f0d8dc0bcbc", date: "2022-06-04 06:38:14 UTC", description: "Avoid sharing target with host system for integration tests", pr_number:                                     12977, scopes: ["tests"], type:                                        "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         29, insertions_count:  58, deletions_count:   4},
		{sha: "649c02aec5854acee4026867f4957ddfd6e6ebba", date: "2022-06-07 00:31:04 UTC", description: "bump memmap2 from 0.5.3 to 0.5.4", pr_number:                                                                12984, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    3},
		{sha: "e09d4fd7e6b194c8092fbd3af42b1d406c43d437", date: "2022-06-07 00:31:30 UTC", description: "bump tokio-stream from 0.1.8 to 0.1.9", pr_number:                                                           12985, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       3, insertions_count:   5, deletions_count:    5},
		{sha: "58bcc13b94c58b1cd058604236ade2e03cb6056f", date: "2022-06-06 21:49:24 UTC", description: "Fix deserialization of action", pr_number:                                                                   12979, scopes: ["tag_cardinality_limit transform"], type:              "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:         1, insertions_count:   1, deletions_count:    1},
		{sha: "a04bdf3c44f370aa1d69681d649b9c03a136accc", date: "2022-06-07 01:50:32 UTC", description: "bump mongodb from 2.2.1 to 2.2.2", pr_number:                                                                12986, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    4},
		{sha: "f44dff988b0da3a7e27fcba657ac3c6268050f77", date: "2022-06-06 22:51:29 UTC", description: "Remove `default_namespace` from example", pr_number:                                                         12971, scopes: ["datadog_metrics sink"], type:                         "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:         1, insertions_count:   3, deletions_count:    3},
		{sha: "354589cf0a11fb9e384622d60163e7b21fb05026", date: "2022-06-07 02:53:13 UTC", description: "Wrong config on monitoring", pr_number:                                                                      12899, scopes: ["external docs"], type:                                "docs", breaking_change:        false, author: "Bruno Mayer Paixão", files_count:    1, insertions_count:   1, deletions_count:    1},
		{sha: "44d54f349d8143394643c9c1fcdf28131e2b224d", date: "2022-06-07 07:26:07 UTC", description: "bump tokio from 1.18.2 to 1.19.1", pr_number:                                                                12987, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       8, insertions_count:   10, deletions_count:   10},
		{sha: "4c7ad2895db594295b2793c64e1e84c24a498ca4", date: "2022-06-07 03:25:38 UTC", description: "Support suppressing timestamp in splunk_hec_logs sink & defer to splunk when invalid timestamp ", pr_number: 12946, scopes: ["sinks"], type:                                        "feat", breaking_change:        false, author: "Kyle Criddle", files_count:          10, insertions_count:  175, deletions_count:  64},
		{sha: "d417613cbd29381b7c98ff8cc0b092e4ac90f6cc", date: "2022-06-07 05:34:00 UTC", description: "bump tokio from 1.19.1 to 1.19.2", pr_number:                                                                12998, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       8, insertions_count:   10, deletions_count:   10},
		{sha: "4531f67c5a8bc718958c93038c463cbb0c699821", date: "2022-06-07 11:46:29 UTC", description: "bump regex from 1.5.4 to 1.5.6 in /lib/value", pr_number:                                                    13002, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   752, deletions_count:  34},
		{sha: "14d771d1e6f784741291ef727255147f8eb2a3d4", date: "2022-06-07 09:41:09 UTC", description: "bump regex from 1.4.3 to 1.5.6 in /lib/vrl/proptests", pr_number:                                            13003, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   214, deletions_count:  202},
		{sha: "12815ec8b5f863796be8b0d2409124e78e8b5809", date: "2022-06-07 22:39:18 UTC", description: "fix if statement type definitions", pr_number:                                                               12954, scopes: ["vrl"], type:                                          "fix", breaking_change:         false, author: "Nathan Fox", files_count:            17, insertions_count:  258, deletions_count:  41},
		{sha: "d3015542e68e58a6eba9cd2509f55c806db639df", date: "2022-06-08 04:40:45 UTC", description: "improve precision of fallible expression diagnostic", pr_number:                                             12880, scopes: ["vrl"], type:                                          "enhancement", breaking_change: false, author: "Jean Mertz", files_count:            8, insertions_count:   161, deletions_count:  46},
		{sha: "a27268e290624183f9aa75b115e646cfcdb4ac71", date: "2022-06-07 23:21:22 UTC", description: "bump http from 0.2.7 to 0.2.8", pr_number:                                                                   13006, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       3, insertions_count:   4, deletions_count:    4},
		{sha: "818e01c51dfdac579895bd3c0b6e74ff9efccafd", date: "2022-06-08 06:23:57 UTC", description: "correctly diagnose invalid function argument types", pr_number:                                              12890, scopes: ["vrl"], type:                                          "enhancement", breaking_change: false, author: "Jean Mertz", files_count:            12, insertions_count:  193, deletions_count:  109},
		{sha: "87c81338a0d77df2ae9a35d06901402ada4fce3e", date: "2022-06-07 22:30:23 UTC", description: "Add file-to-blackhole soak test", pr_number:                                                                 12999, scopes: ["soak tests"], type:                                   "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         5, insertions_count:   46, deletions_count:   1},
		{sha: "ae63fb61adbdab943ddc6e5f5125d352b59100e0", date: "2022-06-08 01:08:43 UTC", description: "fix merge operator type def", pr_number:                                                                     12981, scopes: ["vrl"], type:                                          "fix", breaking_change:         false, author: "Nathan Fox", files_count:            5, insertions_count:   16, deletions_count:   17},
		{sha: "2dfa85b69def8492b4e5081bbfcc755107737957", date: "2022-06-08 00:25:35 UTC", description: "Fix handling of negative acknowledgements", pr_number:                                                       12901, scopes: ["kafka source"], type:                                 "fix", breaking_change:         false, author: "Bruce Guenter", files_count:         3, insertions_count:   145, deletions_count:  29},
		{sha: "2e929a103e7632399b517df16691648bbe9b8067", date: "2022-06-08 04:50:54 UTC", description: "fix Collection::merge to use unknown value", pr_number:                                                      12982, scopes: ["vrl"], type:                                          "fix", breaking_change:         false, author: "Nathan Fox", files_count:            2, insertions_count:   46, deletions_count:   10},
		{sha: "a26f14fb687f202b0ca13e2fd00f0f4fcae08888", date: "2022-06-08 07:33:20 UTC", description: "Add script to run vector with minimal features", pr_number:                                                  13000, scopes: ["dev"], type:                                          "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         2, insertions_count:   72, deletions_count:   0},
		{sha: "da11de593a7b3802c923c732725054e9ab2ee07f", date: "2022-06-08 21:53:15 UTC", description: "Fix handling of negative acknowledgements", pr_number:                                                       12913, scopes: ["journald source"], type:                              "fix", breaking_change:         false, author: "Bruce Guenter", files_count:         2, insertions_count:   82, deletions_count:   21},
		{sha: "4b74b0d045ab6017be7701e589aa6d195bbab0b7", date: "2022-06-09 00:46:24 UTC", description: "Simplify `ExternalEnv`", pr_number:                                                                          12983, scopes: ["vrl"], type:                                          "chore", breaking_change:       false, author: "Nathan Fox", files_count:            8, insertions_count:   79, deletions_count:   126},
		{sha: "c7af84db0898698de2b4edd01c84f022225e4626", date: "2022-06-09 05:36:20 UTC", description: "Remove `remove_empty` from `parse_grok`", pr_number:                                                         13008, scopes: ["deps"], type:                                         "enhancement", breaking_change: true, author:  "dependabot[bot]", files_count:       12, insertions_count:  56, deletions_count:   213},
		{sha: "1ba91c9133f0b382e84a774f7dac6b7bafb1320b", date: "2022-06-09 00:31:13 UTC", description: "Add end-to-end acknowledgement support", pr_number:                                                          13022, scopes: ["console sink"], type:                                 "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:         6, insertions_count:   42, deletions_count:   20},
		{sha: "465a09ba913b3de8cfef6b799f40ace83d35d2fa", date: "2022-06-09 08:15:47 UTC", description: "bump axum from 0.5.6 to 0.5.7", pr_number:                                                                   13036, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   5, deletions_count:    5},
		{sha: "bcb29183191e2597165f8015e3b02f26c3084933", date: "2022-06-09 02:21:33 UTC", description: "Move finalizers and shutdown into `vector-core`", pr_number:                                                 13035, scopes: [], type:                                               "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         19, insertions_count:  84, deletions_count:   97},
		{sha: "c3c2392738954e1e562f5ea37e9d5658d29d72bf", date: "2022-06-09 10:05:36 UTC", description: "bump actions/setup-python from 3 to 4", pr_number:                                                           13039, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   6, deletions_count:    6},
		{sha: "4c584221536be94bfb90ef2385c5628577c55ec0", date: "2022-06-09 05:28:20 UTC", description: "Fix handling of negative acknowledgements", pr_number:                                                       12936, scopes: ["file source"], type:                                  "fix", breaking_change:         false, author: "Bruce Guenter", files_count:         6, insertions_count:   275, deletions_count:  111},
		{sha: "dfc2e29c70caf27ffbb0a0e2fd90eaca091202d7", date: "2022-06-10 01:27:20 UTC", description: "bump mlua from 0.7.4 to 0.8.0", pr_number:                                                                   13051, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       4, insertions_count:   8, deletions_count:    8},
		{sha: "116d0b357924d44af11590b5917bf21d07b38a3e", date: "2022-06-10 03:22:30 UTC", description: "add `never` type", pr_number:                                                                                13024, scopes: ["vrl"], type:                                          "fix", breaking_change:         false, author: "Nathan Fox", files_count:            25, insertions_count:  252, deletions_count:  318},
		{sha: "b471333ea2b8a4fda05befaf6a84613e096ad7d5", date: "2022-06-10 12:38:08 UTC", description: "Integrate `encoding::Encoder` with `splunk_hec`/`humio_logs` sink", pr_number:                               12495, scopes: ["splunk_hec sink", "humio_logs sink", "codecs"], type: "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:         13, insertions_count:  294, deletions_count:  157},
		{sha: "22f853e9959851f86dceae0befeba3422ab18b5d", date: "2022-06-10 05:35:15 UTC", description: "integration test failures due to 'null' type or missing fields", pr_number:                                  13016, scopes: ["aws_ecs_metrics source"], type:                       "fix", breaking_change:         false, author: "Kyle Criddle", files_count:          1, insertions_count:   261, deletions_count:  204},
		{sha: "22b4e069f50d085f7d7bf93ef5259b0cc3fcc989", date: "2022-06-10 07:36:17 UTC", description: "initial integration of configuration schema for sources", pr_number:                                         13005, scopes: ["config"], type:                                       "chore", breaking_change:       false, author: "Toby Lawrence", files_count:         97, insertions_count:  2748, deletions_count: 567},
		{sha: "e855d81b7c0ce9725cb6399cc49ac5a483a1760b", date: "2022-06-10 08:28:13 UTC", description: "Add arbitrary metadata and secrets", pr_number:                                                              12767, scopes: ["core"], type:                                         "chore", breaking_change:       false, author: "Nathan Fox", files_count:            57, insertions_count:  1050, deletions_count: 371},
		{sha: "fd17b264b1b0b8dd4aa75e2c93acc48d7c01666a", date: "2022-06-10 17:28:15 UTC", description: "Compute APM stats", pr_number:                                                                               12806, scopes: ["datadog_traces sink"], type:                          "chore", breaking_change:       false, author: "prognant", files_count:              10, insertions_count:  877, deletions_count:  49},
		{sha: "f8bc670c1f9830f8f65a3c433539541453fa29a6", date: "2022-06-11 05:07:08 UTC", description: "some fixes to unit testing", pr_number:                                                                      13090, scopes: [], type:                                               "docs", breaking_change:        false, author: "Tshepang Mbambo", files_count:       1, insertions_count:   5, deletions_count:    5},
		{sha: "cc9775b3eaca2d5ed9f6c72744b269c00fc9b6ea", date: "2022-06-10 23:30:43 UTC", description: "Updated the splunk_hec_logs sink component documentation with timestamp_key", pr_number:                     13078, scopes: ["docs"], type:                                         "fix", breaking_change:         false, author: "Kyle Criddle", files_count:          1, insertions_count:   13, deletions_count:   0},
		{sha: "f3aa65603227b9b7ac245f1b49c68e781c3a4665", date: "2022-06-11 08:19:41 UTC", description: "Integrate `encoding::Encoder` with `gcp_pubsub` sink", pr_number:                                            12718, scopes: ["gcp_pubsub sink", "codecs"], type:                    "enhancement", breaking_change: true, author:  "Pablo Sichert", files_count:         3, insertions_count:   70, deletions_count:   34},
		{sha: "12c36d589e9507b909b1974c9226c1bbf754b79b", date: "2022-06-11 08:22:56 UTC", description: "Use `Transformer` in `logdna` sink and remove `EncodingConfigWithDefault`", pr_number:                       13065, scopes: ["logdna sink"], type:                                  "chore", breaking_change:       false, author: "Pablo Sichert", files_count:         1, insertions_count:   7, deletions_count:    15},
		{sha: "fcaa2e082f0f79c4ab1f0c501de471ec3d408e70", date: "2022-06-11 08:23:30 UTC", description: "Use `Transformer` in `datadog_archives`", pr_number:                                                         13062, scopes: ["datadog_archives sink"], type:                        "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:         1, insertions_count:   47, deletions_count:   24},
		{sha: "38dc130e753afd4b13c576aee279cccf24650c6d", date: "2022-06-11 08:23:53 UTC", description: "Use `Transformer` in `datadog_logs` and remove `EncodingConfigFixed`", pr_number:                            13061, scopes: ["datadog_logs sink"], type:                            "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:         2, insertions_count:   41, deletions_count:   32},
		{sha: "cac70ee50c54ea73fd3a30626a4080458aadfbbb", date: "2022-06-11 01:50:11 UTC", description: "allow the transform to be 'best effort'", pr_number:                                                         13093, scopes: ["aws_ec2_metadata transform"], type:                   "enhancement", breaking_change: false, author: "Kyle Criddle", files_count:          2, insertions_count:   45, deletions_count:   1},
		{sha: "8069866784096d377fdfa6d87284a1665b2f0eb8", date: "2022-06-11 02:28:30 UTC", description: "Update AWS SDK to 0.13", pr_number:                                                                          13097, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         2, insertions_count:   64, deletions_count:   64},
		{sha: "a928d1fd0b27266be51d2a7ffd395d5ac3bc2015", date: "2022-06-11 06:43:34 UTC", description: "Fix default namespace", pr_number:                                                                           13096, scopes: ["host_metrics source"], type:                          "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:       2, insertions_count:   2, deletions_count:    1},
		{sha: "1401a4193c7f86da863ddfd41d4cff3a93a9e344", date: "2022-06-13 23:54:31 UTC", description: "bump semver from 1.0.9 to 1.0.10", pr_number:                                                                13100, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   5, deletions_count:    5},
		{sha: "ca762dbad69ca03842286be6043db83c7353d615", date: "2022-06-13 23:55:01 UTC", description: "bump uuid from 1.1.1 to 1.1.2", pr_number:                                                                   13110, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   10, deletions_count:   10},
		{sha: "b3d64599aed332473c464277eac76349bf895a4b", date: "2022-06-13 23:55:24 UTC", description: "bump infer from 0.8.0 to 0.8.1", pr_number:                                                                  13113, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   4, deletions_count:    4},
		{sha: "d5c96f793910e724abee42acb354c3b9684868f5", date: "2022-06-13 23:56:09 UTC", description: "bump strum_macros from 0.24.0 to 0.24.1", pr_number:                                                         13111, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   4, deletions_count:    4},
		{sha: "e2d6e0dce68c8d47e9cc1191e18fca2c63720eaa", date: "2022-06-14 06:09:33 UTC", description: "Integrate `encoding::Encoder` with `websocket` sink", pr_number:                                             13054, scopes: ["websocket sink", "codecs"], type:                     "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:         3, insertions_count:   62, deletions_count:   45},
		{sha: "bd0bdcd9aeb28850cc8fe0cdd9042a172a9e107c", date: "2022-06-14 06:09:54 UTC", description: "Use `Transformer` instead of `EncodingConfigFixed` in `elasticsearch` and `sematext` sinks", pr_number:      13060, scopes: ["elasticsearch sink", "sematext sink"], type:          "chore", breaking_change:       false, author: "Pablo Sichert", files_count:         8, insertions_count:   80, deletions_count:   83},
		{sha: "c08ba52a56bfeaf81e7fff2aabcedfdb3061cb7c", date: "2022-06-14 06:10:55 UTC", description: "Use `Transformer` in `new_relic_logs` sink and remove `EncodingConfigWithDefault`", pr_number:               13067, scopes: ["new_relic_logs sink"], type:                          "chore", breaking_change:       false, author: "Pablo Sichert", files_count:         3, insertions_count:   66, deletions_count:   70},
		{sha: "9891ae24172663ffd26fe275688060f4219e73da", date: "2022-06-14 06:11:44 UTC", description: "Use `Transformer` in `stackdriver_logs` sink and remove `EncodingConfigWithDefault`", pr_number:             13081, scopes: ["gcp_stackdriver_logs sink"], type:                    "chore", breaking_change:       false, author: "Pablo Sichert", files_count:         1, insertions_count:   5, deletions_count:    13},
		{sha: "723b4992a51613d9f63afb7f8960f37b08fde8f9", date: "2022-06-14 06:12:06 UTC", description: "Use `Transformer` in `honeycomb` sink", pr_number:                                                           13089, scopes: ["honeycomb sink"], type:                               "enhancement", breaking_change: false, author: "Pablo Sichert", files_count:         1, insertions_count:   15, deletions_count:   3},
		{sha: "7a6588219e87fa71dde8b7cbc1a56e1750eb4d45", date: "2022-06-14 06:12:51 UTC", description: "Use `Transformer` in `new_relic_logs` sink and remove `EncodingConfigWithDefault`", pr_number:               13067, scopes: ["new_relic_logs sink"], type:                          "chore", breaking_change:       false, author: "Pablo Sichert", files_count:         0, insertions_count:   0, deletions_count:    0},
		{sha: "95272dcc8984161da8eae9134741179671b52b40", date: "2022-06-14 06:13:06 UTC", description: "Use `Transformer` in `azure_monitor_logs` sink and remove `EncodingConfigWithDefault`", pr_number:           13079, scopes: ["azure_monitor_logs sink"], type:                      "chore", breaking_change:       false, author: "Pablo Sichert", files_count:         1, insertions_count:   10, deletions_count:   10},
		{sha: "572c1bd9eeb7c24ae2681cc34e06833c501d2233", date: "2022-06-14 05:25:43 UTC", description: "bump strum from 0.24.0 to 0.24.1", pr_number:                                                                13112, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   3, deletions_count:    3},
		{sha: "2292965bf4450ab868f1bde87d1d5646dee3bcc9", date: "2022-06-14 01:21:32 UTC", description: "Fix application of `rustflags`", pr_number:                                                                  13074, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         5, insertions_count:   10, deletions_count:   13},
		{sha: "84c7bf56760f775b1c21e15170718edf65c137d3", date: "2022-06-14 03:37:39 UTC", description: "remove separate optional field from schema", pr_number:                                                      13103, scopes: ["vrl"], type:                                          "chore", breaking_change:       false, author: "Nathan Fox", files_count:            9, insertions_count:   118, deletions_count:  280},
		{sha: "a73415b6af8c7102c8f98d3b3a64667dee6de4f6", date: "2022-06-14 04:58:07 UTC", description: "Add negative index support to lookup v2", pr_number:                                                         13109, scopes: ["core"], type:                                         "feat", breaking_change:        false, author: "Nathan Fox", files_count:            15, insertions_count:  1081, deletions_count: 1011},
		{sha: "29826641ff3e38684e8aaa50a4991a77aba64db4", date: "2022-06-14 08:37:13 UTC", description: "run integration tests on beefier runners + some `runs-on` consolidation", pr_number:                         13126, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "Toby Lawrence", files_count:         14, insertions_count:  47, deletions_count:   47},
		{sha: "e2609bf5f3febcd8b0c7a9a0fb22cd570db5fcb8", date: "2022-06-14 06:57:47 UTC", description: "Move `shutdown` and event finalization to `vector-common`", pr_number:                                       13123, scopes: [], type:                                               "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         26, insertions_count:  119, deletions_count:  63},
		{sha: "ed2ebbcec45a3ce4df90f9bfa27826e4170dbdaf", date: "2022-06-14 08:11:45 UTC", description: "Use the mold wrapper script instead of linker flags", pr_number:                                             13125, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         3, insertions_count:   42, deletions_count:   21},
		{sha: "874f8662a3fd0666618fdba60cc105c5831c4547", date: "2022-06-14 11:20:30 UTC", description: "bump reqwest from 0.11.10 to 0.11.11", pr_number:                                                            13131, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       3, insertions_count:   6, deletions_count:    5},
		{sha: "39c5f13ac6956a577e3f333b54bc6b7ca2995fbc", date: "2022-06-14 11:22:38 UTC", description: "Upgrade Rust to 1.61.0", pr_number:                                                                          12812, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "Luke Steensen", files_count:         57, insertions_count:  173, deletions_count:  105},
		{sha: "a1c9358d659bf96571b9d1e0cda8e9b89a4b8dc8", date: "2022-06-14 23:37:47 UTC", description: "bump rust_decimal from 1.24.0 to 1.25.0", pr_number:                                                         13133, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   2, deletions_count:    2},
		{sha: "f6f38d08bd75ded69109a009ddc023e74525eb7b", date: "2022-06-15 05:07:54 UTC", description: "bump lru from 0.7.6 to 0.7.7", pr_number:                                                                    13140, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    3},
		{sha: "ce65b146c4b8d37c92a0a131c7c4b144b624eba2", date: "2022-06-15 03:25:30 UTC", description: "Add read-only support to `ExternalEnv`", pr_number:                                                          13108, scopes: ["vrl"], type:                                          "feat", breaking_change:        false, author: "Nathan Fox", files_count:            37, insertions_count:  591, deletions_count:  224},
		{sha: "4ef72fabcb43a9127a033afc1ae6abbd8aa54a3c", date: "2022-06-15 04:19:17 UTC", description: "Fix `serde` feature flag in `vector-common`", pr_number:                                                     13149, scopes: ["core"], type:                                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         2, insertions_count:   40, deletions_count:   34},
		{sha: "7cdb9c00eb1940ae8f26e8829b46f799850f4662", date: "2022-06-15 04:16:04 UTC", description: "Replace nextest installation", pr_number:                                                                    13069, scopes: ["tests"], type:                                        "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:         1, insertions_count:   1, deletions_count:    2},
		{sha: "4e6e957238d21af68d75c07cb5badeec1b66ca87", date: "2022-06-15 12:37:49 UTC", description: "Make the base `BatchNotifier` type shared", pr_number:                                                       13150, scopes: ["core"], type:                                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         16, insertions_count:  70, deletions_count:   67},
		{sha: "58f69f678eac5578265cf9f8eebbb53b786d1844", date: "2022-06-15 22:06:58 UTC", description: "avoid cloning event message in tracing-limit when rate limited", pr_number:                                  13155, scopes: ["core"], type:                                         "chore", breaking_change:       false, author: "Toby Lawrence", files_count:         1, insertions_count:   82, deletions_count:   65},
		{sha: "83c384f7aca1a0b32696e8314540d60fd21c62c9", date: "2022-06-16 07:15:10 UTC", description: "update x86_64_unknown-linux-gnu to use ubuntu not rhel", pr_number:                                          13166, scopes: [], type:                                               "chore", breaking_change:       false, author: "Stephen Wakely", files_count:        1, insertions_count:   2, deletions_count:    6},
		{sha: "871833db71d4760fc82c3cc67d8f7323ad170f46", date: "2022-06-16 02:07:22 UTC", description: "remove unused rhel bootstrap script", pr_number:                                                             13167, scopes: [], type:                                               "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         1, insertions_count:   0, deletions_count:    9},
		{sha: "8aa0e2f939d370f156260ca5b2315f4f916caa64", date: "2022-06-16 06:42:32 UTC", description: "bump kube to 0.73.1 and k8s-openapi to 0.15", pr_number:                                                     13170, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:       4, insertions_count:   18, deletions_count:   36},
		{sha: "88981f3e531b81da76b67b7ffca7e6be1d89a53d", date: "2022-06-16 11:23:26 UTC", description: "bump arbitrary from 1.1.0 to 1.1.1", pr_number:                                                              13159, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   2, deletions_count:    2},
		{sha: "1b097b6a0fda5f84a48ee827f8d870047e95ce90", date: "2022-06-16 05:49:54 UTC", description: "Remove splunk_transforms_splunk3 soak test", pr_number:                                                      13173, scopes: [], type:                                               "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         4, insertions_count:   0, deletions_count:    160},
		{sha: "83ef151308b73d4a0c365bd3555fd2218203c3f8", date: "2022-06-16 09:04:07 UTC", description: "drop unneeded dependencies and coalesce some others", pr_number:                                             13172, scopes: ["dev"], type:                                          "chore", breaking_change:       false, author: "Toby Lawrence", files_count:         13, insertions_count:  128, deletions_count:  292},
		{sha: "422d4c64f488719ea9ae95f825ba041500d01ad6", date: "2022-06-17 06:07:00 UTC", description: "Use `Transformer` in `clickhouse` sink and remove `EncodingConfigWithDefault`", pr_number:                   13063, scopes: ["clickhouse sink"], type:                              "chore", breaking_change:       false, author: "Pablo Sichert", files_count:         1, insertions_count:   8, deletions_count:    20},
		{sha: "8b9a91e3b7c32bdd0e167275cc502f5fdc3a8f60", date: "2022-06-16 23:50:18 UTC", description: "Fix no_run doc comment fences", pr_number:                                                                   13179, scopes: [], type:                                               "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         3, insertions_count:   20, deletions_count:   4},
		{sha: "e87af396353dbcd960155768acee555527b5ae68", date: "2022-06-17 00:03:21 UTC", description: "Fail on docs warnings too", pr_number:                                                                       13184, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         1, insertions_count:   1, deletions_count:    0},
		{sha: "89426dc41159ed6ea43b2f15d1f799034f334833", date: "2022-06-17 03:52:24 UTC", description: "avoid panicking on recoverable disk_v2 reader errors", pr_number:                                            13180, scopes: ["buffers"], type:                                      "fix", breaking_change:         false, author: "Toby Lawrence", files_count:         14, insertions_count:  207, deletions_count:  111},
		{sha: "35448da66b36b752121240c6597b23e8bb8eafd9", date: "2022-06-17 01:54:44 UTC", description: "Parse namespace out of incoming events", pr_number:                                                          13176, scopes: ["datadog_agent source"], type:                         "feat", breaking_change:        false, author: "Kyle Criddle", files_count:          2, insertions_count:   55, deletions_count:   8},
		{sha: "4cc6209a8f1d23e8127388ff944bea9423466018", date: "2022-06-17 05:10:15 UTC", description: "fix type def for the `parse_aws_cloudwatch_log_subscription_message` function", pr_number:                   13189, scopes: ["vrl"], type:                                          "fix", breaking_change:         false, author: "Nathan Fox", files_count:            1, insertions_count:   2, deletions_count:    2},
		{sha: "ff86f221049156297230650929a75666aef58973", date: "2022-06-17 05:19:29 UTC", description: "bump no-proxy from 0.3.1 to 0.3.2", pr_number:                                                               13191, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    3},
		{sha: "2fddd2bfaa6faf632223f91238c8e0a2c0600a32", date: "2022-06-17 05:19:49 UTC", description: "bump crossbeam-utils from 0.8.8 to 0.8.9", pr_number:                                                        13190, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       4, insertions_count:   6, deletions_count:    6},
		{sha: "2d6529e997788da8d9bc1d43618b393ad58eb9a8", date: "2022-06-17 04:03:45 UTC", description: "Fail warnings", pr_number:                                                                                   13194, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         35, insertions_count:  50, deletions_count:   1},
		{sha: "79ed386aa2c50efdbc639c740211941497a65ba3", date: "2022-06-17 07:09:58 UTC", description: "try and consolidate nightly/release workflows", pr_number:                                                   13146, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "Toby Lawrence", files_count:         8, insertions_count:   864, deletions_count:  1324},
		{sha: "e2850b6f6a539d37a89b14148011aed2641a1134", date: "2022-06-17 06:14:50 UTC", description: "Drop use of `num_cpus` crate", pr_number:                                                                    13197, scopes: ["core"], type:                                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         8, insertions_count:   12, deletions_count:   7},
		{sha: "a89bd362b840acd4a727bffc255647697ecc4f23", date: "2022-06-17 05:28:21 UTC", description: "Fix whitespace for release.yml workflow", pr_number:                                                         13199, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         1, insertions_count:   1, deletions_count:    1},
		{sha: "7963df6afc182b1e72764fcb84fcd53ca81cb8df", date: "2022-06-17 09:17:29 UTC", description: "Remove xtrace from rustc wrapper", pr_number:                                                                13204, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         1, insertions_count:   0, deletions_count:    1},
		{sha: "c8014f49b819b13df2ae884d45314497bea55b6d", date: "2022-06-17 23:00:32 UTC", description: "improve del return type", pr_number:                                                                         13201, scopes: ["vrl"], type:                                          "feat", breaking_change:        false, author: "Nathan Fox", files_count:            3, insertions_count:   21, deletions_count:   11},
		{sha: "c3c40ee5bff832ebed411e55e7c50f0785ed8a48", date: "2022-06-17 21:52:20 UTC", description: "Mark (almost) all optional dependencies as non-features", pr_number:                                         13195, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         12, insertions_count:  178, deletions_count:  190},
		{sha: "a687b8bc2104188d98603dc320e9247b88028caf", date: "2022-06-18 14:20:56 UTC", description: "Append (rather than prepend) bin dir to $PATH", pr_number:                                                   13182, scopes: ["releasing"], type:                                    "enhancement", breaking_change: false, author: "Mike Bailey", files_count:           1, insertions_count:   2, deletions_count:    1},
		{sha: "ef9225f73bf4de48ade56d9c6456017c25778c51", date: "2022-06-18 00:47:53 UTC", description: "fix `Kind::reduced_kind` when `known` is empty", pr_number:                                                  13208, scopes: ["vrl"], type:                                          "fix", breaking_change:         false, author: "Nathan Fox", files_count:            3, insertions_count:   121, deletions_count:  12},
		{sha: "fb7d4fcbbdb3a33255cf6317f0a22cb6274d43dc", date: "2022-06-18 07:11:30 UTC", description: "bump openssl-src from 111.18.0+1.1.1n to 111.20.0+1.1.1o", pr_number:                                        13214, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   2, deletions_count:    2},
		{sha: "c91f91ca3a5b4831bfe7b3268772100a798f2adb", date: "2022-06-18 03:13:26 UTC", description: "don't emit BufferEventsDropped event unless count is non-zero", pr_number:                                   13213, scopes: ["buffers"], type:                                      "chore", breaking_change:       false, author: "Toby Lawrence", files_count:         2, insertions_count:   169, deletions_count:  126},
		{sha: "ce08bc85beb327bb1f12503c64a821a1dd67f284", date: "2022-06-18 10:11:55 UTC", description: "update file location", pr_number:                                                                            13218, scopes: [], type:                                               "docs", breaking_change:        false, author: "Tshepang Mbambo", files_count:       1, insertions_count:   1, deletions_count:    1},
		{sha: "d4700a07a9a3194cb2b1cf6d9cbc37131e7e7e7c", date: "2022-06-18 16:54:13 UTC", description: "fix parse_int(\"0\")", pr_number:                                                                            13216, scopes: ["vrl"], type:                                          "fix", breaking_change:         false, author: "Xiaonan Shen", files_count:          1, insertions_count:   22, deletions_count:   16},
		{sha: "f51d92a4f5338a266df59e13cd2e207caa9a3a7c", date: "2022-06-18 04:34:18 UTC", description: "Fix handling of streaming pull requests", pr_number:                                                         13203, scopes: ["gcp_pubsub source"], type:                            "fix", breaking_change:         false, author: "Bruce Guenter", files_count:         1, insertions_count:   50, deletions_count:   31},
		{sha: "52bad15b848f03db83212007f692e4edecc73a9a", date: "2022-06-18 13:14:29 UTC", description: "avoid a possible panic", pr_number:                                                                          13206, scopes: [], type:                                               "chore", breaking_change:       false, author: "Tshepang Mbambo", files_count:       3, insertions_count:   11, deletions_count:   8},
		{sha: "fc6d0db5459d290abb7f0239e8e798547d29b12d", date: "2022-06-18 04:29:48 UTC", description: "Group and tag component feature check output", pr_number:                                                    13217, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         1, insertions_count:   1, deletions_count:    1},
		{sha: "b0b52b31d74fb7e85657eaac736751707ec79fad", date: "2022-06-18 14:22:02 UTC", description: "enable raw event endpoint", pr_number:                                                                       13041, scopes: ["splunk_hec_logs sink"], type:                         "enhancement", breaking_change: false, author: "Stephen Wakely", files_count:        17, insertions_count:  440, deletions_count:  81},
		{sha: "f296e018ec9077f28b912ab0f9928388e9e9b5fa", date: "2022-06-18 07:01:22 UTC", description: "Remove build info fields from enterprise logs", pr_number:                                                   13223, scopes: ["observability"], type:                                "chore", breaking_change:       false, author: "Will", files_count:                  1, insertions_count:   0, deletions_count:    7},
		{sha: "73c8f418e36a3b3cc8cdbb815a953d4dba7d3db9", date: "2022-06-22 00:00:27 UTC", description: "bump anyhow from 1.0.57 to 1.0.58", pr_number:                                                               13232, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    3},
		{sha: "daccffce7b5128bee4a44ade81b155881ac0b424", date: "2022-06-22 00:03:46 UTC", description: "bump axum from 0.5.7 to 0.5.9", pr_number:                                                                   13248, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   5, deletions_count:    5},
		{sha: "8f0a1d9b2302592383aa6eef09b8f928372a8905", date: "2022-06-22 00:06:55 UTC", description: "Add `null` to type when merging with missing field", pr_number:                                              13164, scopes: ["vrl"], type:                                          "fix", breaking_change:         false, author: "Nathan Fox", files_count:            17, insertions_count:  244, deletions_count:  204},
		{sha: "cc9790031c59d1e4757c5f4279c96cbf38b5cd1f", date: "2022-06-21 21:57:35 UTC", description: "Remove the `status` field footgun for logs.", pr_number:                                                     13219, scopes: ["datadog_agent source"], type:                         "fix", breaking_change:         false, author: "Ari", files_count:                   2, insertions_count:   9, deletions_count:    11},
		{sha: "cb0f64f1582243dfcb3a905feb30936e1db0b56b", date: "2022-06-22 07:07:42 UTC", description: "clean up op expression", pr_number:                                                                          13252, scopes: ["vrl"], type:                                          "chore", breaking_change:       false, author: "Jean Mertz", files_count:            1, insertions_count:   16, deletions_count:   13},
		{sha: "a987ef312079f9d477f902c7220ed1681e727ec7", date: "2022-06-22 05:26:14 UTC", description: "bump quote from 1.0.18 to 1.0.19", pr_number:                                                                13254, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   2, deletions_count:    2},
		{sha: "bd27e01ec7fae314749f1c92bb289d049e40aed8", date: "2022-06-22 12:14:53 UTC", description: "add support for providing features list for make build", pr_number:                                          13226, scopes: ["dev"], type:                                          "fix", breaking_change:         false, author: "Mikhail Antoshkin", files_count:     1, insertions_count:   7, deletions_count:    7},
		{sha: "d2a87847462b8c079a7fe6b1369891336161e913", date: "2022-06-22 01:01:43 UTC", description: "Update k8s manifests to v0.22.2", pr_number:                                                                 13261, scopes: ["releasing"], type:                                    "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         17, insertions_count:  21, deletions_count:   21},
		{sha: "589bcb116c042aae7284e55b44b9879e428ab6f6", date: "2022-06-22 09:13:40 UTC", description: "bump tower from 0.4.12 to 0.4.13", pr_number:                                                                13231, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    3},
		{sha: "6e65c4ae971c2b6d2059bf12a64fb47176726771", date: "2022-06-22 11:14:03 UTC", description: "Update Pulsar sink doc", pr_number:                                                                          13116, scopes: ["doc"], type:                                          "fix", breaking_change:         false, author: "Collignon-Ducret Rémi", files_count: 3, insertions_count:   5, deletions_count:    2},
		{sha: "7d9f9b51d1a7bd53d99c5a5dfa99a0d128956468", date: "2022-06-22 09:56:16 UTC", description: "bump proc-macro2 from 1.0.39 to 1.0.40", pr_number:                                                          13259, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   2, deletions_count:    2},
		{sha: "79507195cd5d3bf056140eaec41d96cc1e5fd3a0", date: "2022-06-22 04:20:08 UTC", description: "Add an inactivity timeout keepalive", pr_number:                                                             13224, scopes: ["gcp_pubsub source"], type:                            "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:         2, insertions_count:   46, deletions_count:   11},
		{sha: "0844b1b74ab3a54aab99a37ecec04989baf46fc9", date: "2022-06-22 06:58:18 UTC", description: "Add end-to-end acknowledgement support", pr_number:                                                          13147, scopes: ["nats sink"], type:                                    "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count:       2, insertions_count:   46, deletions_count:   18},
		{sha: "f36eb7d21f3bd68ab191bc8aa4b19e21040c2e88", date: "2022-06-22 11:22:59 UTC", description: "bump syn from 1.0.96 to 1.0.98", pr_number:                                                                  13274, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   2, deletions_count:    2},
		{sha: "b28a85b484719413a00bab175e9dcb71d9073748", date: "2022-06-22 04:59:06 UTC", description: "Revert remove the `status` field footgun for logs.", pr_number:                                              13262, scopes: ["datadog_agent source"], type:                         "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:         2, insertions_count:   11, deletions_count:   9},
		{sha: "31bcfcff7c51c660d0d0b6176c0a007706b90f0f", date: "2022-06-22 12:54:03 UTC", description: "bump quote from 1.0.19 to 1.0.20", pr_number:                                                                13278, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   2, deletions_count:    2},
		{sha: "e95c5e2caf925d1a67ea40680eb8701d5a5f927a", date: "2022-06-22 07:02:58 UTC", description: "Rename `_seconds` settings to `_secs`", pr_number:                                                           13279, scopes: ["gcp_pubsub source"], type:                            "chore", breaking_change:       false, author: "Bruce Guenter", files_count:         2, insertions_count:   62, deletions_count:   17},
		{sha: "0882956b14b39bce625ffff5eb38874505417feb", date: "2022-06-22 06:11:44 UTC", description: "bump jpeg-js from 0.4.3 to 0.4.4 in /website", pr_number:                                                    13207, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   3, deletions_count:    3},
		{sha: "96f7bbbf51e12ddb59949bdba33aa19b21315284", date: "2022-06-22 13:24:21 UTC", description: "bump indexmap from 1.8.2 to 1.9.1", pr_number:                                                               13272, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       7, insertions_count:   9, deletions_count:    9},
		{sha: "bd0127319c916c6c86c3af713b2aab30d141ba98", date: "2022-06-22 14:15:24 UTC", description: "bump graphql_client from 0.10.0 to 0.11.0", pr_number:                                                       13273, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   11, deletions_count:   78},
		{sha: "0994616f8f5dbe79782853afe6113baed3449d23", date: "2022-06-22 14:26:06 UTC", description: "bump clap from 3.1.18 to 3.2.6", pr_number:                                                                  13258, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       5, insertions_count:   15, deletions_count:   15},
		{sha: "c19a6f11002f804f0805f615b8f0b6c869501fbc", date: "2022-06-22 14:36:01 UTC", description: "bump nats from 0.20.1 to 0.21.0", pr_number:                                                                 13212, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   3, deletions_count:    3},
		{sha: "06c1b9dde6cf3f6cd6e295a5fea60184f554d9af", date: "2022-06-22 07:55:39 UTC", description: "bump dyn-clone from 1.0.5 to 1.0.6", pr_number:                                                              13281, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       6, insertions_count:   7, deletions_count:    7},
		{sha: "00d9de7d50335f84b6bbddad64180c3f8f9dc690", date: "2022-06-22 07:55:59 UTC", description: "bump arbitrary from 1.1.1 to 1.1.2", pr_number:                                                              13202, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   4, deletions_count:    4},
		{sha: "ca0315c1a8467373c0be315f3957f62485139ec4", date: "2022-06-23 00:44:53 UTC", description: "Defer finalization until after processing", pr_number:                                                       13276, scopes: ["prometheus_exporter sink"], type:                     "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count:       1, insertions_count:   4, deletions_count:    2},
		{sha: "607a89384c17d3e63da78b9185b64841c3501e7a", date: "2022-06-23 00:55:28 UTC", description: "Remove deprecated aws_cloudwatch_subscription_parser page", pr_number:                                       13174, scopes: [], type:                                               "docs", breaking_change:        false, author: "Spencer Gilbert", files_count:       6, insertions_count:   735, deletions_count:  699},
		{sha: "91d88e8540bfe154038ca79265d28b31e6242c05", date: "2022-06-23 04:25:08 UTC", description: "remove standalone test targets to speed up CI", pr_number:                                                   13251, scopes: ["ci"], type:                                           "chore", breaking_change:       false, author: "Toby Lawrence", files_count:         23, insertions_count:  2009, deletions_count: 2152},
		{sha: "aa2d251950610a6b5810f857430cd73d10ccd9a1", date: "2022-06-24 00:31:42 UTC", description: "Improve loki label documentation", pr_number:                                                                13287, scopes: ["loki sink"], type:                                    "docs", breaking_change:        false, author: "Spencer Gilbert", files_count:       1, insertions_count:   33, deletions_count:   4},
		{sha: "96973fa2445eb02ed6761bac3f3dc3cf8e78e5ae", date: "2022-06-24 12:55:52 UTC", description: "fix transform name in yaml and json example config", pr_number:                                              13294, scopes: ["external docs"], type:                                "fix", breaking_change:         false, author: "WangSiyuan", files_count:            1, insertions_count:   2, deletions_count:    2},
		{sha: "a7f848f89bf5d73028f75eeb9e3490f045e4e6a9", date: "2022-06-24 01:57:53 UTC", description: "Add end-to-end acknowledgement support", pr_number:                                                          13289, scopes: ["websocket sink"], type:                               "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count:       3, insertions_count:   27, deletions_count:   5},
		{sha: "c57848af1a6cb30b3156f112a7a4c4a6defc5544", date: "2022-06-23 23:18:49 UTC", description: "Revert integrate `encoding::Encoder` with `pulsar` sink", pr_number:                                         13307, scopes: ["pulsar sink", "codecs"], type:                        "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:         34, insertions_count:  234, deletions_count:  251},
		{sha: "f45171bf399df61e77d9f7c6a722bd23da1e0ea0", date: "2022-06-24 01:07:18 UTC", description: "retry HTTP requests and improve datadog sink error handling consistency", pr_number:                         13130, scopes: ["datadog_logs sink"], type:                            "fix", breaking_change:         false, author: "Kyle Criddle", files_count:          5, insertions_count:   145, deletions_count:  76},
		{sha: "56f4fb259ca05bde3be415758213afca5b4f1802", date: "2022-06-24 15:41:09 UTC", description: "add oauth2 and event time support for pulsar sink", pr_number:                                               10463, scopes: ["pulsar sink"], type:                                  "enhancement", breaking_change: false, author: "Yang Yang", files_count:             5, insertions_count:   215, deletions_count:  145},
		{sha: "205db4c7e96230eaa15141bf86fc2172291b77a1", date: "2022-06-24 01:15:39 UTC", description: "Upgrade AWS SDK to 0.14.0", pr_number:                                                                       13304, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         2, insertions_count:   63, deletions_count:   63},
		{sha: "ccc43db9fe7cd5081f09c8a87196120904b87f0b", date: "2022-06-24 10:26:43 UTC", description: "bump crossbeam-utils from 0.8.9 to 0.8.10", pr_number:                                                       13309, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       4, insertions_count:   5, deletions_count:    5},
		{sha: "4972d4d770db3457ff7a9b7e30e2157374c9b8e0", date: "2022-06-24 11:50:53 UTC", description: "bump arbitrary from 1.1.2 to 1.1.3", pr_number:                                                              13313, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   4, deletions_count:    4},
		{sha: "96e11b7ae6d194c66a21e2056d4e048214967b4a", date: "2022-06-25 06:03:34 UTC", description: "rustfmt", pr_number:                                                                                         13319, scopes: ["vrl"], type:                                          "chore", breaking_change:       false, author: "Jean Mertz", files_count:            18, insertions_count:  47, deletions_count:   49},
		{sha: "85a99e5097fb5397d394770ae43fee878e01d779", date: "2022-06-24 22:34:34 UTC", description: "Add automatic concurrency scaling", pr_number:                                                               13240, scopes: ["gcp_pubsub source"], type:                            "enhancement", breaking_change: false, author: "Bruce Guenter", files_count:         3, insertions_count:   281, deletions_count:  101},
		{sha: "d084afd9bf6c36bdaafebfe9e2ac27f69ec0ce12", date: "2022-06-25 02:31:06 UTC", description: "Integrate `encoding::Encoder` with `pulsar` sink", pr_number:                                                13308, scopes: ["pulsar sink", "codecs"], type:                        "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count:         33, insertions_count:  274, deletions_count:  250},
		{sha: "754d52bf0cf713c975676dbae2f017654a795a95", date: "2022-06-25 03:02:21 UTC", description: "Reexport TimeZone type", pr_number:                                                                          13326, scopes: ["vrl"], type:                                          "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         2, insertions_count:   2, deletions_count:    2},
		{sha: "05dc779b44358b2ba321dbaa16fbb2ade5ea4654", date: "2022-06-28 05:08:48 UTC", description: "clippy deny+fix on compiler", pr_number:                                                                     13321, scopes: ["vrl"], type:                                          "chore", breaking_change:       false, author: "Jean Mertz", files_count:            28, insertions_count:  261, deletions_count:  123},
		{sha: "a9ed919a45bd1877cc8c3e66f9b19679c341059f", date: "2022-06-28 05:16:21 UTC", description: "some readability improvements", pr_number:                                                                   13334, scopes: [], type:                                               "docs", breaking_change:        false, author: "Tshepang Mbambo", files_count:       1, insertions_count:   8, deletions_count:    7},
		{sha: "e8079b0188722c18ff62eb5a8096cb5735d164a4", date: "2022-06-27 23:18:42 UTC", description: "bump smallvec from 1.8.0 to 1.8.1", pr_number:                                                               13328, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       1, insertions_count:   2, deletions_count:    2},
		{sha: "edc01eb1ef6afdfb07bed1989bc34b2f3407d243", date: "2022-06-27 23:20:06 UTC", description: "bump rkyv from 0.7.38 to 0.7.39", pr_number:                                                                 13341, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "dependabot[bot]", files_count:       2, insertions_count:   5, deletions_count:    5},
		{sha: "0ce740945c9aff1157b72be08b8754f979a6ce57", date: "2022-06-27 22:22:11 UTC", description: "`website/content/en/guides` readability improvements", pr_number:                                            13336, scopes: ["guides"], type:                                       "docs", breaking_change:        false, author: "Ryan Russell", files_count:          4, insertions_count:   6, deletions_count:    6},
		{sha: "541142d589221a15e6e0846657c5259191608dae", date: "2022-06-27 22:27:01 UTC", description: "Apply `api_key` configuration to all GCP components", pr_number:                                             13324, scopes: ["gcp service"], type:                                  "fix", breaking_change:         false, author: "Bruce Guenter", files_count:         14, insertions_count:  260, deletions_count:  195},
		{sha: "9c984313b3d0c928c2c27f674fe05e2cab237545", date: "2022-06-27 22:42:47 UTC", description: "Clarify `group_by`", pr_number:                                                                              13330, scopes: ["reduce"], type:                                       "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:         1, insertions_count:   11, deletions_count:   3},
		{sha: "de0b33007694c810ee91d693d6867b9b1381560a", date: "2022-06-27 22:50:07 UTC", description: "Fix parsing of user for parse_nginx_log", pr_number:                                                         13332, scopes: ["vrl stdlib"], type:                                   "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:         3, insertions_count:   3, deletions_count:    3},
		{sha: "5ff0e18e75c9b07a376787adb72c8dc394ccec81", date: "2022-06-28 08:04:50 UTC", description: "clippy deny+fix on parser", pr_number:                                                                       13343, scopes: ["vrl"], type:                                          "chore", breaking_change:       false, author: "Jean Mertz", files_count:            4, insertions_count:   132, deletions_count:  140},
		{sha: "25676e6c7b0af51ff29b61ed116fb29db9e2d9e7", date: "2022-06-27 23:11:38 UTC", description: "Re-add `vector vrl`", pr_number:                                                                             13347, scopes: ["cli"], type:                                          "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:         1, insertions_count:   13, deletions_count:   13},
		{sha: "47fd4dc69354ead5ac10df69380404a14d724a53", date: "2022-06-28 09:01:37 UTC", description: "Remove `encoding` option from `humio_metrics` sink", pr_number:                                              13327, scopes: ["humio_metrics sink"], type:                           "enhancement", breaking_change: true, author:  "Pablo Sichert", files_count:         5, insertions_count:   31, deletions_count:   25},
		{sha: "a52f4de191f2fb58799a4631421e63a7ac71014b", date: "2022-06-28 03:02:53 UTC", description: "Make `parse_syslog` and the `syslog` source handle structured data consistently", pr_number:                 12433, scopes: ["vrl", "syslog source"], type:                         "fix", breaking_change:         true, author:  "Jesse Szwedko", files_count:         9, insertions_count:   171, deletions_count:  44},
		{sha: "7d93baaf57e584ebed3c4ba1e1e3ecdf5d48c067", date: "2022-06-28 11:37:44 UTC", description: "Brings dnstap.proto up to date with upstream", pr_number:                                                    13227, scopes: ["dnstap source"], type:                                "fix", breaking_change:         false, author: "franklymrshankley", files_count:     2, insertions_count:   76, deletions_count:   13},
		{sha: "fad66db7979d5870c402aea9a146be4fc2c0c85d", date: "2022-06-29 10:02:46 UTC", description: "Support v2 endpoint for series ", pr_number:                                                                 13028, scopes: ["datadog_agent source"], type:                         "enhancement", breaking_change: false, author: "prognant", files_count:              8, insertions_count:   450, deletions_count:  58},
		{sha: "904c1e1795e5b68427d0a635c16f1917db4be281", date: "2022-06-29 02:24:17 UTC", description: "Prevent inconsistent configuration hashes", pr_number:                                                       13355, scopes: ["enterprise"], type:                                   "fix", breaking_change:         false, author: "Will", files_count:                  3, insertions_count:   192, deletions_count:  51},
		{sha: "47771e6e483ce4dee15c51a3e7010a68b53d9475", date: "2022-06-30 06:06:08 UTC", description: "proper namespace parsing for v2 series endpoint", pr_number:                                                 13376, scopes: ["datadog_agent source"], type:                         "enhancement", breaking_change: false, author: "prognant", files_count:              2, insertions_count:   13, deletions_count:   5},
		{sha: "d97cb1e7c3b18a805a165c94f610dd406d65db9f", date: "2022-07-01 07:00:16 UTC", description: "add highlight about removal of VRL VM runtime", pr_number:                                                   13091, scopes: ["external docs"], type:                                "docs", breaking_change:        false, author: "Jean Mertz", files_count:            1, insertions_count:   8, deletions_count:    0},
		{sha: "b1b41ab75a87c176ec37073702ad037241acb97d", date: "2022-07-01 02:32:16 UTC", description: "Update kubernetes api access requirements", pr_number:                                                       13395, scopes: ["kubernetes_logs source"], type:                       "docs", breaking_change:        false, author: "Spencer Gilbert", files_count:       1, insertions_count:   4, deletions_count:    4},
		{sha: "762b0e614ccc5d4b2a9b5c7e1e1faf17ec1d7093", date: "2022-07-01 12:18:40 UTC", description: "keep the enterprise section when merging configurations", pr_number:                                         13302, scopes: ["config"], type:                                       "fix", breaking_change:         false, author: "Jérémie Drouet", files_count:        1, insertions_count:   55, deletions_count:   2},
		{sha: "7de5db5818841d51ea3617f2540cd8bfc0c63f56", date: "2022-07-01 05:50:13 UTC", description: "Bump AWS SDK to 0.15.0", pr_number:                                                                          13401, scopes: ["deps"], type:                                         "chore", breaking_change:       false, author: "Jesse Szwedko", files_count:         2, insertions_count:   65, deletions_count:   66},
		{sha: "0ad9a3aa2bc6342b31f5888aab2efd0411912d5a", date: "2022-07-02 03:17:24 UTC", description: "Expand typedef updates upgrade guide section", pr_number:                                                    13413, scopes: ["docs"], type:                                         "chore", breaking_change:       false, author: "Nathan Fox", files_count:            1, insertions_count:   47, deletions_count:   1},
		{sha: "ea170f1b8b3dc66af0b84bda8a8b0780bb4eaea4", date: "2022-07-02 03:49:21 UTC", description: "tighten up logic around not overrunning maximum buffer size", pr_number:                                     13356, scopes: ["buffers"], type:                                      "fix", breaking_change:         false, author: "Toby Lawrence", files_count:         19, insertions_count:  893, deletions_count:  274},
		{sha: "90df3359b80827fd7e1894c2852f1f4a8eb626d2", date: "2022-07-02 01:51:17 UTC", description: "Correct the documentatation of API key handling", pr_number:                                                 13416, scopes: ["datadog provider"], type:                             "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:         5, insertions_count:   15, deletions_count:   28},
		{sha: "be38265ba1e3e5e066c9684e97afe86614885a59", date: "2022-07-02 12:33:29 UTC", description: "Document changes to sink encoding", pr_number:                                                               13331, scopes: ["codecs", "sinks"], type:                              "docs", breaking_change:        false, author: "Pablo Sichert", files_count:         17, insertions_count:  198, deletions_count:  73},
		{sha: "749de22a240d4a1fdff59d4fd90b1b9bed052f46", date: "2022-07-06 12:25:30 UTC", description: "Fix superfluous `flatten` attribute in `encoding` key", pr_number:                                           13445, scopes: ["websocket sink", "pulsar sink"], type:                "fix", breaking_change:         false, author: "Pablo Sichert", files_count:         2, insertions_count:   0, deletions_count:    2},
		{sha: "cd05453b66b1e854823ee729dc523ec6ce725921", date: "2022-07-08 04:40:59 UTC", description: "Restrict sink input types", pr_number:                                                                       13418, scopes: ["sinks", "codecs"], type:                              "fix", breaking_change:         false, author: "Jesse Szwedko", files_count:         20, insertions_count:  69, deletions_count:   42},
		{sha: "37a21f804a9ced497eff1b8e75269b368b4fad1a", date: "2022-07-08 05:22:30 UTC", description: "Add highlight for secrets management", pr_number:                                                            13473, scopes: ["config"], type:                                       "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:         2, insertions_count:   90, deletions_count:   5},
		{sha: "c5f32d9f4d1c88cb0f42bfa7ded4f0b4ba4c6d24", date: "2022-07-08 05:22:38 UTC", description: "Add release highlight for sink codecs", pr_number:                                                           13475, scopes: ["config", "codecs"], type:                             "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:         1, insertions_count:   36, deletions_count:   0},
		{sha: "b1fd67bce8f46cd6f74ded98667181db1096436d", date: "2022-07-09 03:09:19 UTC", description: "Fix 0.23 and 0.24 upgrade guides", pr_number:                                                                13479, scopes: [], type:                                               "chore", breaking_change:       false, author: "Spencer Gilbert", files_count:       1, insertions_count:   1, deletions_count:    1},
		{sha: "20b1ac8e210de526d16af264151db09f79c61c0c", date: "2022-07-09 04:16:01 UTC", description: "Correct sink codec highlight and document length_delimited framing", pr_number:                              13481, scopes: ["codecs"], type:                                       "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:         3, insertions_count:   7, deletions_count:    7},
		{sha: "6008560d44c384d3800ddda89e48c83d65462e08", date: "2022-07-01 01:16:57 UTC", description: "Handle signal overflows", pr_number:                                                                         13241, scopes: ["reload", "shutdown"], type:                           "fix", breaking_change:         false, author: "Will Jordan", files_count:           2, insertions_count:   12, deletions_count:   8},
		{sha: "778d45b8b958d31c13a7aa3a9beaa3c46db74532", date: "2022-07-01 13:45:51 UTC", description: "remove repeated text", pr_number:                                                                            13391, scopes: [], type:                                               "docs", breaking_change:        false, author: "Tshepang Mbambo", files_count:       2, insertions_count:   0, deletions_count:    11},
		{sha: "81fe171a1531a6b5fc792d6c373a894f10a6fd8e", date: "2022-07-06 00:16:38 UTC", description: "Update concurrency documentation", pr_number:                                                                13442, scopes: [], type:                                               "docs", breaking_change:        false, author: "Jesse Szwedko", files_count:         1, insertions_count:   9, deletions_count:    9},
		{sha: "9ddf026b25e4be75f411bb6a2632d945ce22fa64", date: "2022-07-07 04:08:31 UTC", description: "clarify TLS cert validations", pr_number:                                                                    13449, scopes: ["external docs"], type:                                "chore", breaking_change:       false, author: "Kyle Criddle", files_count:          1, insertions_count:   1, deletions_count:    1},
		{sha: "8df13fca81004796ff8f7c479af8950461af2ca3", date: "2022-07-08 09:23:12 UTC", description: "revert kafka tls", pr_number:                                                                                13465, scopes: ["docs"], type:                                         "fix", breaking_change:         false, author: "Maksim Nabokikh", files_count:       2, insertions_count:   12, deletions_count:   2},
	]
}
