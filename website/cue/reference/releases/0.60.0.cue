package metadata

releases: "0.60.0": {
	date: "2026-10-19"

	changelog: [
		{
			type: "fix"
			description: #"""
				Fixed the `internal_logs` source silently dropping events under high load. Events dropped when the underlying broadcast receiver lags now increment the standard `component_discarded_events_total{intentional="false"}` metric so the loss is observable.
				"""#
			pr_numbers: [25218]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "fix"
			description: #"""
				Fixed a disk buffer bug that could cause Vector to read from the wrong data file after many file rotations. This occurred when a file ID calculation exceeded 65,535 and wrapped incorrectly, even if only a few buffer files existed on disk.
				"""#
			pr_numbers: [25539]
			contributors: ["xfocus3"]
		},
		{
			type: "fix"
			description: #"""
				The `kubernetes_logs` source no longer drops Namespace and Node labels from the Vector log namespace metadata (`%kubernetes_logs.namespace_labels` and `%kubernetes_logs.node_labels`) when `namespace_annotation_fields.namespace_labels` or `node_annotation_fields.node_labels` is set to `""`. An empty path now only suppresses the legacy event field, matching how `pod_annotation_fields` behaves.
				"""#
			pr_numbers: [25970]
			contributors: ["srstrickland"]
		},
		{
			type: "feat"
			description: #"""
				The VRL playground's program editor now provides autocompletion for functions, enum values, event fields and variables, along with signature help while typing function arguments and hover documentation for functions.
				"""#
			pr_numbers: [26545]
			contributors: ["MathieuSlt"]
		},
		{
			type: "fix"
			description: #"""
				Restored Kerberos (`sasl.mechanism: GSSAPI`) support for the `kafka` source and sink in the official `x86_64-unknown-linux-gnu` release artifacts (`.tar.gz`, `.deb`, `.rpm`). Since v0.56.0, these binaries failed with `Cyrus/libsasl2 is missing a GSSAPI module`.
				"""#
			pr_numbers: [26570]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				The Avro codec now supports encoding and decoding Date and TimeMillis values.
				"""#
			pr_numbers: [26112]
			contributors: ["omwbennett"]
		},
		{
			type: "fix"
			description: #"""
				Reuse cached upstream schema definitions when building transforms and validating sinks,
				reducing startup and reload overhead for topologies with shared inputs.
				"""#
			pr_numbers: [26587]
			contributors: ["jh7459-gh"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "Datadog metrics series v1 removed"
			anchor:   "datadog-metrics-series-v1"
			description: #"""
				The `datadog_metrics` sink no longer accepts `series_api_version: v1`, which was
				[deprecated in 0.56.0](https://vector.dev/deprecations/).
				Configurations that select `v1` now fail to load.
				"""#
			pr_numbers: [26558]
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				The Debian package now ships `/etc/vector/vector.yaml` as a dpkg conffile (a stub with no active sources or sinks), so local modifications to it are preserved across package upgrades instead of being unknown to the package manager.
				
				Because the path was not previously a conffile, a file already present there is moved aside before the new conffile is unpacked and restored immediately afterwards, byte-for-byte. Upgrades therefore complete without the interactive "file created by you" conffile prompt that would otherwise break unattended `apt` and `dpkg` runs.
				"""#
			pr_numbers: [26600]
			contributors: ["koenserry"]
		},
		{
			type:     "chore"
			breaking: true
			title:    "InfluxDB sinks require an explicit version"
			anchor:   "influxdb-version-required"
			description: #"""
				The `version` option is now required for the `influxdb_logs` and `influxdb_metrics` sinks.
				Omitting it was [deprecated in 0.58.0](https://vector.dev/deprecations/), and the version is no
				longer inferred from the configured settings. Configurations without `version` now fail to load.
				
				Settings that belong to the other version are now rejected even when set to `null`. For example,
				`version: "2"` with `database: null` fails with `unknown field 'database'`. Remove these settings
				from the configuration.
				"""#
			pr_numbers: [26659]
			contributors: ["JinchengCao"]
		},
		{
			type: "feat"
			description: #"""
				The `kubernetes_logs` source now enriches events with the owning Namespace's annotations, in addition to its labels. Add `namespace_annotation_fields.namespace_annotations` to customize or suppress the output field (default: `.kubernetes.namespace_annotations`).
				"""#
			pr_numbers: [25970]
			contributors: ["srstrickland"]
		},
		{
			type: "enhancement"
			description: #"""
				The `opentelemetry` source now supports `decoding.codec: otlp` and `decoding.signal_types` options.
				This is not a new feature but a replacement for the (now deprecated) `use_otlp_decoding` flag.
				Please see the [dedicated migration notice](https://vector.dev/deprecations/#opentelemetry-use_otlp_decoding).
				"""#
			pr_numbers: [26690]
			contributors: ["pront"]
		},
		{
			type: "fix"
			description: #"""
				Preserve native OTLP log event names in the `opentelemetry` source. Decoded logs expose
				`event_name`, while OTLP decoding preserves `eventName`; no duplicate log attribute is needed.
				"""#
			pr_numbers: [26643]
			contributors: ["bpceee"]
		},
		{
			type: "fix"
			description: #"""
				Fixes an issue where components selected for an explicit reload remained selected for later configuration reloads. This could unnecessarily restart unchanged components and reset their in-memory state.
				"""#
			pr_numbers: [25911]
			contributors: ["arunpidugu"]
		},
		{
			type: "fix"
			description: #"""
				The `prometheus_remote_write` source now interprets metrics ending with _total as counters.
				"""#
			pr_numbers: [26100]
			contributors: ["AyseGokmen"]
		},
		{
			type: "enhancement"
			description: #"""
				Add `internal_metrics.include_group_tag` to the `sample` transform. When enabled, `component_discarded_events_total` includes a `group` tag showing which `group_by` group an event belonged to when it was discarded. This option is disabled by default because configurations with many group values can produce a large number of unique metric tags.
				"""#
			pr_numbers: [26404]
			contributors: ["arun.pidugu"]
		},
		{
			type: "security"
			description: #"""
				Bound the `sample` transform's per-group state with the new `max_state_bytes` option, which defaults to 128 MiB and evicts the least recently used groups to prevent unbounded memory growth from high-cardinality or very large `group_by` values. Each group counts the size of its rendered `group_by` value plus a fixed per-group overhead.
				"""#
			pr_numbers: [26362]
			contributors: ["thomasqueirozb"]
		},
		{
			type: "enhancement"
			description: #"""
				Share the default log schema definition across new events instead of allocating
				a separate copy for each event to improve log event ingestion performance.
				"""#
			pr_numbers: [26712]
			contributors: ["bruceg"]
		},
		{
			type: "fix"
			description: #"""
				Vector now reports the underlying error when a source or sink task fails at runtime; previously, it only said `the task completed with an error`. That includes failures to bind a listening socket (for example `Address already in use (os error 48)`), broker subscribe and consume errors, checkpoint file errors, sink runtime errors, and sources that stop because their downstream channel closed. Vector now includes the panic payload in its logs.
				"""#
			pr_numbers: [26549]
			contributors: ["gwenaskell"]
		},
		{
			type: "enhancement"
			description: #"""
				Add `internal_metrics.include_group_tag` to the `throttle` transform. When enabled, `component_discarded_events_total` includes a `group` tag showing which `key_field` group exceeded the rate limit. This option is disabled by default because configurations with many group values increase metric cardinality.
				"""#
			pr_numbers: [26402]
			contributors: ["arun.pidugu"]
		},
		{
			type: "enhancement"
			description: #"""
				Improve VRL performance when reading variable subpaths and event or metadata fields.
				"""#
			pr_numbers: [26649]
			contributors: ["gwenaskell"]
		},
	]

	vrl_changelog: #"""
		### [0.37.0 (2026-10-19)](https://github.com/vectordotdev/vrl/releases/tag/v0.37.0)
		
		#### Enhancements
		
		- Avoid allocating lookup paths for each event and metadata field read.
		
		*Thanks to [gwenaskell](https://github.com/gwenaskell) for contributing PR [#1987](https://github.com/vectordotdev/vrl/pull/1987)!*
		- VRL variable subpath reads clone only the selected value, reducing the cost of reading fields from large objects and arrays.
		
		*Thanks to [gwenaskell](https://github.com/gwenaskell) for contributing PR [#1984](https://github.com/vectordotdev/vrl/pull/1984)!*
		
		#### Fixes
		
		- The lexer no longer panics on a program with consecutive comment lines inside a block when a
		later comment contains a non-ASCII character, and no longer fails to lex such a program when a
		later comment contains a quote.
		
		*Thanks to [DeviousCardi](https://github.com/DeviousCardi) for contributing PR [#1995](https://github.com/vectordotdev/vrl/pull/1995)!*
		- The `keyvalue` filter in `parse_grok` and `parse_groks` no longer panics on unquoted `NaN` values
		such as `ratio=NaN`. These values are now kept as strings.
		
		*Thanks to [tessneau](https://github.com/tessneau) for contributing PR [#1990](https://github.com/vectordotdev/vrl/pull/1990)!*
		
		"""#
}
