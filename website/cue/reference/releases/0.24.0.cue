package metadata

releases: "0.24.0": {
	date:     "2022-08-29"
	codename: ""

	whats_next: [
		{
			title: "Expanded OpenTelemetry support"
			description: """
				This release ships with initial OpenTelemetry support in the form of an
				`opentelemetry` source for consuming logs from OpenTelemetry collectors and SDKs.
				This support will be expanded to cover metrics and traces as well as an
				`opentelemetry` sink.
				"""
		},
	]

	known_issues: [
		"""
			The new `host_metrics` metrics for physical and logical CPU counts were incorrectly
			implemented as new modes for the `cpu_seconds_total` when they were meant to be new gauges.
			Fixed in 0.24.1.
			""",
		"""
			`vector top` and some sinks like `file` incorrectly report metrics from the
			`internal_metrics` source as they show the incremental metrics rather than absolute.
			Fixed in 0.24.1.
			""",
		"""
			The `expire_metrics_secs` option added in this release was not correctly applied. Fixed
			in 0.24.2.
			""",
		"""
			Supplying an empty string (`""`) for options that take a field name started panicking in
			0.24.0 rather than disabling the option as it previously did. Fixed in 0.24.2.
			""",
		"""
			This release was intended to add support for sending rate metrics to the
			`datadog_metrics` sink, but there was a regression in it prior to release. Fixed in
			0.24.2.
			""",
		"""
			VRL code using closures sometimes returned an incorrect type error ("block returns
			invalid value type"). Fixed in 0.24.2.
			""",
	]

	description: """
		The Vector team is pleased to announce version 0.24.0!

		Be sure to check out the [upgrade guide](/highlights/2022-08-16-0-24-0-upgrade-guide) for
		breaking changes in this release.

		In addition to the new features, enhancements, and fixes listed below, this release adds:

		- A new `axiom` sink for sending events to Axiom
		- A new `gcp_chronicle_unstructured` sink for sending unstructured log events to GCP Chronicle
		- A new `file_descriptor` source to consume input from file descriptors
		- A new `opentelemetry` source to receive input from OpenTelemetry collectors and SDKs. Only
		  logs are supported in this release, but support for metrics and traces are in-flight.
		  An `opentelemetry` sink will follow.
		- Support for expiring high cardinality internal metrics through the global `expire_metrics`
		  (will be replaced by `expire_metrics_secs` in 0.24.1). This can alleviate issues with
		  Vector using increased memory over time. For now it is opt-in, but we may make this the
		  default in the future.

		Note that this release has a backwards incompatible data model change that users of the
		`vector` sink and disk buffers should be aware of while upgrading. See the [note in the
		upgrade guide](/highlights/2022-08-16-0-24-0-upgrade-guide#metric-buckets) for more
		details.
		"""

	changelog: [
		{
			type: "enhancement"
			scopes: ["vrl", "vrl: compiler"]
			description: """
				The VRL compiler now rejects assignments to fields on values known not to be
				objects or arrays. For example, this now fails:

				```coffeescript
				foo = 1
				foo.bar = 2
				```

				Where previously it would overwrite the value of `1` with `{ "bar": 2 }`. This was
				done to alleviate accidental assignments. You can still assign like:

				```
				foo = 1
				foo = {}
				foo.bar = 2
				```
				"""
			pr_numbers: [13317]
		},
		{
			type: "enhancement"
			scopes: ["prometheus_exporter sink"]
			description: """
				The `prometheus_exporter` sink now has a `suppress_timestamp` option to avoid adding
				the timestamp to exposed metrics.
				"""
			contributors: ["shenxn"]
			pr_numbers: [13337]
		},
		{
			type: "enhancement"
			scopes: ["prometheus_scrape source"]
			description: """
				The `prometheus_scrape` source now uses unsigned 64-bit integers for histogram
				buckets, allowing it to avoid errors when scraping endpoints that have buckets with
				very counts that didn't fit in an unsigned 32-bit integer.
				"""
			pr_numbers: [13318]
		},
		{
			type: "enhancement"
			scopes: ["vrl", "vrl: stdlib"]
			description: """
				A `filter` function was added to VRL to allow easy removal of keys from objects or
				elements from arrays. It can be used like:

				```coffeescript
				.kubernetes.pod_annotations = filter(.kubernetes.pod_annotations) { |key, _value|
					!starts_with(key, "checksum")
				}
				```
				"""
			pr_numbers: [13411]
		},
		{
			type: "feat"
			scopes: ["sinks"]
			description: """
				A new `axiom` sink was added for sending data to [Axiom](https://www.axiom.co/).
				"""
			contributors: ["bahlo"]
			pr_numbers: [13007]
		},
		{
			type: "feat"
			scopes: ["codecs"]
			description: """
				A new `gelf` codec was added for decoding/encoding
				[GELF](https://docs.graylog.org/docs/gelf) data in Vector's sources and sinks. It
				can be used via `encoding.codec` on sinks and `decoding.codec` on sources, for
				those that support codecs.
				"""
			pr_numbers: [13228, 13333]
		},
		{
			type: "feat"
			scopes: ["enrichment tables"]
			description: """
				A new enrichment table type was added,
				[`geoip`](/docs/reference/configuration/global-options/#enrichment_tables.geoip).
				This can be used with [VRL's enrichment table
				functions](/docs/reference/vrl/functions/#enrichment-functions) to enrich events
				using a [GeoIP database](https://www.maxmind.com/en/geoip2-databases).

				Additionally the `geoip` enrichment table has support for `Connection-Type`
				databases.

				This takes the place of the `geoip`, which has been deprecated.
				"""
			contributors: ["ktff", "w4"]
			pr_numbers: [13338, 13707]
		},
		{
			type: "fix"
			scopes: ["reload"]
			description: """
				Vector no longer panics when reloading a config where a component is deleted
				and then re-added.
				"""
			contributors: ["zhongzc"]
			pr_numbers: [13375]
		},
		{
			type: "chore"
			scopes: ["codecs"]
			breaking: true
			description: """
				The deprecated codec configuration for sinks was removed so that codecs must be
				specified as `encoding.codec` rather than just `encoding`. See [the upgrade note](
				/highlights/2022-08-16-0-24-0-upgrade-guide#sink-encoding-codec) for details.
				"""
			pr_numbers: [13518]
		},
		{
			type: "feat"
			scopes: ["sources"]
			description: """
				A new [`opentelemetry` source](/docs/reference/configuration/sources/opentelemetry/)
				was added to ingest logs from the OpenTelemetry collector and OpenTelemetry SDKs.

				We will be following with support for ingesting metrics and traces.
				"""
			contributors: ["caibirdme"]
			pr_numbers: [13320]
		},
		{
			type: "fix"
			scopes: ["elasticsearch sink"]
			description: """
				The `elasticsearch` sink now sends any configured headers when making healthchecks.
				"""
			pr_numbers: [13572]
		},
		{
			type: "fix"
			scopes: ["datadog_agent source", "datadog_metrics sink", "metrics"]
			description: """
				Vector now has a concept of "rate" metrics which are a counter with an additional
				interval associated. This is used by the `datadog_agent` source the
				`datadog_metrics` sink to correctly pass "rate" metrics from the Datadog Agent to
				Datadog.
				"""
			pr_numbers: [13394]
		},
		{
			type: "enhancement"
			scopes: ["vrl", "vrl: stdlib"]
			description: """
				VRL's `flatten` function now takes an optional parameter, `separator`, to configure
				the separator to use when flattening keys. This defaults to `.`, preserving the
				current behavior.
				"""
			contributors: ["trennepohl"]
			pr_numbers: [13618]
		},
		{
			type: "fix"
			scopes: ["vrl", "vrl: stdlib"]
			description: """
				VRL's `parse_apache_log` function now handles additional error log formats.
				"""
			pr_numbers: [13581]
		},
		{
			type: "fix"
			scopes: ["mongodb_metrics source"]
			description: """
				The `mongodb_metrics` source now correctly decodes `bytes_written_from` values that
				exceed 32-bit integers, up to 64-bit.
				"""
			contributors: ["KernelErr"]
			pr_numbers: [13656]
		},
		{
			type: "enhancement"
			scopes: ["sample transform"]
			description: """
				The `sample` transform can now sample trace events.
				"""
			pr_numbers: [13610]
		},
		{
			type: "enhancement"
			scopes: ["sources"]
			description: """
				A new
				[`gcp_chronicle_unstructured`](/docs/reference/configuration/sinks/gcp_chronicle_unstructured/)
				sink was added to send log events to [GCP
				Chronicle](https://cloud.google.com/chronicle/docs/overview) as unstructured
				events. We expect to support UDM events in the future.
				"""
			pr_numbers: [13550]
		},
		{
			type: "enhancement"
			scopes: ["vrl", "vrl: compiler"]
			breaking: true
			description: """
				A number of VRL type definition bugs have been resolved to allow VRL to more
				precisely know the types of values and fields in more places. In general, this means
				less need for type coercion functions like `string()`.

				See [the release
				highlight](/highlights/2022-08-16-0-24-0-upgrade-guide#vrl-type-def) for more
				details.
				"""
			pr_numbers: [13619]
		},
		{
			type: "fix"
			scopes: ["enrichment tables"]
			description: """
				Vector no longer panics after reloading an existing enrichment table that had index
				updates.
				"""
			pr_numbers: [13704]
		},
		{
			type: "enhancement"
			scopes: ["host_metrics source"]
			description: """
				The `host_metrics` source now emits `mode=iowait` for `host_cpu_seconds_total`.
				"""
			contributors: ["charmitro"]
			pr_numbers: [13696]
		},
		{
			type: "fix"
			scopes: ["host_metrics source"]
			description: """
				The `host_metrics` source has improved handling of fetching cgroups metrics from
				hybrid cgroups. It now checks all possible locations for stats.
				"""
			pr_numbers: [13719]
		},
		{
			type: "enhancement"
			scopes: ["prometheus_remote_write sink", "papertrail sink", "prometheus_exporter sink", "socket sink", "pulsar sink"]
			description: """
				End-to-end acknowledgement support has been added to the following sinks:

				- `papertrail`
				- `socket`
				- `pulsar`
				- `prometheus_exporter`
				- `prometheus_remote_write`
				"""
			pr_numbers: [13741, 13744, 13750, 13755]
		},
		{
			type: "feat"
			scopes: ["apex sink"]
			description: """
				A new `apex` sink was added to send logs to Apex.
				This sink was removed in v0.28 as the Apex service moved to EOL.
				"""
			contributors: ["mcasper"]
			pr_numbers: [13436]
		},
		{
			type: "enhancement"
			scopes: ["aws provider"]
			description: """
				AWS components now allow specifying a region to use when assuming a role
				via `auth.region`. By default, this will use the same region that the component is
				configured to use via `region`.
				"""
			contributors: ["akutta"]
			pr_numbers: [13838]
		},
		{
			type: "fix"
			scopes: ["http sink"]
			description: """
				Certificate verification now correctly works for proxied connections (when `proxy`
				is configured on a component).
				"""
			contributors: ["ntim"]
			pr_numbers: [13759]
		},
		{
			type: "enhancement"
			scopes: ["vrl", "vrl: stdlib"]
			description: """
				Two new functions were added to the VRL standard library to ease detecting whether
				an IP address is IPv4 or IPv6:`is_ipv4` and `is_ipv6`.
				"""
			pr_numbers: [13852]
		},
		{
			type: "enhancement"
			scopes: ["sources", "observability"]
			description: """
				A new internal metric, `source_lag_time_seconds`, was added for sources that is
				a histogram of the time difference of when Vector ingests an event and the timestamp
				of the event itself (if it exists).
				"""
			pr_numbers: [13611]
		},
		{
			type: "enhancement"
			scopes: ["internal_metrics source"]
			description: """
				The `internal_metrics` source now emits an `internal_metrics_cardinality` gauge
				indicating the cardinality of the internal metric store.

				Previously we emitted `internal_metrics_cardinality_total` but this metric is
				a counter and so cannot account for metrics being dropped from the internal metric
				store. `internal_metrics_cardinality_total` has been deprecated and will be removed
				in a future release.
				"""
			pr_numbers: [13854]
		},
		{
			type: "fix"
			scopes: ["docker_logs source"]
			description: """
				The `docker_logs` source now correctly tags internal metrics that it emits with the
				normal component tags (`component_id`, `component_kind`, and `component_type`).
				"""
			contributors: ["zhongzc"]
			pr_numbers: [13868]
		},
		{
			type: "fix"
			scopes: ["host_metrics source"]
			description: """
				The `host_metrics` source now emits two new gauges for the number of CPUs on the
				host: `physical_cpus` and `logical_cpus`. These can be used to better interpret the
				`load*` metrics that are emitted.
				"""
			pr_numbers: [13874]
		},
		{
			type: "enhancement"
			scopes: ["websocket sink"]
			description: """
				The `websocket` sink now allows configuration of the same authentication settings
				the `http` sink does via the new `auth` configuration option.
				"""
			contributors: ["wez470"]
			pr_numbers: [13632]
		},
		{
			type: "enhancement"
			scopes: ["observability"]
			description: """
				Vector's internal metrics store, which is exposed via the `internal_metrics`
				source, now allows for expiration of metrics via a new configurable global
				`expire_metrics` configuration option. When set, the store will drop metrics that
				haven't been seen in the configured duration. This can be used to expire metrics
				from the `kubernetes_logs` source, and others, which tag their internal metrics with
				high cardinality, but ephemeral, tags like `file`.
				"""
			pr_numbers: [13872]
		},
		{
			type: "fix"
			scopes: ["exec source"]
			description: """
				The `exec` source now gracefully waits for the subprocess to exit when shutting down
				by sending a SIGTERM. This has only been implemented for *nix hosts. On Windows, the
				subprocess will be abruptly killed. We hope to improve this in the future.
				"""
			pr_numbers: [11907]
		},
		{
			type: "feat"
			scopes: ["sources"]
			description: """
				A new [`file_descriptor`
				source](/docs/reference/configuration/sources/file_descriptor/) was added to
				read events from [file descriptors](https://en.wikipedia.org/wiki/File_descriptor).
				"""
			contributors: ["mcasper"]
			pr_numbers: [13389]
		},
		{
			type: "fix"
			scopes: ["sources"]
			description: """
				The `file`, `journald`, and `kafka` sources no longer halt when end-to-end
				acknowledgements are enabled and an attached sink returns an error. This was
				a change in v0.23.0, but we backed it out to pursue improved error handling in
				sinks.
				"""
			pr_numbers: [14135]
		},
	]

	commits: [
		{sha: "bda9b86ee78863ed36e33b76808ba9aafc9aa678", date: "2022-06-28 04:05:24 UTC", description: "Defer finalization until after the write", pr_number: 13348, scopes: ["pulsar sink"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 12, deletions_count: 6},
		{sha: "666b7bfccc2af166bfa456cf45e43f33c136f864", date: "2022-06-28 04:15:51 UTC", description: "Note acknowledgement support as true", pr_number: 13351, scopes: ["prometheus_remote_write sink"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "dc5f262da10eec9aafbcf3ecd8ce8c941c0c0b55", date: "2022-06-28 08:33:45 UTC", description: "bump styfle/cancel-workflow-action from 0.9.1 to 0.10.0", pr_number: 13350, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "8ac20b8791cf884ba34737c75bb9dd288b0a63e9", date: "2022-06-28 14:24:07 UTC", description: "clippy deny+fix on cli", pr_number: 13344, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Jean Mertz", files_count: 3, insertions_count: 33, deletions_count: 26},
		{sha: "4b59750386be87bf426106180e14c04bd819873b", date: "2022-06-28 14:24:19 UTC", description: "clippy deny+fix on core", pr_number: 13345, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Jean Mertz", files_count: 3, insertions_count: 27, deletions_count: 15},
		{sha: "6f7bbbaf29d5575f6b77afa46140ee5781358593", date: "2022-06-29 00:01:30 UTC", description: "Add coalescing support to lookup v2", pr_number: 13156, scopes: ["core"], type: "feat", breaking_change: false, author: "Nathan Fox", files_count: 8, insertions_count: 593, deletions_count: 36},
		{sha: "2658b1dda6690f02086b7310cbe5c5fc84b6d93d", date: "2022-06-29 06:28:42 UTC", description: "clippy deny+fix on stdlib", pr_number: 13362, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Jean Mertz", files_count: 45, insertions_count: 161, deletions_count: 142},
		{sha: "b01ac16ff79be73c12fc9ada5609700a5919eef5", date: "2022-06-29 06:47:36 UTC", description: "reject field/index assignment for non-container types", pr_number: 13317, scopes: ["vrl"], type: "enhancement", breaking_change: false, author: "Jean Mertz", files_count: 4, insertions_count: 343, deletions_count: 3},
		{sha: "f262fea3cbf09ab8bfa1bcc4937964310d4b54e0", date: "2022-06-29 02:14:24 UTC", description: "remove deprecated transforms", pr_number: 13315, scopes: ["transforms"], type: "chore", breaking_change: true, author: "Toby Lawrence", files_count: 94, insertions_count: 751, deletions_count: 7020},
		{sha: "2886176691ef13b6062fa3856fc8450371669a53", date: "2022-06-29 02:52:38 UTC", description: "Drop `Event::new_empty_log`", pr_number: 13368, scopes: [], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 22, insertions_count: 150, deletions_count: 178},
		{sha: "dc960e175b90abcf2c53670ef38fae29e891198b", date: "2022-06-29 04:17:20 UTC", description: "remove `Event::from(Bytes)`", pr_number: 13370, scopes: [], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 10, deletions_count: 19},
		{sha: "1216c2db559ceda75df92e0f7701183ff2628951", date: "2022-06-29 07:46:59 UTC", description: "initial integration of configuration schema for transforms", pr_number: 13311, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 52, insertions_count: 1673, deletions_count: 813},
		{sha: "7a2cb87578960ff1af3409540270ff2999ea24a5", date: "2022-06-29 07:20:39 UTC", description: "Remove `Event::from(BTreeMap)` and `Event::from(HashMap)`", pr_number: 13372, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 9, insertions_count: 79, deletions_count: 90},
		{sha: "30a35197de89dfe82ddd8b9a3d8db20b2e44bf3a", date: "2022-06-29 13:25:44 UTC", description: "bump axum from 0.5.9 to 0.5.10", pr_number: 13373, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a1c0e843bf46295fcd8f8e358b5f1efc294a0208", date: "2022-06-30 03:33:03 UTC", description: "Add an option to allow suppressing timestamp", pr_number: 13337, scopes: ["prometheus_exporter sink"], type: "feat", breaking_change: false, author: "Xiaonan Shen", files_count: 3, insertions_count: 45, deletions_count: 2},
		{sha: "add4842bf7925b611bd05b2f1576b556352cea70", date: "2022-06-30 01:36:15 UTC", description: "Remove `Event::from(&str)`", pr_number: 13374, scopes: [], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 53, insertions_count: 229, deletions_count: 225},
		{sha: "8f8008da3e2eb0f5b5a148a0eb1fc689d5e5d614", date: "2022-06-30 00:51:56 UTC", description: "bump clap from 3.2.6 to 3.2.7", pr_number: 13367, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 12, deletions_count: 12},
		{sha: "aa9850500195a488d55eb19602a5bd92580f6b0b", date: "2022-06-30 03:03:49 UTC", description: "Fix running config::enterprise tests", pr_number: 13369, scopes: [], type: "chore", breaking_change: false, author: "Will", files_count: 6, insertions_count: 179, deletions_count: 132},
		{sha: "7e384e1a71b841c98b7f6ecfb2abbde3d23722d3", date: "2022-06-30 08:51:51 UTC", description: "Remove `Event::from(String)`", pr_number: 13387, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 20, insertions_count: 104, deletions_count: 80},
		{sha: "28a6b3b6d25a92404aaa0d3eeb02ef688cb2106c", date: "2022-06-30 22:35:11 UTC", description: "clippy deny+fix on diagnostic", pr_number: 13346, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Jean Mertz", files_count: 7, insertions_count: 72, deletions_count: 20},
		{sha: "c715f5fcbe21c06c1810c7410cc8781c61696719", date: "2022-07-01 06:59:40 UTC", description: "add note about function fallibility", pr_number: 12903, scopes: ["external docs"], type: "docs", breaking_change: false, author: "Jean Mertz", files_count: 3, insertions_count: 70, deletions_count: 5},
		{sha: "090e72ef8b37ff27846adfed66a63385bcac58c2", date: "2022-07-01 13:05:21 UTC", description: "make clear immediately that it's global", pr_number: 13400, scopes: [], type: "docs", breaking_change: false, author: "Tshepang Mbambo", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a4ca029eaea4f3a77fa45e2f48b030ee90cc7a87", date: "2022-07-01 14:22:23 UTC", description: "bump smallvec from 1.8.1 to 1.9.0", pr_number: 13402, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "96a16a847c453d4b9d46d737b20abdeb4b8f24d4", date: "2022-07-01 22:50:05 UTC", description: "bump semver from 1.0.10 to 1.0.12", pr_number: 13407, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "49f00a5fef618df34734b43a4be13a8c50b5e775", date: "2022-07-01 22:53:39 UTC", description: "bump serde_json from 1.0.81 to 1.0.82", pr_number: 13403, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 11, deletions_count: 11},
		{sha: "a82b641abbb163543211fb4d2206bca87120a470", date: "2022-07-02 05:02:55 UTC", description: "add note about VRL REPL to diagnostic messages", pr_number: 13409, scopes: ["vrl"], type: "enhancement", breaking_change: false, author: "Jean Mertz", files_count: 58, insertions_count: 101, deletions_count: 5},
		{sha: "2e53c93938f10af9d8d9936887349d8e6b0b3f32", date: "2022-07-01 23:49:54 UTC", description: "support coalescing for read only paths", pr_number: 13253, scopes: ["vrl"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 6, insertions_count: 69, deletions_count: 14},
		{sha: "b576f1abce4770cd8840e8861f912a0649265202", date: "2022-07-02 01:38:29 UTC", description: "bump mlua from 0.8.0 to 0.8.1", pr_number: 13384, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "dc26ec3cc13f4fbe94e0b5585a58d0529c4f9d8b", date: "2022-07-03 03:23:04 UTC", description: "update bucket size to u64", pr_number: 13318, scopes: ["prometheus_scrape source"], type: "enhancement", breaking_change: false, author: "Stephen Wakely", files_count: 2207, insertions_count: 1904, deletions_count: 70},
		{sha: "2ff02ee37ecca7a655894386add441224b5ee285", date: "2022-07-04 23:37:54 UTC", description: "Use event finalization for acking disk buffers", pr_number: 13396, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 28, insertions_count: 395, deletions_count: 238},
		{sha: "e14b2c1796d35118eef967bce1a94d38df9958f6", date: "2022-07-05 02:53:44 UTC", description: "Fix `cargo test --lib --package vector_common`", pr_number: 13430, scopes: ["tests"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "1eb416d6a4f48f46b10abc2981d73050db91aaff", date: "2022-07-06 05:18:22 UTC", description: "improve performance of `if-predicate` expression", pr_number: 12683, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Jean Mertz", files_count: 1, insertions_count: 6, deletions_count: 5},
		{sha: "9f0f5a7dc5019ff0190a2ea1231133b27aa17b4a", date: "2022-07-06 06:28:08 UTC", description: "add `filter` enumeration function", pr_number: 13411, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Jean Mertz", files_count: 7, insertions_count: 222, deletions_count: 17},
		{sha: "aaa7497055f67bd8996313d216fd9712fde32b3e", date: "2022-07-06 01:09:59 UTC", description: "bump onig from 6.3.1 to 6.3.2", pr_number: 13427, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "5e1111c15663884897e1bc49684c68ae01e7b85e", date: "2022-07-06 01:10:26 UTC", description: "bump infer from 0.8.1 to 0.9.0", pr_number: 13424, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "92c475d2c3ffd84e3c9c4ce8cc80f74cc8d7ada1", date: "2022-07-06 01:10:51 UTC", description: "bump pin-project from 1.0.10 to 1.0.11", pr_number: 13426, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 8, deletions_count: 8},
		{sha: "56adeca93320ca79e578a51a180da2cfbe02a356", date: "2022-07-06 01:11:22 UTC", description: "bump tracing-subscriber from 0.3.11 to 0.3.14", pr_number: 13425, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 35, deletions_count: 35},
		{sha: "3dcc6bf278a76d6693290afdd5f0a38ebe6b6ec5", date: "2022-07-06 07:14:39 UTC", description: "Add Axiom sink", pr_number: 13007, scopes: ["sink"], type: "feat", breaking_change: false, author: "Arne Bahlo", files_count: 12, insertions_count: 644, deletions_count: 1},
		{sha: "be0a3571b2437b6bc23133633b8e1c630b583358", date: "2022-07-05 23:34:18 UTC", description: "Flatten out `test` library", pr_number: 13432, scopes: ["buffers"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 22, insertions_count: 23, deletions_count: 254},
		{sha: "63875d560315f3ecc831a5ae86d160ddef8cb614", date: "2022-07-06 05:21:22 UTC", description: "bump axum from 0.5.10 to 0.5.11", pr_number: 13440, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "1099646c4de3580d3ee288831461fcd4d3c4476d", date: "2022-07-06 06:17:46 UTC", description: "bump crossterm from 0.23.2 to 0.24.0", pr_number: 13439, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 19, deletions_count: 3},
		{sha: "b343393c03b2b4f6ed2e1bcecaa433102608dcb5", date: "2022-07-06 12:38:41 UTC", description: "bump once_cell from 1.12.0 to 1.13.0", pr_number: 13437, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 10, deletions_count: 10},
		{sha: "2b2259ce2a966296efdca8e9ec99591dde683f62", date: "2022-07-06 14:09:05 UTC", description: "bump regex from 1.5.6 to 1.6.0", pr_number: 13448, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 9, deletions_count: 9},
		{sha: "6500cc07e5217f136fa4bcdfeaec04ab55c90447", date: "2022-07-07 06:01:39 UTC", description: "add highlight about VRL assignment breaking change", pr_number: 13412, scopes: ["external docs"], type: "docs", breaking_change: false, author: "Jean Mertz", files_count: 2, insertions_count: 64, deletions_count: 4},
		{sha: "d1076909d84ff72b707b9f5f861af48dde1d82a3", date: "2022-07-06 23:24:02 UTC", description: "Implement GELF decoder", pr_number: 13288, scopes: ["codecs", "sources"], type: "feat", breaking_change: false, author: "Kyle Criddle", files_count: 9, insertions_count: 463, deletions_count: 8},
		{sha: "5232a05032b709c6951c901c1fcab4f6abedc4b7", date: "2022-07-07 06:26:42 UTC", description: "updated highlights regarding u64 bucket counts.", pr_number: 13429, scopes: [], type: "docs", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 40, deletions_count: 0},
		{sha: "8cbcd2b48fb8582c4cb19d1db98900e41ef96c59", date: "2022-07-07 00:57:31 UTC", description: "add makefile target check-deny", pr_number: 13446, scopes: ["dev"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 4, insertions_count: 21, deletions_count: 18},
		{sha: "2d62397824c9a0ccaf6d678e66b63a4f44bfe4e4", date: "2022-07-07 00:02:32 UTC", description: "bump clap from 3.2.7 to 3.2.8", pr_number: 13404, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 10, deletions_count: 10},
		{sha: "54b432b47e41a78db77ece4d377eb395c82faade", date: "2022-07-07 08:34:51 UTC", description: "bump serde from 1.0.137 to 1.0.138", pr_number: 13438, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "ab2c0730cf2c64f9a48ef23f9a1734004008d820", date: "2022-07-07 23:14:31 UTC", description: "bump criterion from 0.3.5 to 0.3.6", pr_number: 13464, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 14, deletions_count: 8},
		{sha: "aee1984c15ff8e43992ce80fc4f3f1b95220f4fc", date: "2022-07-08 06:55:04 UTC", description: "Add `geoip` enrichment table", pr_number: 13338, scopes: ["vrl"], type: "enhancement", breaking_change: false, author: "Kruno Tomola Fabro", files_count: 6, insertions_count: 609, deletions_count: 11},
		{sha: "b12b72358a8954cd0dcde5ae3e2836176ee5f36f", date: "2022-07-08 00:50:24 UTC", description: "delete Cargo.locks & compiler/fuzz", pr_number: 13468, scopes: ["dev"], type: "chore", breaking_change: false, author: "Kyle Criddle", files_count: 6, insertions_count: 0, deletions_count: 4435},
		{sha: "18432ef398d5c3179db96edf2a5b7824cdedadda", date: "2022-07-08 03:16:54 UTC", description: "Fully qualify vector-dev docker image", pr_number: 13469, scopes: [], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "10996fa2ef2238a51df84f83f52bbf97a77b05e8", date: "2022-07-08 04:15:26 UTC", description: "bump bollard from 0.12.0 to 0.13.0", pr_number: 13255, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 7, deletions_count: 9},
		{sha: "d9b06441d95b4155c05662e3d2768ccbf8dc005f", date: "2022-07-08 13:32:31 UTC", description: "bump hyper from 0.14.19 to 0.14.20", pr_number: 13476, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "0126ab3c9921fb49a6eb75c315cc6a45441888c6", date: "2022-07-09 02:58:43 UTC", description: "Upgrade Rust to 1.62.0", pr_number: 13477, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 45, insertions_count: 109, deletions_count: 125},
		{sha: "dc910bb4cbac2361f4a0fa8f99b4892f974c20f9", date: "2022-07-09 04:56:31 UTC", description: "Reimplement finalizer stream using `async-stream`", pr_number: 13463, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 42, deletions_count: 56},
		{sha: "ab7a75cceed030feaca216a0e8d08a11bcf4cc6e", date: "2022-07-09 13:39:26 UTC", description: "add explicit support for traces", pr_number: 13466, scopes: ["pipelines transform"], type: "enhancement", breaking_change: false, author: "prognant", files_count: 3, insertions_count: 76, deletions_count: 0},
		{sha: "613114ad4e4aaa3be4ab81319eb93fcaed0e9119", date: "2022-07-09 06:43:20 UTC", description: "bump serde_yaml from 0.8.24 to 0.8.25", pr_number: 13482, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7fc33f306c4c710a41aa6bd58f05be33aca03514", date: "2022-07-09 07:16:15 UTC", description: "bump typetag from 0.1.8 to 0.2.0", pr_number: 13405, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 10, deletions_count: 10},
		{sha: "17557722b20f81a11722af5ce9224644751a14da", date: "2022-07-11 23:22:40 UTC", description: "Add log namespacing to datadog agent logs source", pr_number: 12218, scopes: ["sources"], type: "feat", breaking_change: false, author: "Nathan Fox", files_count: 100, insertions_count: 1560, deletions_count: 795},
		{sha: "8cb271efa282c96687982d5e076c233c6bd86957", date: "2022-07-12 04:06:14 UTC", description: "fix datadog agent integration test", pr_number: 13502, scopes: ["datadog_agent source"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "497690963a5311f460bd4de1bd3b953f54e5afe9", date: "2022-07-12 03:28:39 UTC", description: "bump serde from 1.0.138 to 1.0.139", pr_number: 13499, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "4baab65efdc3228693dc8d79528144120080196d", date: "2022-07-12 03:32:39 UTC", description: "bump axum from 0.5.11 to 0.5.12", pr_number: 13498, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "1c7162fa6f70a1f613e4791d79c6a585fbd30487", date: "2022-07-12 03:43:53 UTC", description: "bump typetag from 0.2.0 to 0.2.1", pr_number: 13496, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "70a31c104180a14bc9f52ccb996fcadeae1ae440", date: "2022-07-12 03:44:47 UTC", description: "bump memmap2 from 0.5.4 to 0.5.5", pr_number: 13495, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "08b9aa32a56d26e9d65a47f8a848491d51f6fffd", date: "2022-07-12 03:45:46 UTC", description: "bump nats from 0.21.0 to 0.22.0", pr_number: 13494, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c04140879934e42ee568d7b96eae35b73484fee5", date: "2022-07-12 04:58:07 UTC", description: "bump openssl from 0.10.40 to 0.10.41", pr_number: 13497, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "8d484b244ba500abcd18ccc1778fbcfa109bc088", date: "2022-07-12 19:19:43 UTC", description: "don't panic when re-add input", pr_number: 13375, scopes: ["topology"], type: "fix", breaking_change: false, author: "Zhenchi", files_count: 2, insertions_count: 69, deletions_count: 10},
		{sha: "cee613ece620a9a61652096c00fabc730ca19667", date: "2022-07-12 05:15:58 UTC", description: "Bump Cargo.toml versions from release", pr_number: 13506, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 2, deletions_count: 2},
		{sha: "8a7ffafd447426f55001afede795367777ba441b", date: "2022-07-12 08:04:35 UTC", description: "Fix clippy warning on cli", pr_number: 13507, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "de36be69b4f8facb2a04fe8ead3e09c7f144ad4b", date: "2022-07-13 00:15:26 UTC", description: "Migrate `VrlTarget` implementation to lookup v2", pr_number: 13157, scopes: ["core"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 6, insertions_count: 94, deletions_count: 208},
		{sha: "26c4ca0c58fe2f8314a9aad34f979c583d121d44", date: "2022-07-13 02:00:39 UTC", description: "bump clap from 3.2.8 to 3.2.10", pr_number: 13512, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 10, deletions_count: 10},
		{sha: "965b0c5359a8ef902d15934e7159f033969d1c7d", date: "2022-07-13 12:42:05 UTC", description: "bump inventory from 0.1.11 to 0.3.0", pr_number: 13485, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 27},
		{sha: "6ed436c183a55cb098319a4bab25181c21eebd5c", date: "2022-07-14 04:18:18 UTC", description: "Remove obsolete `Acker`", pr_number: 13457, scopes: ["sinks"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 142, insertions_count: 879, deletions_count: 1831},
		{sha: "9a9340adb14cf7f59798c7cadd621507c8ef32b1", date: "2022-07-15 06:13:27 UTC", description: "Remove legacy `EncodingConfiguration`", pr_number: 13518, scopes: ["sinks", "codecs"], type: "chore", breaking_change: false, author: "Pablo Sichert", files_count: 143, insertions_count: 2219, deletions_count: 4308},
		{sha: "10f4a73a8dbd6313e2b88edaf9ac9a531f61e89c", date: "2022-07-15 00:36:25 UTC", description: "bump clap from 3.2.10 to 3.2.11", pr_number: 13546, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 10, deletions_count: 10},
		{sha: "ab1d1236bb85ba772ff6ad2db6a770dd52d53851", date: "2022-07-15 00:37:38 UTC", description: "bump tokio-tungstenite from 0.17.1 to 0.17.2", pr_number: 13545, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 9, deletions_count: 9},
		{sha: "13b8f335385d0ce6e1d4bbe49060161c1abb032d", date: "2022-07-15 08:02:52 UTC", description: "bump clap from 3.2.11 to 3.2.12", pr_number: 13558, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 10, deletions_count: 10},
		{sha: "d0a85e4ebd4fe6b98334bb108cf04df2d93d4778", date: "2022-07-15 09:27:54 UTC", description: "bump tokio from 1.19.2 to 1.20.0", pr_number: 13544, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 12},
		{sha: "00afb0b332327d3ef6d76cbbe68e85678c536e75", date: "2022-07-15 03:38:24 UTC", description: "Implement GELF encoder", pr_number: 13333, scopes: ["codecs", "sources"], type: "feat", breaking_change: false, author: "Kyle Criddle", files_count: 14, insertions_count: 457, deletions_count: 21},
		{sha: "fd0b6c19021b5b96ea355cb15ebfd9b988b9f965", date: "2022-07-15 03:47:37 UTC", description: "upgrade nextest version to 0.9.25", pr_number: 13555, scopes: ["tests"], type: "chore", breaking_change: false, author: "Kyle Criddle", files_count: 3, insertions_count: 3, deletions_count: 5},
		{sha: "2dd6a20733b0dae22e809151afa2d0ea4c18740b", date: "2022-07-16 17:03:03 UTC", description: "opentelemetry log", pr_number: 13320, scopes: ["sources"], type: "feat", breaking_change: false, author: "Deen", files_count: 17, insertions_count: 1028, deletions_count: 1},
		{sha: "13aad53d398d8cee9fbc899bbbde9a448696ce20", date: "2022-07-16 03:18:51 UTC", description: "config http request headers in healthcheck", pr_number: 13572, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Kyle Criddle", files_count: 1, insertions_count: 5, deletions_count: 0},
		{sha: "61756a545fd9e2aad5044739789c6f1e111ddcfc", date: "2022-07-16 03:35:33 UTC", description: "add Accept-Encoding header for compression", pr_number: 13571, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Kyle Criddle", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "d54f26edb3759e38a26756ecb5012a2ed17e4116", date: "2022-07-16 10:28:36 UTC", description: "bump axum from 0.5.12 to 0.5.13", pr_number: 13576, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "7e431fb5f45c06ad2dc38029ab8d9e65180c1ad5", date: "2022-07-16 10:31:27 UTC", description: "bump dyn-clone from 1.0.6 to 1.0.8", pr_number: 13577, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 7, deletions_count: 7},
		{sha: "b5906e3ebd409f4d208213a0abc8dafc62edc542", date: "2022-07-19 06:43:01 UTC", description: "typoes", pr_number: 13582, scopes: [], type: "docs", breaking_change: false, author: "Tshepang Mbambo", files_count: 3, insertions_count: 3, deletions_count: 3},
		{sha: "4e707f88534fc45288920f6810637385c3aa1303", date: "2022-07-19 03:22:46 UTC", description: "Add template for otel source, and correct cue docs", pr_number: 13599, scopes: [], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 16, deletions_count: 11},
		{sha: "000fe8bb918a5409b8b7df0d43c436648c611d32", date: "2022-07-19 01:29:44 UTC", description: "Handle negative acknowledgements by stopping readers", pr_number: 13563, scopes: ["buffers"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 12, insertions_count: 325, deletions_count: 81},
		{sha: "c640add4fe223041534afb5931a10c4b2bfd7ce1", date: "2022-07-19 03:35:26 UTC", description: "pre-flight work for initial integration of configuration schema for sinks", pr_number: 13516, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 33, insertions_count: 1014, deletions_count: 541},
		{sha: "cb27621bd18d53a3ac16d7c0ea146463876354f7", date: "2022-07-19 01:45:06 UTC", description: "revert add Accept-Encoding header for compression", pr_number: 13598, scopes: ["elasticsearch sink"], type: "fix", breaking_change: false, author: "Kyle Criddle", files_count: 1, insertions_count: 1, deletions_count: 3},
		{sha: "cf7ccb576e2c118b14cc26797f74ab4c92785dde", date: "2022-07-19 04:17:54 UTC", description: "bump nix from 0.24.1 to 0.24.2", pr_number: 13588, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "9cc6226923b64237b9da14e7ba6f9afa907524a8", date: "2022-07-19 05:25:56 UTC", description: "Log the address used when building gRPC servers", pr_number: 13605, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 2, deletions_count: 0},
		{sha: "8eed8e976307ec06964fcb3b4f9e898cb831977c", date: "2022-07-19 03:31:53 UTC", description: "Replace `EventArray::for_each_X` with an iterator", pr_number: 13601, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 7, insertions_count: 58, deletions_count: 39},
		{sha: "22f854341afbc49bfbb1cc0387dd19be8c8692e3", date: "2022-07-19 02:38:07 UTC", description: "Pin to cross 0.2.4", pr_number: 13604, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 9, insertions_count: 11, deletions_count: 11},
		{sha: "a79f19742d154e2e68042c5d8d2d3a33bb4d756f", date: "2022-07-19 09:39:14 UTC", description: "bump serde_yaml from 0.8.25 to 0.8.26", pr_number: 13589, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "fb051d7e4b828056a508fc60db2768116f7bf5cf", date: "2022-07-19 02:59:20 UTC", description: "Brushing off REVIEWING.md", pr_number: 13542, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 17, deletions_count: 4},
		{sha: "feb6c82823888e7366d984ba13bd0b8c21a11dd9", date: "2022-07-19 03:29:51 UTC", description: "Add enterprise soak test", pr_number: 13595, scopes: ["enterprise"], type: "chore", breaking_change: false, author: "Will", files_count: 4, insertions_count: 76, deletions_count: 0},
		{sha: "30eada18ee776326d4c88ef6da410d19ab35d9be", date: "2022-07-19 10:32:59 UTC", description: "bump async-graphql-warp from 4.0.4 to 4.0.5", pr_number: 13606, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "cf73d67e4270509b7b6f3394f88f965ccea4a0b5", date: "2022-07-19 11:57:07 UTC", description: "bump rustyline from 9.1.2 to 10.0.0", pr_number: 13590, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 16, deletions_count: 10},
		{sha: "da0076250e4d6110bfbd64426c674b12d87e8db2", date: "2022-07-19 05:43:32 UTC", description: "Change default enterprise reporting interval seconds to 1", pr_number: 13608, scopes: ["enterprise"], type: "chore", breaking_change: false, author: "Will", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "788078497177b949d8983bee43e782e43ed4d34c", date: "2022-07-19 20:10:39 UTC", description: "Add UX note about not requiring manual intervention", pr_number: 13607, scopes: ["dev"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 65, deletions_count: 47},
		{sha: "3b71e0085dc40306163825e4384e22c367f3eae6", date: "2022-07-20 07:23:37 UTC", description: "Add typedef support to metadata", pr_number: 13462, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Nathan Fox", files_count: 17, insertions_count: 333, deletions_count: 82},
		{sha: "02c8b50b058546dca616d9483099afa4c3a40fd7", date: "2022-07-21 05:56:47 UTC", description: "introduce an interval field", pr_number: 13394, scopes: ["metrics"], type: "enhancement", breaking_change: false, author: "prognant", files_count: 18, insertions_count: 210, deletions_count: 59},
		{sha: "f9b2e8d21fa2cd6292e576160c7eb6d0e45ffa57", date: "2022-07-21 01:11:20 UTC", description: "Try to fix file_directory_update test", pr_number: 13636, scopes: ["tests"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 10, deletions_count: 3},
		{sha: "4f2f541e48efe5c47435bd1eaf564fa8cdcc3509", date: "2022-07-21 01:37:16 UTC", description: "Update link to Security guide", pr_number: 13640, scopes: ["docs"], type: "chore", breaking_change: false, author: "Kyle Criddle", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "e4fc031e49c5632f6a8228447f0366983c118e01", date: "2022-07-21 02:03:39 UTC", description: "Standardize on a common maximum line length", pr_number: 13574, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "67cbd1102d1272d73b97d1ddd7d439dacfaceb3d", date: "2022-07-21 03:28:02 UTC", description: "Eliminate histogram bucket loop", pr_number: 13631, scopes: ["metrics"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 60, deletions_count: 31},
		{sha: "2f8d0ec16e5ec8d5e1625c53a576c01cd9f4b8c9", date: "2022-07-21 05:32:17 UTC", description: "bump clap from 3.2.12 to 3.2.13", pr_number: 13627, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 10, deletions_count: 10},
		{sha: "10b7b1c0e6920e58810653058013867f72468bdc", date: "2022-07-21 05:32:30 UTC", description: "bump lru from 0.7.7 to 0.7.8", pr_number: 13628, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "77285cca5c51e3c70fac3f173cfefd8be5e9d9a1", date: "2022-07-21 05:33:15 UTC", description: "bump mongodb from 2.2.2 to 2.3.0", pr_number: 13630, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 13, deletions_count: 6},
		{sha: "443f4d81d325addf3f26360c8163f66b7b699f46", date: "2022-07-21 05:33:28 UTC", description: "bump docker/build-push-action from 3.0.0 to 3.1.0", pr_number: 13623, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "e6eb3846abcaa4dcd80d202660eacd0e031559a0", date: "2022-07-21 11:40:43 UTC", description: "Introduce an optional separator parameter", pr_number: 13618, scopes: ["flatten"], type: "enhancement", breaking_change: false, author: "Thiago Trennepohl", files_count: 2, insertions_count: 73, deletions_count: 12},
		{sha: "05917afce645719cde1e050995b301fe889c9b98", date: "2022-07-21 04:29:10 UTC", description: "extend parse_apache_log error format compatibility", pr_number: 13581, scopes: ["vrl"], type: "fix", breaking_change: false, author: "Kyle Criddle", files_count: 3, insertions_count: 134, deletions_count: 75},
		{sha: "00c5520b01b313f08c235de156718b2306e4944f", date: "2022-07-21 11:26:45 UTC", description: "bump bytes from 1.1.0 to 1.2.0", pr_number: 13629, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 76, deletions_count: 76},
		{sha: "82ec94a564ac909e4fd4f96bc852bfc8b8cad062", date: "2022-07-21 22:34:48 UTC", description: "Upgrade `metrics` crates", pr_number: 13653, scopes: ["internal_metrics source"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 10, insertions_count: 149, deletions_count: 299},
		{sha: "5665f41ffbed4f7994f0f3c50fc3d95e74e0c2f7", date: "2022-07-22 01:42:30 UTC", description: "bump goauth from 0.13.0 to 0.13.1", pr_number: 13649, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "9911b525a94dad0815151d93fd9c8be508bd0b2d", date: "2022-07-22 01:42:49 UTC", description: "bump windows-service from 0.4.0 to 0.5.0", pr_number: 13650, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 11, deletions_count: 5},
		{sha: "81e1796b0f84e7f76db4a315d414cf4ebac895cf", date: "2022-07-22 06:11:23 UTC", description: "bump tracing-subscriber from 0.3.14 to 0.3.15", pr_number: 13652, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 17, deletions_count: 17},
		{sha: "7e06aa698805c964f79dc7a2aa4683876fb1e884", date: "2022-07-22 06:15:45 UTC", description: "bump serde from 1.0.139 to 1.0.140", pr_number: 13646, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "5f7b366cfab919587fb7454e4d336cd10edd4e02", date: "2022-07-22 14:33:49 UTC", description: "Change bytes_written_from to i64", pr_number: 13656, scopes: ["mongodb_metrics source"], type: "fix", breaking_change: false, author: "Rui Li", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "b39bbd9c04e6345c37b98e397cccfd873028ab04", date: "2022-07-22 07:00:40 UTC", description: "bump serde_with from 1.14.0 to 2.0.0", pr_number: 13591, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 33, deletions_count: 5},
		{sha: "4c3c3d936a4af13a005d400d80ecbc65047e9d0c", date: "2022-07-22 01:27:41 UTC", description: "Upgrade metrics crates", pr_number: 13663, scopes: ["dependencies"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 6, insertions_count: 48, deletions_count: 84},
		{sha: "988d1754f5c0e1dfc30f3e2226736965b2ed6079", date: "2022-07-22 04:31:13 UTC", description: "Refactor source into multiple files", pr_number: 13658, scopes: ["opentelemetry source"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 235, deletions_count: 226},
		{sha: "a2cf7d2fff1bf98f5025ab6891c54f0cd565e1dc", date: "2022-07-22 06:12:20 UTC", description: "bump clap from 3.2.13 to 3.2.14", pr_number: 13660, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 10, deletions_count: 10},
		{sha: "a3a332fbb1d1970432c193f80c242e17eeea9e5e", date: "2022-07-22 12:31:51 UTC", description: "add RFC for Google Chronicle sink", pr_number: 12733, scopes: ["new sink"], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 1, insertions_count: 215, deletions_count: 0},
		{sha: "f9023e2167386fbb92113bc1956dce5e912149a2", date: "2022-07-22 18:05:59 UTC", description: "fix typos", pr_number: 13669, scopes: [], type: "docs", breaking_change: false, author: "Tshepang Mbambo", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "b179c9ee7561b5d29ee3c658f7a125c56ce65f2b", date: "2022-07-22 23:29:50 UTC", description: "bump test-case from 2.1.0 to 2.2.1", pr_number: 13671, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "902496eba76014902858acebfb4518a4ec33366f", date: "2022-07-22 21:30:16 UTC", description: "Upgrade Rust to 1.62.1", pr_number: 13667, scopes: ["deps"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 7, insertions_count: 7, deletions_count: 7},
		{sha: "1ad011028dbffbd55d54fb7584d06fb7c71b6738", date: "2022-07-23 00:32:15 UTC", description: "Allow for traces as inputs and outputs", pr_number: 13610, scopes: ["sample transform"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 3, insertions_count: 28, deletions_count: 9},
		{sha: "0bac4c4bd179f16e335c0d43300a9a52bd44c768", date: "2022-07-23 05:49:25 UTC", description: "new sink for Google chronicle", pr_number: 13550, scopes: ["new sink"], type: "enhancement", breaking_change: false, author: "Stephen Wakely", files_count: 11, insertions_count: 787, deletions_count: 1},
		{sha: "cfd6f5ce8fb6d3ac0148ec0da5a2bfa4270f408d", date: "2022-07-23 03:15:44 UTC", description: "bump metrics from 0.20.0 to 0.20.1", pr_number: 13689, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 8, deletions_count: 8},
		{sha: "05a318729cbcd3733957da7c3f6cf1d9bf70283b", date: "2022-07-23 13:45:13 UTC", description: "bump rdkafka from 0.27.0 to 0.28.0", pr_number: 10302, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 12, deletions_count: 7},
		{sha: "5e546c6d67be4fb01d835679f68af58ff1b492aa", date: "2022-07-26 02:33:51 UTC", description: "rename `sources::datadog::agent` to `sources::datadog_agent`", pr_number: 13693, scopes: ["core"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 8, insertions_count: 11, deletions_count: 16},
		{sha: "47624d48302767429c7749cef7216b3154442e86", date: "2022-07-26 03:09:05 UTC", description: "initial integration of configuration schema for sinks (part one)", pr_number: 13688, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 77, insertions_count: 1867, deletions_count: 304},
		{sha: "9674f28326f981e9985292b6d0f05392267de726", date: "2022-07-26 03:16:37 UTC", description: "Fix many typedef bugs, introduce `Kind::get` and `Kind::insert`", pr_number: 13619, scopes: ["vrl"], type: "fix", breaking_change: false, author: "Nathan Fox", files_count: 53, insertions_count: 1953, deletions_count: 2545},
		{sha: "158f861491032bfb3823a7626d83745f5f315165", date: "2022-07-26 15:58:03 UTC", description: "bump chrono-tz from 0.6.1 to 0.6.2", pr_number: 13700, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 30, deletions_count: 11},
		{sha: "eba8ea0c184fe4afcff9b1d1b6ded69d378ba3e6", date: "2022-07-26 17:34:41 UTC", description: "bump bytecheck from 0.6.8 to 0.6.9", pr_number: 13698, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "238d3a1f3ae7ce7f0b92a29ecc85522b4bfa92d4", date: "2022-07-27 00:01:16 UTC", description: "fetch indexes if vector config is modified", pr_number: 13704, scopes: ["enriching"], type: "fix", breaking_change: false, author: "Stephen Wakely", files_count: 2, insertions_count: 6, deletions_count: 1},
		{sha: "6bd464bde37125b1f9ce449e851a6196898da5f9", date: "2022-07-26 22:12:46 UTC", description: "bump axum from 0.5.13 to 0.5.14", pr_number: 13708, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "c57b312188363e6fa539905bced3a9e9a0232395", date: "2022-07-26 22:12:56 UTC", description: "bump chrono-tz from 0.6.2 to 0.6.3", pr_number: 13709, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 13, deletions_count: 14},
		{sha: "8f8c8e0824649707d0de503edec1e14d7206fdca", date: "2022-07-26 22:13:11 UTC", description: "bump proc-macro2 from 1.0.40 to 1.0.42", pr_number: 13710, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "0d8978034b80a48b676e5e215b4760352b082a84", date: "2022-07-26 22:13:25 UTC", description: "bump tokio from 1.20.0 to 1.20.1", pr_number: 13711, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 12, deletions_count: 12},
		{sha: "0245bd496838b14ca1be908640377e722ed41e0d", date: "2022-07-26 22:13:38 UTC", description: "bump mlua from 0.8.1 to 0.8.2", pr_number: 13712, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "99377a93301d061446a7e7149d0a6342d02f9f05", date: "2022-07-26 22:13:51 UTC", description: "bump clap from 3.2.14 to 3.2.15", pr_number: 13713, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 12, deletions_count: 12},
		{sha: "07e26e785fc365fe93bc8aac91ef9aec91042d03", date: "2022-07-26 22:14:25 UTC", description: "bump crossbeam-queue from 0.3.5 to 0.3.6", pr_number: 13699, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "edacee2482be1e79e10ea9a3d6766114cae44e67", date: "2022-07-27 06:54:31 UTC", description: "bump crossbeam-utils from 0.8.10 to 0.8.11", pr_number: 13697, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 5, deletions_count: 5},
		{sha: "3fd00d6cb43ca00233023a8a2b611923778a7af0", date: "2022-07-27 13:48:01 UTC", description: "'iowait' stat on Linux", pr_number: 13696, scopes: ["host_metrics source"], type: "enhancement", breaking_change: false, author: "Charalampos Mitrodimas", files_count: 2, insertions_count: 11, deletions_count: 1},
		{sha: "29e6b2abd1111018ae32a744f5c76b4a46090d21", date: "2022-07-27 08:34:18 UTC", description: "Upgrade AWS Smithy crates to 0.46.0", pr_number: 13657, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 66, deletions_count: 63},
		{sha: "f689385a7ced0cf87a9c81dee8d05b5dbdc21890", date: "2022-07-27 06:42:35 UTC", description: "add source_type for sources that report it", pr_number: 13541, scopes: [], type: "docs", breaking_change: false, author: "Kyle Criddle", files_count: 22, insertions_count: 256, deletions_count: 47},
		{sha: "97ccda4bebd75e2a9b567071a1eb77c537b0a6b9", date: "2022-07-28 05:35:46 UTC", description: "Call `parse_groks` in implementation of `parse_grok`", pr_number: 13635, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Pablo Sichert", files_count: 2, insertions_count: 148, deletions_count: 205},
		{sha: "c851b0326967c146c2186807cf9dd0c4df10b28d", date: "2022-07-28 01:00:57 UTC", description: "initial integration of configuration schema for sinks (part two)", pr_number: 13722, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 28, insertions_count: 677, deletions_count: 90},
		{sha: "dbd06c7955a86e32b8d1ac5fddcb28702da4c38f", date: "2022-07-27 22:21:02 UTC", description: "Try configuring release build to use `git` to fetch", pr_number: 13730, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 6, deletions_count: 0},
		{sha: "7014c1cbe703306e958d396f05bc08e3665d5f9c", date: "2022-07-28 00:58:37 UTC", description: "Fix handling parsing Linux hybrid cgroups", pr_number: 13719, scopes: ["host_metrics source"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 470, deletions_count: 188},
		{sha: "6efbdfaf9e5d8644a377ff8203e9935b60818842", date: "2022-07-28 05:31:46 UTC", description: "Simplify schema definition configuration", pr_number: 13717, scopes: ["schemas"], type: "enhancement", breaking_change: false, author: "Nathan Fox", files_count: 15, insertions_count: 515, deletions_count: 121},
		{sha: "b838a19e3421994c165c6d47671fe803860bf32f", date: "2022-07-28 08:41:31 UTC", description: "Example for adding `log_namespace` support to `demo_logs` source", pr_number: 13720, scopes: ["schema"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 3, insertions_count: 50, deletions_count: 18},
		{sha: "c3fcc6e20fbf06ed14c7898bbc0018bf6d00de11", date: "2022-07-28 08:02:17 UTC", description: "Regularize the emitted events in socket sinks", pr_number: 13739, scopes: ["sinks"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 32, deletions_count: 17},
		{sha: "daa21d67c8e02648d1ca87f44c657d0a15013d2d", date: "2022-07-29 00:19:28 UTC", description: "Log the address for the datadog_agent http server", pr_number: 13745, scopes: ["datadog_agent source"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 4, deletions_count: 0},
		{sha: "3d65e4e5d10c44280d13ad49e6d85e2d984164bb", date: "2022-07-28 22:22:58 UTC", description: "Add support for acknowledgements to all socket sinks", pr_number: 13741, scopes: ["sinks"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 11, insertions_count: 141, deletions_count: 62},
		{sha: "975b99c65e6445b87c3ef7d3885f18f519f2f3bc", date: "2022-07-29 00:33:19 UTC", description: "Add end-to-end acknowledgements support", pr_number: 13744, scopes: ["pulsar sink"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 2, insertions_count: 18, deletions_count: 6},
		{sha: "9a049b5d18590a195aec0fa8958f80245ca1c3b0", date: "2022-07-29 01:28:06 UTC", description: "update DEVELOPING.md to reference correct integration test github workflow file.", pr_number: 13751, scopes: ["docs"], type: "chore", breaking_change: false, author: "Kyle Criddle", files_count: 1, insertions_count: 2, deletions_count: 3},
		{sha: "582733307bb9e8ec8569c9b0bbc6f8d5946bf7be", date: "2022-07-29 04:09:50 UTC", description: "Publish docs", pr_number: 13753, scopes: ["gcp_chronicle_unstructured sink"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 16, deletions_count: 1},
		{sha: "3b0f45c4e4e523393f532709a67af008d39e19e7", date: "2022-07-29 05:43:14 UTC", description: "Simplify internal collection of metrics", pr_number: 13735, scopes: ["host_metrics source"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 8, insertions_count: 434, deletions_count: 561},
		{sha: "945764924733c52b7566ec396ddecd9462b408d7", date: "2022-07-29 06:33:05 UTC", description: "Remove extraneous to_string", pr_number: 13749, scopes: ["observability"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 13, deletions_count: 11},
		{sha: "8973cde4db0118760a2d0905743350e13157baac", date: "2022-07-29 07:38:35 UTC", description: "Add acknowledgements support", pr_number: 13750, scopes: ["prometheus_exporter sink"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 44, deletions_count: 20},
		{sha: "ee4efa1b44a788cd7e74b0ec762d8eb5157b605d", date: "2022-07-30 00:34:25 UTC", description: "Add flag for enabling sink requirements", pr_number: 13666, scopes: ["core"], type: "feat", breaking_change: false, author: "Nathan Fox", files_count: 3, insertions_count: 123, deletions_count: 13},
		{sha: "10cfcc2e7b9be90f8937da46d4655f8cd81ece3f", date: "2022-07-30 00:16:09 UTC", description: "Add acknowledgements support", pr_number: 13755, scopes: ["prometheus_remote_write sink"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 9, deletions_count: 1},
		{sha: "7ec77ad0853252ffc8bb7550507281ac965ebd0f", date: "2022-07-30 07:28:47 UTC", description: "initial integration of configuration schema for sinks (part three)", pr_number: 13757, scopes: ["config"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 26, insertions_count: 1030, deletions_count: 137},
		{sha: "782549cd26fbdb39a1271e617ff723edeee956ea", date: "2022-07-30 05:39:30 UTC", description: "Make acknowledgement configuration non-optional", pr_number: 13760, scopes: ["sinks"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 62, insertions_count: 141, deletions_count: 133},
		{sha: "6a50d5a5a644096879bcb1fa7de5554d422fa513", date: "2022-08-02 06:36:09 UTC", description: "add deprecated comment to old sink components", pr_number: 13626, scopes: [], type: "chore", breaking_change: false, author: "Stephen Wakely", files_count: 6, insertions_count: 14, deletions_count: 0},
		{sha: "da251b14b5bebbe884b024faf409735711e4b558", date: "2022-08-02 15:26:45 UTC", description: "Fix data types of metrics", pr_number: 13729, scopes: ["mongodb_metrics source"], type: "fix", breaking_change: false, author: "Rui Li", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "e5fb5c910c56568ba15e9f38c77b4ff0fdbebff1", date: "2022-08-02 01:37:02 UTC", description: "(revert) Fix data types of metrics", pr_number: 13787, scopes: ["mongodb_metrics source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "3c5c0760b7d2168771e5600c4994d075a7f1aa07", date: "2022-08-02 14:12:34 UTC", description: "add `--prefix` option for installation script", pr_number: 13613, scopes: ["distribution"], type: "enhancement", breaking_change: false, author: "Diptesh Choudhuri", files_count: 4, insertions_count: 93, deletions_count: 10},
		{sha: "e5c435f8c7049fea93438408059f02167c61b747", date: "2022-08-02 06:12:41 UTC", description: "Fix datadog_agent integration test", pr_number: 13788, scopes: ["ci"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "3fd07543f5c2db8d7243753458cc088340daf18e", date: "2022-08-02 08:15:11 UTC", description: "deny unknown fields on transformer config", pr_number: 13792, scopes: ["codecs"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 13, deletions_count: 0},
		{sha: "2a889d14fe92b1eb5c85209aa891ab52af06f770", date: "2022-08-03 00:42:26 UTC", description: "Make `make check` and `make check-clippy` line up", pr_number: 13763, scopes: ["dev"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 5, deletions_count: 5},
		{sha: "d752f86bcd198bd8ea8e4206f5349a380857b538", date: "2022-08-03 03:32:42 UTC", description: "bump bytes from 1.2.0 to 1.2.1", pr_number: 13775, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 11, insertions_count: 77, deletions_count: 77},
		{sha: "42f550765374da88828a457974971447b6f895b8", date: "2022-08-03 03:32:55 UTC", description: "bump arc-swap from 1.5.0 to 1.5.1", pr_number: 13776, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "45b2cc5c1a44e1f77cf685843f15d153276adb43", date: "2022-08-03 03:33:10 UTC", description: "bump ndarray from 0.15.4 to 0.15.6", pr_number: 13777, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "8fa656147449f5124978f08efe6dd24100810320", date: "2022-08-03 03:59:16 UTC", description: "Indicate that `processing_errors_total` is deprecated", pr_number: 13765, scopes: ["observability"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "2aa2b3328d1e9b9f20daef41c6f9089a6f8299fd", date: "2022-08-03 05:29:01 UTC", description: "bump typetag from 0.2.1 to 0.2.2", pr_number: 13803, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "639f8bed7fa2ef1399ff211be0ae99ab66131200", date: "2022-08-03 05:29:18 UTC", description: "bump serde from 1.0.140 to 1.0.141", pr_number: 13799, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "fb2d534c330d3d432001c52afc081d120fadedb6", date: "2022-08-03 05:29:31 UTC", description: "bump async-trait from 0.1.56 to 0.1.57", pr_number: 13800, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "47d2a0eb005f84952c674c78e90f4aaf326d6465", date: "2022-08-03 12:56:10 UTC", description: "bump clap from 3.2.15 to 3.2.16", pr_number: 13778, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 10, deletions_count: 10},
		{sha: "8ce2b45214bd5b2393543a3f3ce3cbd30fc8093a", date: "2022-08-03 14:23:32 UTC", description: "bump indoc from 1.0.6 to 1.0.7", pr_number: 13801, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 7, deletions_count: 7},
		{sha: "49f02b92ae117708bfab0d9512daef1e84075533", date: "2022-08-03 23:55:32 UTC", description: "bump semver from 1.0.12 to 1.0.13", pr_number: 13811, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 5},
		{sha: "fa656583c240b651d4b4902c26eaf90cb9999a6f", date: "2022-08-03 23:56:01 UTC", description: "bump syn from 1.0.98 to 1.0.99", pr_number: 13807, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "eba5d85af1f2571cd76c4af04331c2f1a260c835", date: "2022-08-03 23:57:46 UTC", description: "bump thiserror from 1.0.31 to 1.0.32", pr_number: 13810, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 4, deletions_count: 4},
		{sha: "508b54326ca75ce111c5587c17724a6d6ef2fb89", date: "2022-08-03 23:58:01 UTC", description: "bump proc-macro2 from 1.0.42 to 1.0.43", pr_number: 13808, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "8ca8325548a719e613072dfadb0ac3bf7b560782", date: "2022-08-04 05:05:13 UTC", description: "bump async-graphql-warp from 4.0.5 to 4.0.6", pr_number: 13805, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "4a145531b00ae3940ca9fbaafc117de4a62951fa", date: "2022-08-04 05:26:16 UTC", description: "bump anyhow from 1.0.58 to 1.0.59", pr_number: 13816, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "b8e2fae6bba58b01d03847c5b9dcb1701df1edfb", date: "2022-08-04 06:03:56 UTC", description: "bump dyn-clone from 1.0.8 to 1.0.9", pr_number: 13817, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 6, insertions_count: 7, deletions_count: 7},
		{sha: "b16644a5b1896fdaa7af5a691ec7e66c7908e10f", date: "2022-08-04 06:04:02 UTC", description: "bump quote from 1.0.20 to 1.0.21", pr_number: 13819, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "4fd558d2400edb8c997f545b171ba37552aefbb6", date: "2022-08-04 06:05:43 UTC", description: "bump inventory from 0.3.0 to 0.3.1", pr_number: 13818, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "3658aee88cbf3243a49b790ac27b3a7e4fb279b8", date: "2022-08-04 06:42:42 UTC", description: "bump mlua from 0.8.2 to 0.8.3", pr_number: 13822, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "eb97b86e51261d2bb37b4eade704f486f9940800", date: "2022-08-04 15:10:03 UTC", description: "Fix powershell script", pr_number: 13566, scopes: ["installation msi"], type: "docs", breaking_change: false, author: "Rui Li", files_count: 1, insertions_count: 2, deletions_count: 3},
		{sha: "7d9e3e6d9b0d514dc4a01460a284c011a8da7af2", date: "2022-08-04 07:28:04 UTC", description: "bump typetag from 0.2.2 to 0.2.3", pr_number: 13823, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 3, insertions_count: 6, deletions_count: 6},
		{sha: "65fe41b1f8ef41fc9cddec7cd10561751685922a", date: "2022-08-04 07:33:02 UTC", description: "bump paste from 1.0.7 to 1.0.8", pr_number: 13824, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "b5fa9271aeeab6ccfd3957c40fc737f041902bcb", date: "2022-08-04 07:45:00 UTC", description: "bump serde_bytes from 0.11.6 to 0.11.7", pr_number: 13825, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "001d4c3a13bf41ec3bc02f5dfdef0b6d268ab72a", date: "2022-08-04 08:37:41 UTC", description: "bump inherent from 1.0.1 to 1.0.2", pr_number: 13820, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "c3f8f580a4489d406c3b82549faf87dad930554d", date: "2022-08-04 02:37:21 UTC", description: "new `apex` sink", pr_number: 13436, scopes: ["new sink"], type: "feat", breaking_change: false, author: "Matt Casper", files_count: 13, insertions_count: 420, deletions_count: 1},
		{sha: "05b78f5552119126a1a9b60e63fb9f2eb636ab8d", date: "2022-08-04 03:09:34 UTC", description: "Remove PASS_FEATURES", pr_number: 13831, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 3, deletions_count: 10},
		{sha: "f64bceffe36cf987b23bd541bebc5ef8498d8171", date: "2022-08-04 10:15:00 UTC", description: "bump serde_json from 1.0.82 to 1.0.83", pr_number: 13821, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 11, deletions_count: 11},
		{sha: "3b0771fbf7bc4cb46c68b68b12c5c69ef0553aa6", date: "2022-08-04 21:19:46 UTC", description: "bump libc from 0.2.126 to 0.2.127", pr_number: 13841, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "4b3fe0ad66d3e2d50194da12d82b3153a86b5b22", date: "2022-08-04 21:19:57 UTC", description: "bump serde from 1.0.141 to 1.0.142", pr_number: 13826, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "2c768b0b643d4035b511764048921f30ac2b3298", date: "2022-08-05 05:53:02 UTC", description: "bump wiremock from 0.5.13 to 0.5.14", pr_number: 13848, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 3},
		{sha: "bbe6eb8db7e548bd708de4bc1fe8ec6fdc634b9b", date: "2022-08-05 04:30:40 UTC", description: "add auth.region option when assuming role", pr_number: 13838, scopes: ["aws"], type: "feat", breaking_change: false, author: "Andrew Kutta", files_count: 2, insertions_count: 25, deletions_count: 4},
		{sha: "57db12a7eb758696a91ab536295f86c32c9965eb", date: "2022-08-05 11:17:20 UTC", description: "cert verification with proxy enabled", pr_number: 13759, scopes: ["http sink"], type: "fix", breaking_change: false, author: "ntim", files_count: 1, insertions_count: 7, deletions_count: 0},
		{sha: "c402e6ed94400959dbc298d4b342dcb0842089d4", date: "2022-08-05 05:46:06 UTC", description: "add methods to check if a string is an IPv4 or IPv6 address", pr_number: 13852, scopes: ["vrl"], type: "feat", breaking_change: false, author: "Toby Lawrence", files_count: 8, insertions_count: 391, deletions_count: 1},
		{sha: "e6f1267b5180771504d81c0d5bcc7a0ac75f04d4", date: "2022-08-06 04:47:25 UTC", description: "bump docker/build-push-action from 3.1.0 to 3.1.1", pr_number: 13866, scopes: ["ci"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 4, insertions_count: 6, deletions_count: 6},
		{sha: "4ed030c583243126fc5bab74aa791c58d7b5e0f2", date: "2022-08-06 04:32:17 UTC", description: "RFC for registered internal events", pr_number: 13771, scopes: ["observability"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 1, insertions_count: 264, deletions_count: 0},
		{sha: "5707371b031a1d888a892452aec2cb52ac24caba", date: "2022-08-06 04:34:28 UTC", description: "Emit new `source_lag_time_seconds` histogram", pr_number: 13611, scopes: ["sources"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 4, insertions_count: 188, deletions_count: 12},
		{sha: "483a810b2172e15ecb5c93dcd6e8a54f27b98e81", date: "2022-08-06 05:13:56 UTC", description: "Remove some stale cargo-deny entries", pr_number: 13873, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 0, deletions_count: 16},
		{sha: "4611307463ced2d069e806666178a3977265a3bc", date: "2022-08-08 08:26:03 UTC", description: "Add HTTP server and integration test", pr_number: 13798, scopes: ["opentelemetry source"], type: "enhancement", breaking_change: false, author: "Spencer Gilbert", files_count: 15, insertions_count: 694, deletions_count: 39},
		{sha: "bdff6b8c8a17ef01deb9c8c673fa6bfa4dab37e3", date: "2022-08-09 01:57:59 UTC", description: "add metric timestamp if source log lacks one", pr_number: 13871, scopes: ["log_to_metric transform"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 3, deletions_count: 1},
		{sha: "a101266cadfbe7e548dd12dee1af0f3b315ea682", date: "2022-08-09 02:25:24 UTC", description: "bump anyhow from 1.0.59 to 1.0.60", pr_number: 13879, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "4a37187716c85163a99510b5cae1facb7713bcd0", date: "2022-08-09 03:14:58 UTC", description: "bump nats from 0.22.0 to 0.23.0", pr_number: 13878, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "2d485f72737c2b06d8926e1e0c99558537e67a2f", date: "2022-08-09 01:08:47 UTC", description: "bump rust_decimal from 1.25.0 to 1.26.1", pr_number: 13877, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 1, insertions_count: 2, deletions_count: 2},
		{sha: "3bab7aed9471ba8ace2a95a04178f26783433c85", date: "2022-08-09 07:06:24 UTC", description: "wait for mock endpoint in tests", pr_number: 13885, scopes: ["prometheus_scrape source", "prometheus_remote_write source"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 2, insertions_count: 13, deletions_count: 6},
		{sha: "8f2ae47fea9f20d4bf170b643cf0ea5a1f980c57", date: "2022-08-09 04:48:03 UTC", description: "Lower log level for HTTP errors", pr_number: 12930, scopes: ["observability"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "afcee9be8840174292d47a9e06d19d059f77e246", date: "2022-08-09 07:21:36 UTC", description: "Fix type of cardinality metric to gauge", pr_number: 13854, scopes: ["internal_metrics source"], type: "fix", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 82, deletions_count: 32},
		{sha: "085042864fea0b6beaf21b17a7cd0b57c0e64f56", date: "2022-08-10 01:52:15 UTC", description: "remove unnecessary dep from datadog metrics sink feature flag", pr_number: 13901, scopes: ["deps"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "a2f5889702880a88d6a5cab82c5671c9520684ad", date: "2022-08-10 15:21:37 UTC", description: "fix metrics not attached to the source", pr_number: 13868, scopes: ["docker_logs source"], type: "fix", breaking_change: false, author: "Zhenchi", files_count: 1, insertions_count: 31, deletions_count: 26},
		{sha: "140e6432eb682b2e642d03d1679d9a8f2262757e", date: "2022-08-10 15:24:48 UTC", description: "Fix panic after bump bollard", pr_number: 13862, scopes: ["docker_logs source"], type: "fix", breaking_change: false, author: "Zhenchi", files_count: 3, insertions_count: 5, deletions_count: 3},
		{sha: "8d280d198e3f65eca93ec9d73a51f139d7af3b6b", date: "2022-08-10 02:20:31 UTC", description: "Add number of logical & physical CPUs metrics", pr_number: 13874, scopes: ["host_metrics source"], type: "enhancement", breaking_change: false, author: "Kyle Criddle", files_count: 1, insertions_count: 47, deletions_count: 5},
		{sha: "a47dc196134e1f19b4c712a38541437879b33a08", date: "2022-08-10 01:24:41 UTC", description: "Upgrade AWS crates to 0.17 / 0.47", pr_number: 13869, scopes: ["deps"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 109, deletions_count: 64},
		{sha: "20d69b7a06bd2d76594ea4564743a35a24c5614b", date: "2022-08-10 05:33:10 UTC", description: "Allow setting HTTP authorization header", pr_number: 13632, scopes: ["websocket sink"], type: "enhancement", breaking_change: false, author: "Weston Carlson", files_count: 3, insertions_count: 110, deletions_count: 27},
		{sha: "447fdc9895466d4e78566dfc057e7452c2a6cde1", date: "2022-08-10 08:30:14 UTC", description: "Add support for expiring internal metrics", pr_number: 13872, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 23, insertions_count: 257, deletions_count: 58},
		{sha: "579d49882775d00b5297a41adb5faaac2ba2898a", date: "2022-08-10 22:06:25 UTC", description: "Convert internal metric counters to incremental", pr_number: 13883, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 25, deletions_count: 9},
		{sha: "838e465997f3c5c4f97b9bee550b42637523d447", date: "2022-08-11 04:01:13 UTC", description: "Add new `configuration_examples` field for components", pr_number: 13928, scopes: ["external_docs"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 28, deletions_count: 82},
		{sha: "5104c6ac4f167e0faef09193d64a033d7ba2937e", date: "2022-08-11 07:15:19 UTC", description: "bump libc from 0.2.127 to 0.2.129", pr_number: 13915, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "e21c190d212a5dea75676a4e5f0807b7fec20033", date: "2022-08-11 07:15:39 UTC", description: "bump axum from 0.5.14 to 0.5.15", pr_number: 13909, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "f7cbf4fc3b2940d3023eb63e6ef63e232e02521a", date: "2022-08-11 04:22:57 UTC", description: "bump onig from 6.3.2 to 6.4.0", pr_number: 13908, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 6, deletions_count: 6},
		{sha: "112ce8d890dabb0514b3c3936e2c58cff7e15e8b", date: "2022-08-11 04:01:50 UTC", description: "Release v0.23.1 as v0.23.3", pr_number: 13929, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 5, insertions_count: 10, deletions_count: 8},
		{sha: "c5a94ffcb18da11f8471ce07d23377c641ef8de3", date: "2022-08-11 05:52:17 UTC", description: "Add initial registered internal event implementation", pr_number: 13905, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 24, insertions_count: 370, deletions_count: 190},
		{sha: "4ffd5580cf59727081005d1a11694e8d3c62091a", date: "2022-08-11 05:22:16 UTC", description: "Verify tag matches version for releases", pr_number: 13930, scopes: ["releasing"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 8, deletions_count: 0},
		{sha: "aed82a94adcbd057798b6572dfc4dd5a352988ab", date: "2022-08-11 12:54:49 UTC", description: "bump serde from 1.0.142 to 1.0.143", pr_number: 13907, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 9, insertions_count: 13, deletions_count: 13},
		{sha: "c9b59369e869d5d5f15524f04d83da826e81ad11", date: "2022-08-11 06:28:37 UTC", description: "bump crossterm from 0.24.0 to 0.25.0", pr_number: 13933, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 4, deletions_count: 4},
		{sha: "2e5787cc9e4cf46c3562dfa6c7653858da91cd01", date: "2022-08-11 07:58:32 UTC", description: "Clarify why versions were skipped", pr_number: 13937, scopes: ["releasing"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "4435d1673b18aa6658ab5d59d8f4bb6b4466f369", date: "2022-08-11 15:05:18 UTC", description: "bump console-subscriber from 0.1.6 to 0.1.7", pr_number: 13936, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 91, deletions_count: 26},
		{sha: "148a40944c27a904b9fad8d4b48592dda5eaaa24", date: "2022-08-11 22:45:29 UTC", description: "Clarify bytes metrics in component spec", pr_number: 12912, scopes: [], type: "chore", breaking_change: false, author: "Ben Johnson", files_count: 1, insertions_count: 111, deletions_count: 107},
		{sha: "3e4c2ef3759cd45196300338473381a2be425c51", date: "2022-08-11 20:14:13 UTC", description: "(revert) Add initial registered internal event implementation", pr_number: 13938, scopes: ["observability"], type: "enhancement", breaking_change: false, author: "Jesse Szwedko", files_count: 24, insertions_count: 190, deletions_count: 370},
		{sha: "4ccf57c54ed448e8e6a09ee967951e4b8c8fb775", date: "2022-08-12 00:44:44 UTC", description: "bump anyhow from 1.0.60 to 1.0.61", pr_number: 13940, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "a24cbd334ad9a3be22b60c143378b778be053bca", date: "2022-08-12 00:46:23 UTC", description: "Include source_type on logs received by opentelemetry source", pr_number: 13939, scopes: ["opentelemetry source"], type: "feat", breaking_change: false, author: "Spencer Gilbert", files_count: 2, insertions_count: 3, deletions_count: 0},
		{sha: "5465812ef74cfc90ce2e324e043d4032a86d527d", date: "2022-08-12 01:17:15 UTC", description: "remove unimplemented protobuf_decode metric", pr_number: 13945, scopes: ["opentelemetry source"], type: "chore", breaking_change: false, author: "Spencer Gilbert", files_count: 1, insertions_count: 0, deletions_count: 1},
		{sha: "9b7e25c735b63d7dc56bb2cba5306e39fd3817f4", date: "2022-08-11 22:33:55 UTC", description: "Remove tls.enabled flag from components that don't support it", pr_number: 13925, scopes: ["config"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 40, insertions_count: 69, deletions_count: 24},
		{sha: "3d92547bc360abe2e5df0118da189609e8f9909b", date: "2022-08-12 01:46:13 UTC", description: "Refresh documentation and examples", pr_number: 13921, scopes: ["opentelemetry source"], type: "docs", breaking_change: false, author: "Spencer Gilbert", files_count: 4, insertions_count: 88, deletions_count: 29},
		{sha: "33625e722984b16ae88e6380de254ae06f73d56e", date: "2022-08-12 01:59:00 UTC", description: "add the correct bin directory to PATH", pr_number: 13946, scopes: ["setup"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "146c129d63587544b85dae542ba84587dd9b8643", date: "2022-08-12 03:46:12 UTC", description: "make integration tests trigger when label is added", pr_number: 13951, scopes: ["ci"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 1, deletions_count: 0},
		{sha: "d40d102de633f2a997d19de0feb1783b70108d98", date: "2022-08-12 22:25:58 UTC", description: "Set up registered internal event structures", pr_number: 13953, scopes: ["core"], type: "chore", breaking_change: false, author: "Bruce Guenter", files_count: 3, insertions_count: 183, deletions_count: 49},
		{sha: "0a95689fff69189a24a02404121e4f7d85b81d3e", date: "2022-08-12 22:14:29 UTC", description: "Regenerate Kubernetes manifests", pr_number: 13958, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 17, insertions_count: 21, deletions_count: 21},
		{sha: "162cf37ac6b1600ee19bf677b1136595c27c727e", date: "2022-08-12 22:51:43 UTC", description: "Fix graceful shutdown behavior", pr_number: 11907, scopes: ["exec source"], type: "fix", breaking_change: false, author: "Jesse Szwedko", files_count: 3, insertions_count: 251, deletions_count: 51},
		{sha: "e1268b1ad376669dc8069d0bf59d34dedc415b2e", date: "2022-08-13 00:54:58 UTC", description: "new `file_descriptor` source", pr_number: 13389, scopes: ["new source"], type: "feat", breaking_change: false, author: "Matt Casper", files_count: 12, insertions_count: 729, deletions_count: 248},
		{sha: "e94fb1eccfd8ed9870eb62e47defc8b65610492b", date: "2022-08-13 05:43:51 UTC", description: "bump clap from 3.2.16 to 3.2.17", pr_number: 13962, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 5, insertions_count: 12, deletions_count: 12},
		{sha: "b79e28ce1abda1730146f11cdcc0736ba6f16f27", date: "2022-08-13 05:44:19 UTC", description: "bump libc from 0.2.129 to 0.2.131", pr_number: 13956, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "64f6724155e34fde78f5c4fdcfef3300b6d02e2a", date: "2022-08-13 05:44:47 UTC", description: "bump memmap2 from 0.5.5 to 0.5.6", pr_number: 13955, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 3, deletions_count: 3},
		{sha: "fb61aff32f27a254b190a27acca568eb613a18b2", date: "2022-08-13 03:04:21 UTC", description: "Remove mention of `coercer` and parser transforms", pr_number: 13966, scopes: [], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 3},
		{sha: "541320071adb0e6e98e8507988623cb049ef7eb7", date: "2022-08-13 03:05:44 UTC", description: "Deprecate `geoip` transform", pr_number: 13964, scopes: ["geoip transform"], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 5, deletions_count: 1},
		{sha: "a4782f83b0fa3ac8d6ca1f56a4990687e338ed06", date: "2022-08-13 07:11:52 UTC", description: "Upgrade to Rust 1.63.0", pr_number: 13947, scopes: [], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 93, insertions_count: 244, deletions_count: 226},
		{sha: "488bccb1a4b7494af0f90072b07395f23c97f26b", date: "2022-08-16 04:26:42 UTC", description: "Add support for Connection-Type databases", pr_number: 13707, scopes: ["geoip enrichment"], type: "enhancement", breaking_change: false, author: "jordan", files_count: 3, insertions_count: 132, deletions_count: 124},
		{sha: "40100a3345dd09ec47e76d1f246b17f53ef88358", date: "2022-08-16 00:14:17 UTC", description: "Install protoc for soak builder too", pr_number: 13967, scopes: [], type: "chore", breaking_change: false, author: "Jesse Szwedko", files_count: 2, insertions_count: 12, deletions_count: 1},
		{sha: "262cc95d149026ae6059e7c5374daba88924f924", date: "2022-08-16 05:44:30 UTC", description: "bump tui from 0.18.0 to 0.19.0", pr_number: 13970, scopes: ["deps"], type: "chore", breaking_change: false, author: "dependabot[bot]", files_count: 2, insertions_count: 5, deletions_count: 21},
		{sha: "39abd94b087344a53fc8db898a07b5f7d3e3a7b3", date: "2022-08-17 02:14:20 UTC", description: "fix counter datapoint aggregation for identical timestamps", pr_number: 13960, scopes: ["datadog_metrics sink"], type: "fix", breaking_change: false, author: "Toby Lawrence", files_count: 5, insertions_count: 442, deletions_count: 37},
		{sha: "177a7c2ff978a7588e8a64e5f191df16e12b87ba", date: "2022-08-17 20:07:47 UTC", description: "Add optional byte offset to file source events", pr_number: 13422, scopes: ["file source"], type: "enhancement", breaking_change: false, author: "Samuel Roberts", files_count: 7, insertions_count: 110, deletions_count: 40},
		{sha: "fd72da488f3e5b0c9455678ee1b694c8192a7aa4", date: "2022-08-18 12:08:09 UTC", description: "add option to set alpn protocols in tls settings", pr_number: 13399, scopes: ["tls"], type: "enhancement", breaking_change: false, author: "Terje Torkelsen", files_count: 4, insertions_count: 67, deletions_count: 3},
		{sha: "800b7cf39ab4ba987bc251e2fa6effd94864e94b", date: "2022-08-24 04:21:11 UTC", description: "announce deprecation of renamed components in 0.24.0", pr_number: 14050, scopes: ["external docs"], type: "chore", breaking_change: false, author: "Toby Lawrence", files_count: 1, insertions_count: 29, deletions_count: 0},
		{sha: "1d0938bd32313ad17b6d72c292d03030caad03e5", date: "2022-08-25 23:21:16 UTC", description: "traces are missing k8s tags", pr_number: 14049, scopes: ["datadog agent source"], type: "fix", breaking_change: false, author: "Kyle Criddle", files_count: 4, insertions_count: 32, deletions_count: 2},
		{sha: "8fafd17069894d415d907d2a3d5e4194466947bc", date: "2022-08-26 03:24:39 UTC", description: "revert `parse_grok` delegation to `parse_groks`", pr_number: 14109, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 2, insertions_count: 205, deletions_count: 146},
		{sha: "309e7186b9719b3c36ed855619371453b130d857", date: "2022-08-26 04:39:44 UTC", description: "add `mod` function, and deprecate the \"%\" operator", pr_number: 14081, scopes: ["vrl"], type: "chore", breaking_change: false, author: "Nathan Fox", files_count: 19, insertions_count: 302, deletions_count: 241},
		{sha: "69fdc086d3557015452ba32e436970912ac194e4", date: "2022-08-18 06:53:30 UTC", description: "Update docker setup docs to include example config", pr_number: 14008, scopes: ["docker platform"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 24, deletions_count: 1},
		{sha: "067177747480a16a99aff6dcf37aae53b3ceb791", date: "2022-08-19 00:09:30 UTC"
			description: "Tweak docker docs for better UX", pr_number: 14016, scopes: [], type:
					"docs", breaking_change:                      false, author: "Spencer Gilbert", files_count: 1, insertions_count: 2, deletions_count: 2
		},
		{sha: "4d43e318e39edaaa6f98ca9602e5c237a9be7d35", date: "2022-08-26 04:11:49 UTC", description: "Typo in VRL encryption function announcement", pr_number: 14112, scopes: [], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 1, deletions_count: 1},
		{sha: "51cb3ec1b43ff73001d8dc97952120ce1694f162", date: "2022-08-27 03:31:37 UTC", description: "Typo in component_received_events_total", pr_number: 14131, scopes: ["internal_metrics source"], type: "docs", breaking_change: false, author: "Jesse Szwedko", files_count: 1, insertions_count: 2, deletions_count: 1},
		{sha: "a467052b7f7c98e8ad7d7cbd8ddb42265042513c", date: "2022-08-30 09:37:35 UTC", description: "update unquoted allowed-characters", pr_number: 14114, scopes: ["vrl"], type: "fix", breaking_change: false, author: "Jean Mertz", files_count: 9, insertions_count: 16, deletions_count: 4},
		{sha: "ebd090e45d4f1d9323ed7f00002ca523dbafbd46", date: "2022-08-30 06:37:57 UTC", description: "Revert halting stream sources on error acknowledgements", pr_number: 14135, scopes: ["sources"], type: "enhancement", breaking_change: false, author: "Bruce Guenter", files_count: 8, insertions_count: 90, deletions_count: 381},

	]
}
